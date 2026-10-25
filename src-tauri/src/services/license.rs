//! First-run license. The user enters one API key; it is stored in the credential store and two fixed,
//! hidden providers (text + image) are created. Campaigns use them by default and the UI never lists them.

use crate::error::{AppError, AppResult};
use crate::services::{ai, image_providers};
use crate::state::AppState;
use serde::Serialize;

pub const TEXT_PROVIDER_ID: &str = "builtin-text";
pub const IMAGE_PROVIDER_ID: &str = "builtin-image";

const BASE_URL: &str = "https://tooken.club/v1";
const TEXT_MODEL: &str = "gpt-6-luna";
const IMAGE_MODEL: &str = "gpt-image-2.5";
const ACTIVATED_KEY: &str = "license.activated";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStatus {
    pub activated: bool,
}

/// True for the fixed providers that are managed by the application.
pub fn is_builtin(id: &str) -> bool {
    id == TEXT_PROVIDER_ID || id == IMAGE_PROVIDER_ID
}

pub async fn status(state: &AppState) -> AppResult<LicenseStatus> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value_json FROM settings WHERE key=?")
        .bind(ACTIVATED_KEY)
        .fetch_optional(&state.db)
        .await?;
    Ok(LicenseStatus { activated: matches!(row, Some((ref v,)) if v == "true") })
}

/// Stores the key, creates the fixed providers, and tests both connections. Activation succeeds only if
/// the text AND the image test pass; otherwise the stored key is removed again.
/// Fills the fixed providers into campaigns that were created before the license was activated.
/// Safe to call repeatedly: it only touches campaigns whose provider is still empty.
pub async fn attach_fixed_defaults(state: &AppState) -> AppResult<()> {
    sqlx::query("UPDATE campaigns SET ai_provider_id = ? WHERE ai_provider_id IS NULL AND EXISTS (SELECT 1 FROM ai_providers WHERE id = ?)")
        .bind(TEXT_PROVIDER_ID)
        .bind(TEXT_PROVIDER_ID)
        .execute(&state.db)
        .await?;
    sqlx::query("UPDATE campaigns SET image_provider_id = ? WHERE image_provider_id IS NULL AND EXISTS (SELECT 1 FROM image_providers WHERE id = ?)")
        .bind(IMAGE_PROVIDER_ID)
        .bind(IMAGE_PROVIDER_ID)
        .execute(&state.db)
        .await?;
    Ok(())
}

pub async fn activate(state: &AppState, api_key: &str) -> AppResult<()> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(AppError::validation("لایسنس را وارد کنید."));
    }
    upsert_providers(state).await?;
    state.secrets.set(&format!("ai-key:{TEXT_PROVIDER_ID}"), key)?;
    state.secrets.set(&format!("image-key:{IMAGE_PROVIDER_ID}"), key)?;

    let text = ai::test(state, TEXT_PROVIDER_ID).await;
    let image = image_providers::test(state, IMAGE_PROVIDER_ID).await;
    match (text, image) {
        (Ok(_), Ok(_)) => {
            sqlx::query("INSERT INTO settings (key, value_json) VALUES (?, 'true') ON CONFLICT(key) DO UPDATE SET value_json='true', updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')")
                .bind(ACTIVATED_KEY)
                .execute(&state.db)
                .await?;
            attach_fixed_defaults(state).await?;
            Ok(())
        }
        (t, i) => {
            state.secrets.delete(&format!("ai-key:{TEXT_PROVIDER_ID}"))?;
            state.secrets.delete(&format!("image-key:{IMAGE_PROVIDER_ID}"))?;
            let mut problems = Vec::new();
            if let Err(e) = t {
                problems.push(format!("تست متن: {}", e.message));
            }
            if let Err(e) = i {
                problems.push(format!("تست تصویر: {}", e.message));
            }
            Err(AppError::validation(format!("فعال‌سازی لایسنس ناموفق بود. {}", problems.join(" | "))))
        }
    }
}

async fn upsert_providers(state: &AppState) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO ai_providers (id, name, kind, base_url, model, temperature, max_tokens, timeout_sec, proxy_url, secret_ref, rpm_limit, max_concurrent)
         VALUES (?, 'متن', 'openai_compatible', ?, ?, 0.7, 3000, 120, NULL, ?, NULL, 3)
         ON CONFLICT(id) DO UPDATE SET base_url=excluded.base_url, model=excluded.model, temperature=excluded.temperature,
           max_tokens=excluded.max_tokens, timeout_sec=excluded.timeout_sec, secret_ref=excluded.secret_ref,
           updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
    )
    .bind(TEXT_PROVIDER_ID)
    .bind(BASE_URL)
    .bind(TEXT_MODEL)
    .bind(format!("ai-key:{TEXT_PROVIDER_ID}"))
    .execute(&state.db)
    .await?;
    sqlx::query(
        "INSERT INTO image_providers (id, name, kind, base_url, model, output_format, image_quality, preferred_size, timeout_sec, proxy_url, secret_ref, rpm_limit, max_concurrent)
         VALUES (?, 'تصویر', 'openai_image', ?, ?, 'png', 'low', 'template', 180, NULL, ?, NULL, 2)
         ON CONFLICT(id) DO UPDATE SET base_url=excluded.base_url, model=excluded.model, output_format=excluded.output_format,
           image_quality=excluded.image_quality, preferred_size=excluded.preferred_size, timeout_sec=excluded.timeout_sec,
           secret_ref=excluded.secret_ref, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
    )
    .bind(IMAGE_PROVIDER_ID)
    .bind(BASE_URL)
    .bind(IMAGE_MODEL)
    .bind(format!("image-key:{IMAGE_PROVIDER_ID}"))
    .execute(&state.db)
    .await?;
    Ok(())
}
