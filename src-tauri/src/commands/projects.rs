use crate::error::AppResult;
use crate::services::projects::{self as svc, ProjectCard, ProjectInput};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_projects(state: State<'_, AppState>) -> AppResult<Vec<ProjectCard>> {
    svc::list(&state).await
}

#[tauri::command]
pub async fn get_project(state: State<'_, AppState>, id: String) -> AppResult<ProjectCard> {
    svc::get(&state, &id).await
}

#[tauri::command]
pub async fn create_project(state: State<'_, AppState>, input: ProjectInput) -> AppResult<ProjectCard> {
    svc::create(&state, input).await
}

#[tauri::command]
pub async fn update_project(state: State<'_, AppState>, id: String, input: ProjectInput) -> AppResult<ProjectCard> {
    svc::update(&state, &id, input).await
}

#[tauri::command]
pub async fn delete_project(state: State<'_, AppState>, id: String, confirm: bool) -> AppResult<()> {
    svc::delete(&state, &id, confirm).await
}
