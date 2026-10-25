use crate::error::AppResult;
use crate::services::articles::{self, ArticlePatch};
use crate::services::backup::{self, BackupPreview, ExportResult, ImportResult};
use crate::services::generation::ArticleDetail;
use crate::services::logs::{self, LogPage, LogQuery};
use crate::services::upload;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn upload_to_wordpress(state: State<'_, AppState>, plan_id: String, publish_now: Option<bool>) -> AppResult<()> {
    upload::request_upload(&state, &plan_id, publish_now.unwrap_or(false)).await
}

#[tauri::command]
pub async fn upload_campaign(state: State<'_, AppState>, campaign_id: String, confirm: bool) -> AppResult<i64> {
    upload::enqueue_campaign(&state, &campaign_id, confirm).await
}

#[tauri::command]
pub async fn set_plan_taxonomy(state: State<'_, AppState>, plan_id: String, category_ids: Vec<i64>, tags: Vec<String>) -> AppResult<()> {
    upload::set_taxonomy(&state, &plan_id, category_ids, tags).await
}

#[tauri::command]
pub async fn set_default_category(state: State<'_, AppState>, project_id: String, wp_id: Option<i64>) -> AppResult<()> {
    upload::set_default_category(&state, &project_id, wp_id).await
}

#[tauri::command]
pub async fn update_article(state: State<'_, AppState>, plan_id: String, patch: ArticlePatch, expected_version: i64) -> AppResult<ArticleDetail> {
    articles::update(&state, &plan_id, patch, expected_version).await
}

#[tauri::command]
pub async fn preview_html(html: String) -> AppResult<String> {
    Ok(articles::preview_html(&html))
}

#[tauri::command]
pub async fn list_logs(state: State<'_, AppState>, query: LogQuery) -> AppResult<LogPage> {
    logs::list(&state, &query).await
}

#[tauri::command]
pub async fn list_log_actions(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    logs::actions(&state).await
}

#[tauri::command]
pub async fn export_logs(state: State<'_, AppState>, query: LogQuery) -> AppResult<String> {
    logs::export(&state, &query).await
}

#[tauri::command]
pub async fn export_backup(state: State<'_, AppState>, path: Option<String>, project_id: Option<String>) -> AppResult<ExportResult> {
    backup::export(&state, path, project_id).await
}

#[tauri::command]
pub async fn preview_backup(state: State<'_, AppState>, path: String) -> AppResult<BackupPreview> {
    backup::preview(&state, &path).await
}

#[tauri::command]
pub async fn import_backup(state: State<'_, AppState>, path: String, mode: String) -> AppResult<ImportResult> {
    backup::import(&state, &path, &mode).await
}

#[tauri::command]
pub async fn clear_cache(state: State<'_, AppState>) -> AppResult<crate::services::cache::CacheReport> {
    crate::services::cache::clear(&state).await
}
