use crate::services::qc::count_phrase;
use crate::error::{AppError, AppResult};
use crate::services::{ai, campaigns, logs};
use crate::state::AppState;
use crate::util::json::string_items;
use crate::util::text::{language_name, normalize};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

const BATCH: usize = 10;
const MAX_TITLE_CHARS: usize = 200;

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PlanView {
    pub plan_id: String,
    pub article_no: i64,
    pub keyword_id: String,
    pub keyword: String,
    pub intent: Option<String>,
    pub status: String,
    pub title_id: Option<String>,
    pub title: Option<String>,
    pub title_state: Option<String>,
    pub title_origin: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanList {
    pub plans: Vec<PlanView>,
    pub total: i64,
    pub approved: i64,
    pub pending: i64,
    pub all_approved: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TitlesResult {
    pub generated: usize,
    pub missing: Vec<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TitleView {
    pub id: String,
    pub title: String,
    pub state: String,
    pub origin: String,
}

pub async fn list(state: &AppState, campaign_id: &str) -> AppResult<PlanList> {
    let plans = sqlx::query_as::<_, PlanView>(
        "SELECT p.id AS plan_id, p.article_no, p.keyword_id, k.keyword, COALESCE(p.intent, k.intent) AS intent, p.status,
                t.id AS title_id, t.title, t.state AS title_state, t.origin AS title_origin
         FROM article_plans p
         JOIN keywords k ON k.id = p.keyword_id
         LEFT JOIN article_titles t ON t.id = (
            SELECT id FROM article_titles WHERE plan_id = p.id AND state IN ('APPROVED','SUGGESTED')
            ORDER BY (state='APPROVED') DESC, rowid DESC LIMIT 1)
         WHERE p.campaign_id = ? ORDER BY p.article_no",
    )
    .bind(campaign_id)
    .fetch_all(&state.db)
    .await?;
    let total = plans.len() as i64;
    let approved = plans.iter().filter(|p| p.title_state.as_deref() == Some("APPROVED")).count() as i64;
    Ok(PlanList { plans, total, approved, pending: total - approved, all_approved: total > 0 && approved == total })
}

async fn active_title_norms(state: &AppState, campaign_id: &str) -> AppResult<HashSet<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT t.title FROM article_titles t JOIN article_plans p ON p.id=t.plan_id
         WHERE p.campaign_id=? AND t.state IN ('APPROVED','SUGGESTED')",
    )
    .bind(campaign_id)
    .fetch_all(&state.db)
    .await?;
    Ok(rows.into_iter().map(|r| normalize(&r.0)).collect())
}

fn title_prompt_style(language: &str) -> String {
    format!(
        "Write natural, engaging, SEO-friendly titles in {}. Prefer a question that matches the reader's search intent. \
         Every title MUST contain its keyword phrase exactly as given, complete and unchanged (never split or reworded). \
         Where it fits naturally, use a number in the title (for example \"۵ روش ...\"). \
         Each title must be unique across the whole list and at most 90 characters.",
        language_name(language)
    )
}

/// Generates a title for every plan that has no active (suggested/approved) title yet.
pub async fn generate(state: &AppState, campaign_id: &str) -> AppResult<TitlesResult> {
    let c = campaigns::get(state, campaign_id).await?;
    let provider = campaigns::require_ai_provider(&c)?;
    let pending: Vec<(String, i64, String, Option<String>)> = sqlx::query_as(
        "SELECT p.id, p.article_no, k.keyword, COALESCE(p.intent, k.intent)
         FROM article_plans p JOIN keywords k ON k.id=p.keyword_id
         WHERE p.campaign_id=? AND NOT EXISTS (SELECT 1 FROM article_titles t WHERE t.plan_id=p.id AND t.state IN ('APPROVED','SUGGESTED'))
         ORDER BY p.article_no",
    )
    .bind(campaign_id)
    .fetch_all(&state.db)
    .await?;
    if pending.is_empty() {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM article_plans WHERE campaign_id=?")
            .bind(campaign_id)
            .fetch_one(&state.db)
            .await?;
        if n == 0 {
            return Err(AppError::validation("ابتدا کلمات کلیدی ثانویه را تأیید و برنامه‌ی مقالات را بسازید."));
        }
    }

    let mut used = active_title_norms(state, campaign_id).await?;
    let mut generated = 0usize;
    let mut missing: Vec<i64> = Vec::new();

    for chunk in pending.chunks(BATCH) {
        let items: Vec<Value> = chunk
            .iter()
            .map(|(_, no, kw, intent)| serde_json::json!({"no": no, "keyword": kw, "intent": intent}))
            .collect();
        let system = "You are an SEO editor. Reply with ONLY valid JSON, no commentary.";
        let user = format!(
            "{style}\n\nItems: {items}\n\nWrite exactly one title per item. Return a JSON array: [{{\"no\": <number>, \"title\": \"...\"}}]",
            style = title_prompt_style(&c.language),
            items = serde_json::to_string(&items).unwrap_or_default(),
        );
        let v = match ai::complete_json(state, &provider, system, &user, Some(2500)).await {
            Ok(v) => v,
            Err(e) => {
                logs::record(state, "ERROR", Some(&c.project_id), Some(&c.id), "generate_titles", "FAILED", &e.message).await;
                if generated == 0 {
                    return Err(e);
                }
                missing.extend(chunk.iter().map(|x| x.1));
                continue;
            }
        };
        let arr = match &v {
            Value::Array(a) => a.clone(),
            Value::Object(m) => m.values().find_map(|x| x.as_array().cloned()).unwrap_or_default(),
            _ => Vec::new(),
        };
        for (plan_id, no, kw, _) in chunk {
            let title = arr.iter().find_map(|it| {
                let n = it.get("no").and_then(|x| x.as_i64().or_else(|| x.as_str().and_then(|s| s.parse().ok())))?;
                if n != *no {
                    return None;
                }
                it.get("title").and_then(|t| t.as_str()).map(|s| s.trim().to_string())
            });
            match title {
                Some(t) if !t.is_empty() && t.chars().count() <= MAX_TITLE_CHARS && count_phrase(&t, kw) > 0 && used.insert(normalize(&t)) => {
                    insert_suggested(state, plan_id, &t).await?;
                    generated += 1;
                }
                _ => missing.push(*no),
            }
        }
    }
    logs::record(state, if missing.is_empty() { "INFO" } else { "WARN" }, Some(&c.project_id), Some(&c.id),
        "generate_titles", if missing.is_empty() { "SUCCESS" } else { "WARN" },
        &format!("generated={} missing={}", generated, missing.len())).await;
    campaigns::set_step(state, campaign_id, 8).await?;
    Ok(TitlesResult { generated, missing })
}

async fn insert_suggested(state: &AppState, plan_id: &str, title: &str) -> AppResult<String> {
    let id = ulid::Ulid::new().to_string();
    sqlx::query("INSERT INTO article_titles (id, plan_id, title, origin, state) VALUES (?, ?, ?, 'ai', 'SUGGESTED')")
        .bind(&id)
        .bind(plan_id)
        .bind(title)
        .execute(&state.db)
        .await?;
    Ok(id)
}

/// Returns (campaign_id, project_id). Titles can only change before content generation has started.
async fn plan_campaign(state: &AppState, plan_id: &str) -> AppResult<(String, String)> {
    let row = sqlx::query_as::<_, (String, String, String)>("SELECT campaign_id, project_id, status FROM article_plans WHERE id=?")
        .bind(plan_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("مقاله پیدا نشد."))?;
    if !matches!(row.2.as_str(), "PLANNED" | "TITLE_APPROVED") {
        return Err(AppError::conflict("پس از شروع تولید، عنوان این مقاله قابل تغییر نیست."));
    }
    Ok((row.0, row.1))
}

pub async fn approve(state: &AppState, plan_id: &str, title_id: &str) -> AppResult<()> {
    plan_campaign(state, plan_id).await?;
    let mut tx = state.db.begin().await?;
    let exists: Option<(String,)> =
        sqlx::query_as("SELECT id FROM article_titles WHERE id=? AND plan_id=? AND state IN ('SUGGESTED','APPROVED')")
            .bind(title_id)
            .bind(plan_id)
            .fetch_optional(&mut *tx)
            .await?;
    if exists.is_none() {
        return Err(AppError::not_found("عنوان پیدا نشد یا قبلاً رد شده است."));
    }
    sqlx::query("UPDATE article_titles SET state='SUPERSEDED' WHERE plan_id=? AND state='APPROVED' AND id<>?")
        .bind(plan_id)
        .bind(title_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE article_titles SET state='APPROVED' WHERE id=?").bind(title_id).execute(&mut *tx).await?;
    sqlx::query("UPDATE article_plans SET status='TITLE_APPROVED', updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?")
        .bind(plan_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn edit(state: &AppState, plan_id: &str, title: &str) -> AppResult<TitleView> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS {
        return Err(AppError::validation("عنوان باید بین ۱ تا ۲۰۰ نویسه باشد."));
    }
    plan_campaign(state, plan_id).await?;
    let (keyword,): (String,) = sqlx::query_as("SELECT k.keyword FROM article_plans p JOIN keywords k ON k.id=p.keyword_id WHERE p.id=?")
        .bind(plan_id)
        .fetch_one(&state.db)
        .await?;
    if count_phrase(title, &keyword) == 0 {
        return Err(AppError::validation(format!("عنوان باید شامل کلمه‌ی کلیدی «{keyword}» باشد.")));
    }
    let id = ulid::Ulid::new().to_string();
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE article_titles SET state='SUPERSEDED' WHERE plan_id=? AND state IN ('APPROVED','SUGGESTED')")
        .bind(plan_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO article_titles (id, plan_id, title, origin, state) VALUES (?, ?, ?, 'manual', 'APPROVED')")
        .bind(&id)
        .bind(plan_id)
        .bind(title)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE article_plans SET status='TITLE_APPROVED', updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?")
        .bind(plan_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(TitleView { id, title: title.to_string(), state: "APPROVED".into(), origin: "manual".into() })
}

pub async fn reject(state: &AppState, plan_id: &str, title_id: &str) -> AppResult<()> {
    plan_campaign(state, plan_id).await?;
    let mut tx = state.db.begin().await?;
    let r = sqlx::query("UPDATE article_titles SET state='REJECTED' WHERE id=? AND plan_id=? AND state IN ('SUGGESTED','APPROVED')")
        .bind(title_id)
        .bind(plan_id)
        .execute(&mut *tx)
        .await?;
    if r.rows_affected() == 0 {
        return Err(AppError::not_found("عنوان پیدا نشد."));
    }
    sqlx::query("UPDATE article_plans SET status='PLANNED', updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND NOT EXISTS (SELECT 1 FROM article_titles WHERE plan_id=? AND state='APPROVED')")
        .bind(plan_id)
        .bind(plan_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Asks the AI for a different title for one article. Repeatable until the user approves one.
pub async fn alternative(state: &AppState, plan_id: &str) -> AppResult<TitleView> {
    let (campaign_id, _) = plan_campaign(state, plan_id).await?;
    let c = campaigns::get(state, &campaign_id).await?;
    let provider = campaigns::require_ai_provider(&c)?;
    let (keyword, intent): (String, Option<String>) = sqlx::query_as(
        "SELECT k.keyword, COALESCE(p.intent, k.intent) FROM article_plans p JOIN keywords k ON k.id=p.keyword_id WHERE p.id=?",
    )
    .bind(plan_id)
    .fetch_one(&state.db)
    .await?;
    let prev: Vec<(String,)> = sqlx::query_as("SELECT title FROM article_titles WHERE plan_id=? ORDER BY rowid")
        .bind(plan_id)
        .fetch_all(&state.db)
        .await?;
    let mut avoid: Vec<String> = prev.into_iter().map(|p| p.0).collect();
    let mut used = active_title_norms(state, &campaign_id).await?;
    used.extend(avoid.iter().map(|t| normalize(t)));

    for _ in 0..3 {
        let system = "You are an SEO editor. Reply with ONLY valid JSON, no commentary.";
        let user = format!(
            "{style}\n\nKeyword: {keyword}\nIntent: {intent}\nTitles already tried (must be clearly different from all of them): {avoid}\n\
             Return a JSON array with exactly one string: [\"new title\"]",
            style = title_prompt_style(&c.language),
            intent = intent.clone().unwrap_or_default(),
            avoid = serde_json::to_string(&avoid).unwrap_or_default(),
        );
        let v = ai::complete_json(state, &provider, system, &user, Some(400)).await?;
        let candidate = string_items(&v, &["title", "text"], "x").into_iter().next().map(|x| x.0);
        if let Some(t) = candidate {
            if !t.is_empty() && t.chars().count() <= MAX_TITLE_CHARS && count_phrase(&t, &keyword) > 0 && !used.contains(&normalize(&t)) {
                let mut tx = state.db.begin().await?;
                // The previously shown title is replaced; an approved one goes back to "pending".
                sqlx::query("UPDATE article_titles SET state='REJECTED' WHERE plan_id=? AND state='SUGGESTED'")
                    .bind(plan_id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("UPDATE article_titles SET state='SUPERSEDED' WHERE plan_id=? AND state='APPROVED'")
                    .bind(plan_id)
                    .execute(&mut *tx)
                    .await?;
                let id = ulid::Ulid::new().to_string();
                sqlx::query("INSERT INTO article_titles (id, plan_id, title, origin, state) VALUES (?, ?, ?, 'ai', 'SUGGESTED')")
                    .bind(&id)
                    .bind(plan_id)
                    .bind(&t)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("UPDATE article_plans SET status='PLANNED', updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?")
                    .bind(plan_id)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                return Ok(TitleView { id, title: t, state: "SUGGESTED".into(), origin: "ai".into() });
            }
            avoid.push(t);
        }
    }
    Err(AppError::from_parts("INVALID_RESPONSE", "عنوان جدید و متفاوتی تولید نشد؛ دوباره تلاش کنید.".into(), true))
}

pub async fn approve_all(state: &AppState, campaign_id: &str) -> AppResult<i64> {
    campaigns::get(state, campaign_id).await?;
    let mut tx = state.db.begin().await?;
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT p.id, t.id FROM article_plans p JOIN article_titles t ON t.plan_id=p.id
         WHERE p.campaign_id=? AND p.status='PLANNED' AND t.state='SUGGESTED'
           AND t.rowid = (SELECT MAX(rowid) FROM article_titles WHERE plan_id=p.id AND state='SUGGESTED')",
    )
    .bind(campaign_id)
    .fetch_all(&mut *tx)
    .await?;
    for (plan_id, title_id) in &rows {
        sqlx::query("UPDATE article_titles SET state='APPROVED' WHERE id=?").bind(title_id).execute(&mut *tx).await?;
        sqlx::query("UPDATE article_plans SET status='TITLE_APPROVED', updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?")
            .bind(plan_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(rows.len() as i64)
}
