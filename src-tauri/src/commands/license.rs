use crate::error::AppResult;
use crate::services::license::{self as svc, LicenseStatus};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_license_status(state: State<'_, AppState>) -> AppResult<LicenseStatus> {
    svc::status(&state).await
}

#[tauri::command]
pub async fn activate_license(state: State<'_, AppState>, api_key: String) -> AppResult<LicenseStatus> {
    svc::activate(&state, &api_key).await?;
    svc::status(&state).await
}
