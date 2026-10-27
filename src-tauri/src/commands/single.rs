use crate::error::AppResult;
use crate::services::single::{self, SingleCreated, SingleInput, TitleOption};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn create_single_article(state: State<'_, AppState>, input: SingleInput) -> AppResult<SingleCreated> {
    single::create(&state, input).await
}

#[tauri::command]
pub async fn list_single_articles(state: State<'_, AppState>) -> AppResult<Vec<crate::services::single::SingleRow>> {
    single::list(&state).await
}

#[tauri::command]
pub async fn suggest_single_titles(state: State<'_, AppState>, campaign_id: String) -> AppResult<Vec<TitleOption>> {
    single::suggest_titles(&state, &campaign_id).await
}
