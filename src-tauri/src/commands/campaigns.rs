use crate::error::AppResult;
use crate::services::campaigns::{self as svc, CampaignView, CreateCampaign};
use crate::services::secondary::{self, SecondaryResult};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_campaigns(state: State<'_, AppState>, project_id: Option<String>) -> AppResult<Vec<CampaignView>> {
    svc::list(&state, project_id.as_deref()).await
}

#[tauri::command]
pub async fn get_campaign(state: State<'_, AppState>, id: String) -> AppResult<CampaignView> {
    svc::get(&state, &id).await
}

#[tauri::command]
pub async fn create_campaign(state: State<'_, AppState>, input: CreateCampaign) -> AppResult<CampaignView> {
    svc::create(&state, input).await
}

#[tauri::command]
pub async fn delete_campaign(state: State<'_, AppState>, id: String, confirm: bool) -> AppResult<()> {
    svc::delete(&state, &id, confirm).await
}

#[tauri::command]
pub async fn save_wizard_state(
    state: State<'_, AppState>,
    campaign_id: String,
    step: i64,
    data: serde_json::Value,
) -> AppResult<()> {
    svc::save_wizard_state(&state, &campaign_id, step, data).await
}

#[tauri::command]
pub async fn set_campaign_ai_provider(
    state: State<'_, AppState>,
    campaign_id: String,
    provider_id: Option<String>,
) -> AppResult<CampaignView> {
    svc::set_ai_provider(&state, &campaign_id, provider_id).await
}

#[tauri::command]
pub async fn generate_secondary_keywords(
    state: State<'_, AppState>,
    campaign_id: String,
    replace: bool,
) -> AppResult<SecondaryResult> {
    secondary::generate(&state, &campaign_id, replace).await
}

#[tauri::command]
pub async fn confirm_secondary_keywords(state: State<'_, AppState>, campaign_id: String) -> AppResult<i64> {
    secondary::confirm(&state, &campaign_id).await
}

#[tauri::command]
pub async fn reset_article_plans(state: State<'_, AppState>, campaign_id: String, confirm: bool) -> AppResult<()> {
    secondary::reset_plans(&state, &campaign_id, confirm).await
}
