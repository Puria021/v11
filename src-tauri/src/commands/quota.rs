use crate::error::AppResult;
use crate::services::quota::{self, Quota};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_quota(state: State<'_, AppState>) -> AppResult<Quota> {
    quota::view(&state).await
}
