use crate::error::{AppError, AppResult};
use crate::providers::image::openai_image::OpenAiImage;
use crate::providers::image::{GeneratedImage, ImageProvider};
use crate::providers::{ProviderError, ProviderErrorKind};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use std::time::Duration;
use url::Url;

const OPENAI_BASE: &str = "https://api.openai.com/v1";

#[derive(Debug, sqlx::FromRow)]
struct Row {
    id: String,
    name: String,
    kind: String,
    base_url: Option<String>,
    model: String,
    output_format: String,
    image_quality: String,
    preferred_size: String,
    timeout_sec: i64,
    proxy_url: Option<String>,
    secret_ref: Option<String>,
    rpm_limit: Option<i64>,
    max_concurrent: i64,
}

const COLS: &str = "id, name, kind, base_url, model, output_format, image_quality, preferred_size, timeout_sec, proxy_url, secret_ref, rpm_limit, max_concurrent";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageProviderView {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: Option<String>,
    pub model: String,
    pub output_format: String,
    pub image_quality: String,
    pub preferred_size: String,
    pub timeout_sec: i64,
    pub proxy_url: Option<String>,
    pub rpm_limit: Option<i64>,
    pub max_concurrent: i64,
    pub has_secret: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveImageProvider {
    pub id: Option<String>,
    pub name: String,
    pub kind: String,
    pub base_url: Option<String>,
    pub model: String,
    pub output_format: Option<String>,
    pub image_quality: Option<String>,
    pub preferred_size: Option<String>,
    pub timeout_sec: Option<i64>,
    pub proxy_url: Option<String>,
    pub rpm_limit: Option<i64>,
    pub max_concurrent: Option<i64>,
    pub api_key: Option<String>,
}

fn secret_key(id: &str) -> String {
    format!("image-key:{id}")
}

fn view(state: &AppState, r: Row) -> AppResult<ImageProviderView> {
    let has_secret = match &r.secret_ref {
        Some(k) => state.secrets.get(k)?.is_some(),
        None => false,
    };
    Ok(ImageProviderView {
        id: r.id,
        name: r.name,
        kind: r.kind,
        base_url: r.base_url,
        model: r.model,
        output_format: r.output_format,
        image_quality: r.image_quality,
        preferred_size: r.preferred_size,
        timeout_sec: r.timeout_sec,
        proxy_url: r.proxy_url,
        rpm_limit: r.rpm_limit,
        max_concurrent: r.max_concurrent,
        has_secret,
    })
}

pub async fn list(state: &AppState) -> AppResult<Vec<ImageProviderView>> {
    let rows = sqlx::query_as::<_, Row>(&format!("SELECT {COLS} FROM image_providers WHERE id NOT IN ('builtin-text','builtin-image') ORDER BY created_at, rowid"))
        .fetch_all(&state.db)
        .await?;
    rows.into_iter().map(|r| view(state, r)).collect()
}

pub async fn save(state: &AppState, input: SaveImageProvider) -> AppResult<ImageProviderView> {
    if input.id.as_deref().map(super::license::is_builtin).unwrap_or(false) {
        return Err(AppError::validation("این ارائه‌دهنده توسط برنامه مدیریت می‌شود."));
    }
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::validation("نام ارائه‌دهنده الزامی است."));
    }
    if input.kind != "openai_image" {
        return Err(AppError::validation("فعلاً فقط سرویس‌های سازگار با OpenAI Images پشتیبانی می‌شوند."));
    }
    let model = input.model.trim().to_string();
    if model.is_empty() {
        return Err(AppError::validation("نام مدل الزامی است."));
    }
    let base_url = match input.base_url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(u) => {
            let p = Url::parse(u).map_err(|_| AppError::validation("Base URL نامعتبر است."))?;
            if !matches!(p.scheme(), "http" | "https") {
                return Err(AppError::validation("Base URL باید با http یا https شروع شود."));
            }
            Some(u.trim_end_matches('/').to_string())
        }
        None => None,
    };
    let output_format = input.output_format.as_deref().unwrap_or("png");
    if !["png", "jpeg", "webp"].contains(&output_format) {
        return Err(AppError::validation("فرمت تصویر باید png، jpeg یا webp باشد."));
    }
    let image_quality = input.image_quality.as_deref().unwrap_or("auto");
    if !["auto", "low", "medium", "high"].contains(&image_quality) {
        return Err(AppError::validation("کیفیت تصویر نامعتبر است."));
    }
    let preferred_size = input.preferred_size.as_deref().unwrap_or("template");
    if !["template", "auto", "1024x1024", "1536x1024", "1024x1536", "256x256", "512x512", "1792x1024", "1024x1792"].contains(&preferred_size) {
        return Err(AppError::validation("ابعاد تصویر نامعتبر است."));
    }
    let timeout = input.timeout_sec.unwrap_or(180);
    if !(10..=900).contains(&timeout) {
        return Err(AppError::validation("Timeout باید بین ۱۰ تا ۹۰۰ ثانیه باشد."));
    }
    let max_conc = input.max_concurrent.unwrap_or(2);
    if !(1..=10).contains(&max_conc) {
        return Err(AppError::validation("حداکثر درخواست همزمان باید بین ۱ تا ۱۰ باشد."));
    }
    let rpm = input.rpm_limit.filter(|r| *r > 0);
    let proxy = input.proxy_url.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let id = match &input.id {
        Some(id) => {
            let e: Option<(String,)> = sqlx::query_as("SELECT id FROM image_providers WHERE id = ?")
                .bind(id)
                .fetch_optional(&state.db)
                .await?;
            e.ok_or_else(|| AppError::not_found("ارائه‌دهنده پیدا نشد."))?;
            id.clone()
        }
        None => ulid::Ulid::new().to_string(),
    };
    let key = secret_key(&id);
    match input.api_key.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(k) => state.secrets.set(&key, k)?,
        None => {
            if state.secrets.get(&key)?.is_none() {
                return Err(AppError::validation("کلید API الزامی است."));
            }
        }
    }
    sqlx::query(
        "INSERT INTO image_providers (id, name, kind, base_url, model, output_format, image_quality, preferred_size, timeout_sec, proxy_url, secret_ref, rpm_limit, max_concurrent)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, kind=excluded.kind, base_url=excluded.base_url, model=excluded.model,
           output_format=excluded.output_format, image_quality=excluded.image_quality, preferred_size=excluded.preferred_size,
           timeout_sec=excluded.timeout_sec, proxy_url=excluded.proxy_url, secret_ref=excluded.secret_ref,
           rpm_limit=excluded.rpm_limit, max_concurrent=excluded.max_concurrent,
           updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
    )
    .bind(&id)
    .bind(&name)
    .bind(&input.kind)
    .bind(&base_url)
    .bind(&model)
    .bind(output_format)
    .bind(image_quality)
    .bind(preferred_size)
    .bind(timeout)
    .bind(proxy)
    .bind(&key)
    .bind(rpm)
    .bind(max_conc)
    .execute(&state.db)
    .await?;
    let row = sqlx::query_as::<_, Row>(&format!("SELECT {COLS} FROM image_providers WHERE id = ?"))
        .bind(&id)
        .fetch_one(&state.db)
        .await?;
    view(state, row)
}

pub async fn delete(state: &AppState, id: &str) -> AppResult<()> {
    if super::license::is_builtin(id) {
        return Err(AppError::validation("این ارائه‌دهنده توسط برنامه مدیریت می‌شود."));
    }
    let r = sqlx::query("DELETE FROM image_providers WHERE id = ?").bind(id).execute(&state.db).await?;
    if r.rows_affected() == 0 {
        return Err(AppError::not_found("ارائه‌دهنده پیدا نشد."));
    }
    state.secrets.delete(&secret_key(id))?;
    Ok(())
}

/// Connection test: generates one small test image and returns its size in bytes.
pub async fn test(state: &AppState, id: &str) -> AppResult<usize> {
    let image = generate(state, id, "A simple blue circle on a plain white background, no text", "1024x1024").await?;
    Ok(image.bytes.len())
}

/// Generates one image through the configured provider (rate-limited).
pub async fn generate(state: &AppState, id: &str, prompt: &str, size: &str) -> AppResult<GeneratedImage> {
    let r = sqlx::query_as::<_, Row>(&format!("SELECT {COLS} FROM image_providers WHERE id = ?"))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::validation("ارائه‌دهنده‌ی تصویر انتخاب‌شده پیدا نشد."))?;
    let key = r
        .secret_ref
        .as_deref()
        .map(|k| state.secrets.get(k))
        .transpose()?
        .flatten()
        .ok_or_else(|| AppError::validation(format!("کلید API برای «{}» ذخیره نشده است.", r.name)))?;
    let base = r.base_url.clone().unwrap_or_else(|| OPENAI_BASE.to_string());
    let client = OpenAiImage::new(&base, &key, &r.model, Duration::from_secs(r.timeout_sec as u64), r.proxy_url.as_deref(), &r.output_format, &r.image_quality)?;
    let _permit = state.limits.acquire(&r.id, r.rpm_limit, r.max_concurrent).await;
    let effective_size = if r.preferred_size == "template" { size } else { &r.preferred_size };
    client.generate(prompt, effective_size).await.map_err(|e: ProviderError| {
        if matches!(e.kind, ProviderErrorKind::Network | ProviderErrorKind::Timeout) && !state.online.load(Ordering::Relaxed) {
            AppError::offline()
        } else {
            e.into()
        }
    })
}
