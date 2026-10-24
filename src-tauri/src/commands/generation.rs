use crate::error::AppResult;
use crate::services::campaigns::{self, CampaignView};
use crate::services::generation::{self as svc, ArticleDetail, ArticleRow, Progress, StartResult};
use crate::state::AppState;
use serde_json::Value;
use tauri::State;

#[tauri::command]
pub async fn set_campaign_template(state: State<'_, AppState>, campaign_id: String, template_id: Option<String>) -> AppResult<CampaignView> {
    campaigns::set_template(&state, &campaign_id, template_id).await
}

#[tauri::command]
pub async fn set_campaign_image_provider(state: State<'_, AppState>, campaign_id: String, provider_id: Option<String>) -> AppResult<CampaignView> {
    campaigns::set_image_provider(&state, &campaign_id, provider_id).await
}

#[tauri::command]
pub async fn start_generation(state: State<'_, AppState>, campaign_id: String, confirm: bool) -> AppResult<StartResult> {
    svc::start(&state, &campaign_id, confirm).await
}

#[tauri::command]
pub async fn pause_campaign(state: State<'_, AppState>, campaign_id: String) -> AppResult<()> {
    svc::pause(&state, &campaign_id).await
}

#[tauri::command]
pub async fn resume_campaign(state: State<'_, AppState>, campaign_id: String) -> AppResult<()> {
    svc::resume(&state, &campaign_id).await
}

#[tauri::command]
pub async fn cancel_campaign(state: State<'_, AppState>, campaign_id: String) -> AppResult<()> {
    svc::cancel(&state, &campaign_id).await
}

#[tauri::command]
pub async fn get_progress(state: State<'_, AppState>, campaign_id: String) -> AppResult<Progress> {
    svc::progress(&state, &campaign_id).await
}

#[tauri::command]
pub async fn list_articles(state: State<'_, AppState>, campaign_id: String) -> AppResult<Vec<ArticleRow>> {
    svc::list_articles(&state, &campaign_id).await
}

#[tauri::command]
pub async fn get_article(state: State<'_, AppState>, plan_id: String) -> AppResult<ArticleDetail> {
    svc::get_article(&state, &plan_id).await
}

#[tauri::command]
pub async fn get_article_preview_images(state: State<'_, AppState>, plan_id: String) -> AppResult<svc::ArticlePreviewImages> {
    svc::get_article_preview_images(&state, &plan_id).await
}

#[tauri::command]
pub async fn get_article_image(state: State<'_, AppState>, plan_id: String) -> AppResult<Option<String>> {
    svc::get_article_image(&state, &plan_id).await
}

#[tauri::command]
pub async fn retry_article(state: State<'_, AppState>, plan_id: String, stage: String) -> AppResult<()> {
    svc::retry_article(&state, &plan_id, &stage).await
}

#[tauri::command]
pub async fn retry_failed(state: State<'_, AppState>, campaign_id: String) -> AppResult<i64> {
    svc::retry_failed(&state, &campaign_id).await
}

#[tauri::command]
pub async fn preview_prompt(state: State<'_, AppState>, plan_id: String) -> AppResult<Value> {
    svc::preview_prompt(&state, &plan_id).await
}

#[tauri::command]
pub async fn get_queue_settings(state: State<'_, AppState>) -> AppResult<Value> {
    svc::queue_settings(&state).await
}

#[tauri::command]
pub async fn update_queue_settings(state: State<'_, AppState>, content: i64, image: i64, wordpress: i64) -> AppResult<Value> {
    svc::update_queue_settings(&state, content, image, wordpress).await
}

#[tauri::command]
pub async fn set_upload_schedule(state: State<'_, AppState>, campaign_id: String, plan_id: Option<String>, upload_at: Option<String>) -> AppResult<usize> {
    crate::services::upload::set_schedule(&state, &campaign_id, plan_id.as_deref(), upload_at.as_deref()).await
}
