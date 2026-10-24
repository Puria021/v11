use crate::error::AppResult;
use crate::services::system as svc;
use crate::state::AppState;
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
}

#[tauri::command]
pub async fn get_app_info(state: State<'_, AppState>) -> AppResult<AppInfo> {
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: state.data_dir.display().to_string(),
    })
}

#[tauri::command]
pub async fn check_internet(state: State<'_, AppState>) -> AppResult<bool> {
    Ok(svc::check_internet(&state).await)
}

#[tauri::command]
pub async fn get_dashboard_stats(state: State<'_, AppState>) -> AppResult<svc::DashboardStats> {
    svc::dashboard_stats(&state).await
}

#[tauri::command]
pub async fn reset_application(state: tauri::State<'_, AppState>, confirmation: String) -> AppResult<()> {
    if confirmation != "RESET ALL" { return Err(crate::error::AppError::validation("برای بازنشانی، عبارت RESET ALL را وارد کنید.")); }
    let busy: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs WHERE status IN ('PENDING','PROCESSING','RETRY_PENDING','PAUSED')").fetch_one(&state.db).await?;
    if busy.0 > 0 { return Err(crate::error::AppError::validation("ابتدا صف‌های در حال اجرا را متوقف یا لغو کنید.")); }
    let ai: Vec<(String,)> = sqlx::query_as("SELECT id FROM ai_providers").fetch_all(&state.db).await?;
    let images: Vec<(String,)> = sqlx::query_as("SELECT id FROM image_providers").fetch_all(&state.db).await?;
    let wp: Vec<(String,)> = sqlx::query_as("SELECT secret_ref FROM wordpress_connections WHERE secret_ref IS NOT NULL").fetch_all(&state.db).await?;
    let mut tx = state.db.begin().await?;
    for table in ["jobs", "article_plans", "keywords", "campaigns", "projects", "article_templates", "ai_providers", "image_providers", "system_logs"] {
        let sql = format!("DELETE FROM {table}");
        sqlx::query(&sql).execute(&mut *tx).await?;
    }
    sqlx::query("DELETE FROM settings").execute(&mut *tx).await?;
    for (key, value) in [("queue.concurrency.content", "3"), ("queue.concurrency.image", "2"), ("queue.concurrency.wordpress", "3"), ("ui.theme", "\"light\""), ("notifications.enabled", "true")] {
        sqlx::query("INSERT INTO settings(key, value_json) VALUES (?,?)").bind(key).bind(value).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    for (id,) in ai { state.secrets.delete(&format!("ai-key:{id}"))?; }
    for (id,) in images { state.secrets.delete(&format!("image-key:{id}"))?; }
    for (key,) in wp { state.secrets.delete(&key)?; }
    let files = state.data_dir.join("projects");
    if files.exists() { std::fs::remove_dir_all(files)?; }
    Ok(())
}
