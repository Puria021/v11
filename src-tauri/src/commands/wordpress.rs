use crate::error::AppResult;
use crate::providers::wordpress::Term;
use crate::services::wordpress::{self as svc, SaveConnection, SyncResult, TestResult, WpConnection};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_wordpress_connection(state: State<'_, AppState>, project_id: String) -> AppResult<Option<WpConnection>> {
    svc::get_connection(&state, &project_id).await
}

#[tauri::command]
pub async fn save_wordpress_connection(state: State<'_, AppState>, input: SaveConnection) -> AppResult<WpConnection> {
    svc::save_connection(&state, input).await
}

#[tauri::command]
pub async fn test_wordpress_connection(state: State<'_, AppState>, project_id: String) -> AppResult<TestResult> {
    svc::test_connection(&state, &project_id).await
}

#[tauri::command]
pub async fn sync_wordpress_taxonomies(state: State<'_, AppState>, project_id: String) -> AppResult<SyncResult> {
    svc::sync_taxonomies(&state, &project_id).await
}

#[tauri::command]
pub async fn list_wp_terms(state: State<'_, AppState>, project_id: String, kind: String) -> AppResult<Vec<Term>> {
    svc::list_terms(&state, &project_id, &kind).await
}
