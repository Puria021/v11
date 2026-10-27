use crate::error::AppResult;
use crate::services::prefs::{self, AppPrefs};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_app_prefs(state: State<'_, AppState>) -> AppResult<AppPrefs> {
    prefs::load(&state).await
}

#[tauri::command]
pub async fn save_app_prefs(state: State<'_, AppState>, prefs: AppPrefs) -> AppResult<AppPrefs> {
    prefs::save(&state, prefs).await
}
