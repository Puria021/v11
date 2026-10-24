use crate::error::AppResult;
use crate::services::products::{self as svc, ProductPage};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_products(state: State<'_, AppState>, project_id: String, search: String, page: i64) -> AppResult<ProductPage> {
    svc::list(&state, &project_id, &search, page).await
}
#[tauri::command]
pub async fn generate_product_description(state: State<'_, AppState>, provider_id: String, name: String, keyword: String, existing: String, language: String) -> AppResult<String> {
    svc::generate(&state, &provider_id, &name, &keyword, &existing, &language).await
}
#[tauri::command]
pub async fn save_product_description(state: State<'_, AppState>, project_id: String, product_id: i64, description: String) -> AppResult<()> {
    svc::save(&state, &project_id, product_id, &description).await
}
