use crate::error::{AppError, AppResult};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCard {
    pub id: String,
    pub name: String,
    pub description: String,
    pub site_url: String,
    pub brand_name: String,
    pub logo_data_url: String,
    pub target_audience: String,
    pub phone: String,
    pub default_article_count: i64,
    pub default_country: String,
    pub default_language: String,
    pub last_activity_at: Option<String>,
    pub default_category_wp_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub wp_status: Option<String>,
    pub campaign_count: i64,
    pub article_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectInput {
    pub name: String,
    pub description: String,
    pub site_url: String,
    pub brand_name: String,
    pub logo_data_url: String,
    pub target_audience: String,
    pub phone: String,
    pub default_article_count: i64,
    pub default_country: String,
    pub default_language: String,
}

impl Default for ProjectInput {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            site_url: String::new(),
            brand_name: String::new(),
            logo_data_url: String::new(),
            target_audience: String::new(),
            phone: String::new(),
            default_article_count: 60,
            default_country: "IR".into(),
            default_language: "fa".into(),
        }
    }
}

const SELECT_CARD: &str = "SELECT p.id, p.name, p.description, p.site_url, p.brand_name, p.logo_data_url, p.target_audience,
  p.default_country, p.default_language, p.phone, p.default_article_count, p.last_activity_at, p.default_category_wp_id, p.created_at, p.updated_at,
  (SELECT w.status FROM wordpress_connections w WHERE w.project_id = p.id) AS wp_status,
  (SELECT COUNT(*) FROM campaigns c WHERE c.project_id = p.id) AS campaign_count,
  (SELECT COUNT(*) FROM article_plans a WHERE a.project_id = p.id) AS article_count
  FROM projects p";

/// Validates and normalizes user input.
pub fn validate(mut i: ProjectInput) -> AppResult<ProjectInput> {
    i.name = i.name.trim().to_string();
    if i.name.is_empty() || i.name.chars().count() > 120 {
        return Err(AppError::validation("نام پروژه الزامی است و باید حداکثر ۱۲۰ نویسه باشد."));
    }
    i.site_url = normalize_site_url(&i.site_url)?;
    i.description = i.description.trim().to_string();
    i.brand_name = i.brand_name.trim().to_string();
    if !i.logo_data_url.is_empty() {
        let (prefix, encoded) = i.logo_data_url.split_once(',').ok_or_else(|| AppError::validation("فرمت لوگو نامعتبر است."))?;
        if !matches!(prefix, "data:image/png;base64" | "data:image/jpeg;base64" | "data:image/webp;base64") || encoded.len() > 2_800_000 {
            return Err(AppError::validation("لوگو باید PNG، JPG یا WebP و حداکثر ۲ مگابایت باشد."));
        }
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).map_err(|_| AppError::validation("داده لوگو نامعتبر است."))?;
        image::load_from_memory(&bytes).map_err(|_| AppError::validation("تصویر لوگو قابل خواندن نیست."))?;
    }
    i.target_audience = i.target_audience.trim().to_string();
    i.phone = i.phone.trim().to_string();
    if !i.phone.chars().all(|c| c.is_ascii_digit() || "+- ()".contains(c)) || i.phone.len() > 32 { return Err(AppError::validation("شماره تماس نامعتبر است.")); }
    if !(1..=200).contains(&i.default_article_count) { return Err(AppError::validation("تعداد مقاله باید بین ۱ تا ۲۰۰۰ باشد.")); }
    i.default_country = i.default_country.trim().to_uppercase();
    if i.default_country.len() != 2 || !i.default_country.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(AppError::validation("کد کشور باید دو حرف باشد (مثلاً IR)."));
    }
    i.default_language = i.default_language.trim().to_lowercase();
    if i.default_language.len() < 2 || i.default_language.len() > 8 {
        return Err(AppError::validation("کد زبان نامعتبر است (مثلاً fa)."));
    }
    Ok(i)
}

pub fn normalize_site_url(raw: &str) -> AppResult<String> {
    let raw = raw.trim();
    let candidate = if raw.contains("://") { raw.to_string() } else { format!("https://{raw}") };
    let u = Url::parse(&candidate).map_err(|_| AppError::validation("آدرس سایت نامعتبر است."))?;
    if !matches!(u.scheme(), "http" | "https") || u.host_str().is_none() {
        return Err(AppError::validation("آدرس سایت باید با http یا https شروع شود."));
    }
    Ok(u.as_str().trim_end_matches('/').to_string())
}

pub async fn list(state: &AppState) -> AppResult<Vec<ProjectCard>> {
    let sql = format!("{SELECT_CARD} ORDER BY p.created_at DESC, p.rowid DESC");
    Ok(sqlx::query_as::<_, ProjectCard>(&sql).fetch_all(&state.db).await?)
}

pub async fn get(state: &AppState, id: &str) -> AppResult<ProjectCard> {
    let sql = format!("{SELECT_CARD} WHERE p.id = ?");
    sqlx::query_as::<_, ProjectCard>(&sql)
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("پروژه پیدا نشد."))
}

pub async fn create(state: &AppState, input: ProjectInput) -> AppResult<ProjectCard> {
    let i = validate(input)?;
    let id = ulid::Ulid::new().to_string();
    sqlx::query(
        "INSERT INTO projects (id, name, description, site_url, brand_name, target_audience, default_country, default_language, phone, default_article_count, logo_data_url)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&i.name)
    .bind(&i.description)
    .bind(&i.site_url)
    .bind(&i.brand_name)
    .bind(&i.target_audience)
    .bind(&i.default_country)
    .bind(&i.default_language)
    .bind(&i.phone)
    .bind(i.default_article_count)
    .bind(&i.logo_data_url)
    .execute(&state.db)
    .await?;
    get(state, &id).await
}

pub async fn update(state: &AppState, id: &str, input: ProjectInput) -> AppResult<ProjectCard> {
    let i = validate(input)?;
    let res = sqlx::query(
        "UPDATE projects SET name=?, description=?, site_url=?, brand_name=?, target_audience=?,
         default_country=?, default_language=?, phone=?, default_article_count=?, logo_data_url=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(&i.name)
    .bind(&i.description)
    .bind(&i.site_url)
    .bind(&i.brand_name)
    .bind(&i.target_audience)
    .bind(&i.default_country)
    .bind(&i.default_language)
    .bind(&i.phone)
    .bind(i.default_article_count)
    .bind(&i.logo_data_url)
    .bind(id)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("پروژه پیدا نشد."));
    }
    get(state, id).await
}

/// Deletes a project and everything that belongs to it (FK cascade) plus its stored secrets.
pub async fn delete(state: &AppState, id: &str, confirm: bool) -> AppResult<()> {
    if !confirm {
        return Err(AppError::validation("حذف پروژه نیاز به تأیید صریح دارد."));
    }
    let secret_ref: Option<(String,)> =
        sqlx::query_as("SELECT secret_ref FROM wordpress_connections WHERE project_id = ?")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let res = sqlx::query("DELETE FROM projects WHERE id = ?").bind(id).execute(&state.db).await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("پروژه پیدا نشد."));
    }
    if let Some((key,)) = secret_ref {
        state.secrets.delete(&key)?;
    }
    Ok(())
}
