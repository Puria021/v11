use crate::error::AppResult;
use crate::services::ai::{self as svc, AiProviderView, ProviderTest, SaveAiProvider};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_ai_providers(state: State<'_, AppState>) -> AppResult<Vec<AiProviderView>> {
    svc::list(&state).await
}

#[tauri::command]
pub async fn save_ai_provider(state: State<'_, AppState>, input: SaveAiProvider) -> AppResult<AiProviderView> {
    svc::save(&state, input).await
}

#[tauri::command]
pub async fn delete_ai_provider(state: State<'_, AppState>, id: String) -> AppResult<()> {
    svc::delete(&state, &id).await
}

#[tauri::command]
pub async fn test_ai_provider(state: State<'_, AppState>, id: String) -> AppResult<ProviderTest> {
    svc::test(&state, &id).await
}
