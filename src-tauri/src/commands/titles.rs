use crate::error::AppResult;
use crate::services::titles::{self as svc, PlanList, TitleView, TitlesResult};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn generate_titles(state: State<'_, AppState>, campaign_id: String) -> AppResult<TitlesResult> {
    svc::generate(&state, &campaign_id).await
}

#[tauri::command]
pub async fn list_article_plans(state: State<'_, AppState>, campaign_id: String) -> AppResult<PlanList> {
    svc::list(&state, &campaign_id).await
}

#[tauri::command]
pub async fn approve_title(state: State<'_, AppState>, plan_id: String, title_id: String) -> AppResult<()> {
    svc::approve(&state, &plan_id, &title_id).await
}

#[tauri::command]
pub async fn edit_title(state: State<'_, AppState>, plan_id: String, title: String) -> AppResult<TitleView> {
    svc::edit(&state, &plan_id, &title).await
}

#[tauri::command]
pub async fn reject_title(state: State<'_, AppState>, plan_id: String, title_id: String) -> AppResult<()> {
    svc::reject(&state, &plan_id, &title_id).await
}

#[tauri::command]
pub async fn generate_alternative_title(state: State<'_, AppState>, plan_id: String) -> AppResult<TitleView> {
    svc::alternative(&state, &plan_id).await
}

#[tauri::command]
pub async fn approve_all_titles(state: State<'_, AppState>, campaign_id: String) -> AppResult<i64> {
    svc::approve_all(&state, &campaign_id).await
}
