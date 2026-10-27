use crate::error::{AppError, AppResult};
use crate::state::AppState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CampaignView {
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub name: String,
    pub target_count: i64,
    pub status: String,
    pub wizard_step: i64,
    pub country: String,
    pub language: String,
    pub ai_provider_id: Option<String>,
    pub image_provider_id: Option<String>,
    pub template_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub primary_count: i64,
    pub candidate_count: i64,
    pub selected_count: i64,
    pub secondary_count: i64,
    pub plan_count: i64,
    pub approved_count: i64,
    /// Articles whose text and images are ready (or already drafted in WordPress).
    pub complete_count: i64,
    /// `campaign` or `single` (single-article tab).
    pub kind: String,
    /// Images per article for this campaign; `None` uses the template default.
    pub image_count: Option<i64>,
    pub external_url: Option<String>,
    pub external_title: Option<String>,
    pub external_follow: i64,
    pub cta_text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCampaign {
    pub project_id: String,
    pub name: String,
    pub target_count: Option<i64>,
    pub country: Option<String>,
    pub language: Option<String>,
    pub ai_provider_id: Option<String>,
}

const SELECT: &str = "SELECT c.id, c.project_id, p.name AS project_name, c.name, c.target_count, c.status, c.wizard_step,
  c.country, c.language, c.ai_provider_id, c.image_provider_id, c.template_id, c.created_at, c.updated_at,
  (SELECT COUNT(*) FROM keywords k WHERE k.campaign_id=c.id AND k.role='primary') AS primary_count,
  (SELECT COUNT(*) FROM keywords k WHERE k.campaign_id=c.id AND k.role='candidate') AS candidate_count,
  (SELECT COUNT(*) FROM keywords k WHERE k.campaign_id=c.id AND k.role='candidate' AND k.is_selected=1) AS selected_count,
  (SELECT COUNT(*) FROM keywords k WHERE k.campaign_id=c.id AND k.role='secondary') AS secondary_count,
  (SELECT COUNT(*) FROM article_plans a WHERE a.campaign_id=c.id) AS plan_count,
  (SELECT COUNT(*) FROM article_plans a WHERE a.campaign_id=c.id AND EXISTS (SELECT 1 FROM article_titles t WHERE t.plan_id=a.id AND t.state='APPROVED')) AS approved_count,
  (SELECT COUNT(*) FROM article_plans a WHERE a.campaign_id=c.id AND a.status IN ('READY_TO_UPLOAD','UPLOADING','DRAFT_CREATED')) AS complete_count,
  c.kind, c.image_count, c.external_url, c.external_title, c.external_follow, c.cta_text
  FROM campaigns c JOIN projects p ON p.id = c.project_id";

pub async fn list(state: &AppState, project_id: Option<&str>) -> AppResult<Vec<CampaignView>> {
    let rows = match project_id {
        Some(pid) => {
            sqlx::query_as::<_, CampaignView>(&format!("{SELECT} WHERE c.project_id = ? AND c.kind = 'campaign' ORDER BY c.created_at DESC, c.rowid DESC"))
                .bind(pid)
                .fetch_all(&state.db)
                .await?
        }
        None => {
            sqlx::query_as::<_, CampaignView>(&format!("{SELECT} WHERE c.kind = 'campaign' ORDER BY c.created_at DESC, c.rowid DESC"))
                .fetch_all(&state.db)
                .await?
        }
    };
    Ok(rows)
}

pub async fn get(state: &AppState, id: &str) -> AppResult<CampaignView> {
    // Campaigns created before activation get the fixed providers the first time they are opened.
    let _ = super::license::attach_fixed_defaults(state).await;
    sqlx::query_as::<_, CampaignView>(&format!("{SELECT} WHERE c.id = ?"))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("کمپین پیدا نشد."))
}

pub async fn create(state: &AppState, input: CreateCampaign) -> AppResult<CampaignView> {
    let name = input.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 160 {
        return Err(AppError::validation("نام کمپین الزامی است و باید حداکثر ۱۶۰ نویسه باشد."));
    }
    let proj: Option<(String, String, Option<String>, i64)> =
        sqlx::query_as("SELECT default_country, default_language, ai_provider_id, default_article_count FROM projects WHERE id = ?")
            .bind(&input.project_id)
            .fetch_optional(&state.db)
            .await?;
    let (pc, pl, pai, default_count) = proj.ok_or_else(|| AppError::not_found("پروژه پیدا نشد."))?;
    let requested = input.target_count.unwrap_or(default_count);
    if !(1..=200).contains(&requested) {
        return Err(AppError::validation("تعداد مقالات باید بین ۱ تا ۲۰۰ باشد."));
    }
    // The license quota caps every campaign: articles already written or promised elsewhere are not reusable.
    let available = super::quota::article_available(state).await?;
    if available <= 0 {
        return Err(AppError::validation("سهمیه‌ی مقالات این لایسنس تمام شده است؛ با لایسنس جدید ادامه دهید."));
    }
    let target = requested.min(available);
    let epoch = super::quota::current_epoch(state).await?;
    let country = input.country.map(|c| c.trim().to_uppercase()).filter(|c| !c.is_empty()).unwrap_or(pc);
    let language = input.language.map(|c| c.trim().to_lowercase()).filter(|c| !c.is_empty()).unwrap_or(pl);
    // Fixed built-in providers are the defaults, but only once the license has created them (FK).
    let builtin_text: Option<(String,)> = sqlx::query_as("SELECT id FROM ai_providers WHERE id = ?")
        .bind(super::license::TEXT_PROVIDER_ID)
        .fetch_optional(&state.db)
        .await?;
    let builtin_image: Option<(String,)> = sqlx::query_as("SELECT id FROM image_providers WHERE id = ?")
        .bind(super::license::IMAGE_PROVIDER_ID)
        .fetch_optional(&state.db)
        .await?;
    let ai = input.ai_provider_id.filter(|s| !s.is_empty()).or(pai).or(builtin_text.map(|r| r.0));
    let id = ulid::Ulid::new().to_string();
    sqlx::query(
        "INSERT INTO campaigns (id, project_id, name, target_count, country, language, ai_provider_id, wizard_step, quota_epoch)
         VALUES (?, ?, ?, ?, ?, ?, ?, 2, ?)",
    )
    .bind(&id)
    .bind(&input.project_id)
    .bind(&name)
    .bind(target)
    .bind(&country)
    .bind(&language)
    .bind(&ai)
    .bind(epoch)
    .execute(&state.db)
    .await?;
    if let Some((image_id,)) = builtin_image {
        sqlx::query("UPDATE campaigns SET image_provider_id=? WHERE id=?").bind(image_id).bind(&id).execute(&state.db).await?;
    }
    sqlx::query("UPDATE campaigns SET template_id=? WHERE id=?").bind(super::templates::ARTICLE_TEMPLATE_ID).bind(&id).execute(&state.db).await?;
    touch_project(state, &input.project_id).await;
    get(state, &id).await
}

pub async fn delete(state: &AppState, id: &str, confirm: bool) -> AppResult<()> {
    if !confirm {
        return Err(AppError::validation("حذف کمپین نیاز به تأیید صریح دارد."));
    }
    let res = sqlx::query("DELETE FROM campaigns WHERE id = ?").bind(id).execute(&state.db).await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("کمپین پیدا نشد."));
    }
    Ok(())
}

pub async fn save_wizard_state(state: &AppState, id: &str, step: i64, data: serde_json::Value) -> AppResult<()> {
    if !(1..=14).contains(&step) {
        return Err(AppError::validation("شماره‌ی مرحله نامعتبر است."));
    }
    let res = sqlx::query(
        "UPDATE campaigns SET wizard_step=?, wizard_state_json=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(step)
    .bind(data.to_string())
    .bind(id)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("کمپین پیدا نشد."));
    }
    Ok(())
}

pub async fn set_ai_provider(state: &AppState, id: &str, provider_id: Option<String>) -> AppResult<CampaignView> {
    let pid = provider_id.filter(|s| !s.is_empty());
    let res = sqlx::query("UPDATE campaigns SET ai_provider_id=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?")
        .bind(&pid)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| AppError::validation("ارائه‌دهنده‌ی AI نامعتبر است."))?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("کمپین پیدا نشد."));
    }
    get(state, id).await
}

pub async fn set_template(state: &AppState, id: &str, template_id: Option<String>) -> AppResult<CampaignView> {
    let tid = template_id.filter(|s| !s.is_empty());
    let res = sqlx::query("UPDATE campaigns SET template_id=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?")
        .bind(&tid)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| AppError::validation("قالب نامعتبر است."))?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("کمپین پیدا نشد."));
    }
    get(state, id).await
}

pub async fn set_image_provider(state: &AppState, id: &str, provider_id: Option<String>) -> AppResult<CampaignView> {
    let pid = provider_id.filter(|s| !s.is_empty());
    let res = sqlx::query("UPDATE campaigns SET image_provider_id=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?")
        .bind(&pid)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| AppError::validation("ارائه‌دهنده‌ی تصویر نامعتبر است."))?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("کمپین پیدا نشد."));
    }
    get(state, id).await
}

pub async fn set_step(state: &AppState, id: &str, step: i64) -> AppResult<()> {
    sqlx::query("UPDATE campaigns SET wizard_step = MAX(wizard_step, ?), updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?")
        .bind(step)
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(())
}

pub async fn touch_project(state: &AppState, project_id: &str) {
    let _ = sqlx::query("UPDATE projects SET last_activity_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?")
        .bind(project_id)
        .execute(&state.db)
        .await;
}

/// Returns the AI provider id a campaign should use, or a friendly validation error.
pub fn require_ai_provider(campaign: &CampaignView) -> AppResult<String> {
    campaign
        .ai_provider_id
        .clone()
        .ok_or_else(|| AppError::validation("لایسنس را وارد کنید."))
}
