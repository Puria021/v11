use crate::error::AppResult;
use crate::services::network::{self as svc, NetworkSettings};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_network_settings(state: State<'_, AppState>) -> AppResult<NetworkSettings> {
    Ok(svc::get(&state).await)
}
#[tauri::command]
pub async fn save_network_settings(state: State<'_, AppState>, settings: NetworkSettings) -> AppResult<NetworkSettings> {
    svc::save(&state, settings).await
}
