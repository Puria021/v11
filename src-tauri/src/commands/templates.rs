use crate::error::AppResult;
use crate::services::templates::{self as svc, Template};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_templates(state: State<'_, AppState>) -> AppResult<Vec<Template>> {
    svc::list(&state).await
}

#[tauri::command]
pub async fn get_template(state: State<'_, AppState>, id: String) -> AppResult<Template> {
    svc::get(&state, &id).await
}

#[tauri::command]
pub async fn save_template(state: State<'_, AppState>, template: Template) -> AppResult<Template> {
    svc::save(&state, template).await
}

#[tauri::command]
pub async fn duplicate_template(state: State<'_, AppState>, id: String) -> AppResult<Template> {
    svc::duplicate(&state, &id).await
}

#[tauri::command]
pub async fn delete_template(state: State<'_, AppState>, id: String) -> AppResult<()> {
    svc::delete(&state, &id).await
}
