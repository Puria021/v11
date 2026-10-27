//! Single-article tab: a campaign of exactly one article with its own image count, optional external link,
//! follow/nofollow choice and optional CTA. It reuses the campaign machinery (titles, generation, upload).

use crate::error::{AppError, AppResult};
use crate::services::campaigns::{self, CampaignView, CreateCampaign};
use crate::services::keywords::{insert_keyword, NewKeyword};
use crate::services::ai;
use crate::state::AppState;
use crate::util::text::language_name;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SingleRow {
    pub campaign_id: String,
    pub plan_id: String,
    pub keyword: String,
    pub title: Option<String>,
    pub status: String,
    pub wp_status: Option<String>,
    pub word_count: Option<i64>,
    pub created_at: String,
}

/// Single articles, newest first: in progress or finished.
pub async fn list(state: &AppState) -> AppResult<Vec<SingleRow>> {
    let rows = sqlx::query_as::<_, SingleRow>(
        "SELECT c.id AS campaign_id, p.id AS plan_id, k.keyword,
                (SELECT t.title FROM article_titles t WHERE t.plan_id=p.id AND t.state='APPROVED') AS title,
                p.status, w.wp_status, ac.word_count, c.created_at
         FROM campaigns c
         JOIN article_plans p ON p.campaign_id=c.id
         JOIN keywords k ON k.id=p.keyword_id
         LEFT JOIN article_contents ac ON ac.plan_id=p.id
         LEFT JOIN wordpress_posts w ON w.plan_id=p.id
         WHERE c.kind='single'
         ORDER BY c.created_at DESC, c.rowid DESC
         LIMIT 100",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(rows)
}

const MAX_TITLE_CHARS: usize = 200;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SingleInput {
    pub project_id: String,
    pub keyword: String,
    pub image_count: i64,
    pub external_url: Option<String>,
    pub external_title: Option<String>,
    pub external_follow: bool,
    pub cta: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SingleCreated {
    pub campaign_id: String,
    pub plan_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleOption {
    pub id: String,
    pub title: String,
}

pub async fn create(state: &AppState, input: SingleInput) -> AppResult<SingleCreated> {
    let keyword = input.keyword.trim().to_string();
    if keyword.is_empty() || keyword.chars().count() > 200 {
        return Err(AppError::validation("کلمه‌ی کلیدی الزامی است و باید حداکثر ۲۰۰ نویسه باشد."));
    }
    if !(1..=4).contains(&input.image_count) {
        return Err(AppError::validation("تعداد تصاویر باید بین ۱ تا ۴ باشد."));
    }
    let external_url = input.external_url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty());
    if let Some(u) = &external_url {
        let parsed = url::Url::parse(u).map_err(|_| AppError::validation("آدرس لینک خارجی معتبر نیست."))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(AppError::validation("آدرس لینک خارجی باید با http یا https شروع شود."));
        }
    }
    let external_title = input.external_title.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    if external_title.as_ref().is_some_and(|t| t.chars().count() > MAX_TITLE_CHARS) {
        return Err(AppError::validation("عنوان لینک خارجی حداکثر ۲۰۰ نویسه است."));
    }
    let cta = input.cta.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());

    let campaign = campaigns::create(
        state,
        CreateCampaign { project_id: input.project_id.clone(), name: keyword.clone(), target_count: Some(1), country: None, language: None, ai_provider_id: None },
    )
    .await?;
    sqlx::query(
        "UPDATE campaigns SET kind='single', image_count=?, external_url=?, external_title=?, external_follow=?, cta_text=? WHERE id=?",
    )
    .bind(input.image_count)
    .bind(&external_url)
    .bind(&external_title)
    .bind(input.external_follow as i64)
    .bind(&cta)
    .bind(&campaign.id)
    .execute(&state.db)
    .await?;

    let mut tx = state.db.begin().await?;
    insert_keyword(&mut tx, &campaign, NewKeyword::plain(&keyword, "primary", "user", None)).await?;
    insert_keyword(&mut tx, &campaign, NewKeyword::plain(&keyword, "secondary", "user", None)).await?;
    let (kid,): (String,) = sqlx::query_as("SELECT id FROM keywords WHERE campaign_id=? AND role='secondary' LIMIT 1")
        .bind(&campaign.id)
        .fetch_one(&mut *tx)
        .await?;
    let plan_id = ulid::Ulid::new().to_string();
    sqlx::query("INSERT INTO article_plans (id, project_id, campaign_id, article_no, keyword_id) VALUES (?, ?, ?, 1, ?)")
        .bind(&plan_id)
        .bind(&input.project_id)
        .bind(&campaign.id)
        .bind(&kid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(SingleCreated { campaign_id: campaign.id, plan_id })
}

/// Asks the AI for three titles for the single article's keyword. Previous suggestions are superseded.
pub async fn suggest_titles(state: &AppState, campaign_id: &str) -> AppResult<Vec<TitleOption>> {
    let c: CampaignView = campaigns::get(state, campaign_id).await?;
    if c.kind != "single" {
        return Err(AppError::validation("این کمپین مربوط به تولید مقاله‌ی تکی نیست."));
    }
    let provider = campaigns::require_ai_provider(&c)?;
    let (plan_id, keyword): (String, String) = sqlx::query_as(
        "SELECT p.id, k.keyword FROM article_plans p JOIN keywords k ON k.id = p.keyword_id WHERE p.campaign_id=? LIMIT 1",
    )
    .bind(campaign_id)
    .fetch_one(&state.db)
    .await?;

    let system = "You are an SEO editor. Reply with ONLY valid JSON, no commentary.";
    let user = format!(
        "Language: {lang}. Focus keyword: {keyword}\n\
         Suggest exactly 3 distinct, click-worthy article titles that naturally contain the focus keyword. \
         Each title must be at most 80 characters, different in angle, and must not promise numbers or facts you cannot support.\n\
         Return JSON: {{\"titles\":[\"...\",\"...\",\"...\"]}}",
        lang = language_name(&c.language),
    );
    let v = ai::complete_json(state, &provider, system, &user, Some(800)).await?;
    let list: Vec<String> = v
        .get("titles")
        .and_then(|t| t.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty() && s.chars().count() <= MAX_TITLE_CHARS && crate::services::qc::count_phrase(s, &keyword) > 0).collect())
        .unwrap_or_default();
    if list.is_empty() {
        return Err(AppError::validation("AI عنوانی که شامل کلمه‌ی کلیدی باشد پیشنهاد نداد؛ دوباره تلاش کنید یا عنوان دستی وارد کنید."));
    }

    sqlx::query("UPDATE article_titles SET state='SUPERSEDED' WHERE plan_id=? AND state='SUGGESTED'")
        .bind(&plan_id)
        .execute(&state.db)
        .await?;
    let mut out = Vec::new();
    for title in list.into_iter().take(3) {
        let id = ulid::Ulid::new().to_string();
        sqlx::query("INSERT INTO article_titles (id, plan_id, title, origin, state) VALUES (?, ?, ?, 'ai', 'SUGGESTED')")
            .bind(&id)
            .bind(&plan_id)
            .bind(&title)
            .execute(&state.db)
            .await?;
        out.push(TitleOption { id, title });
    }
    Ok(out)
}
