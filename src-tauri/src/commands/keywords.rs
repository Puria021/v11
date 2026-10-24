use crate::error::AppResult;
use crate::services::keywords::{self as svc, ImportResult, Keyword, KeywordPage, KeywordQuery, SimilarityWarning, SuggestResult};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn save_primary_keywords(state: State<'_, AppState>, campaign_id: String, seeds: Vec<String>) -> AppResult<Vec<Keyword>> {
    svc::save_primary(&state, &campaign_id, seeds).await
}

#[tauri::command]
pub async fn import_keywords_csv(state: State<'_, AppState>, campaign_id: String, csv: String) -> AppResult<ImportResult> {
    svc::import_csv(&state, &campaign_id, &csv).await
}

#[tauri::command]
pub async fn add_manual_keyword(state: State<'_, AppState>, campaign_id: String, keyword: String) -> AppResult<Keyword> {
    svc::add_manual(&state, &campaign_id, &keyword).await
}

#[tauri::command]
pub async fn ai_suggest_keywords(state: State<'_, AppState>, campaign_id: String, count: Option<usize>) -> AppResult<SuggestResult> {
    svc::ai_suggest(&state, &campaign_id, count.unwrap_or(30)).await
}

#[tauri::command]
pub async fn list_keywords(state: State<'_, AppState>, query: KeywordQuery) -> AppResult<KeywordPage> {
    svc::list(&state, query).await
}

#[tauri::command]
pub async fn select_keywords(state: State<'_, AppState>, campaign_id: String, ids: Vec<String>, selected: bool) -> AppResult<usize> {
    svc::select(&state, &campaign_id, &ids, selected).await
}

#[tauri::command]
pub async fn delete_keywords(state: State<'_, AppState>, campaign_id: String, ids: Vec<String>) -> AppResult<usize> {
    svc::delete(&state, &campaign_id, &ids).await
}

#[tauri::command]
pub async fn get_similarity_warnings(state: State<'_, AppState>, campaign_id: String) -> AppResult<Vec<SimilarityWarning>> {
    svc::similarity_warnings(&state, &campaign_id, 0.6).await
}

#[tauri::command]
pub async fn add_secondary_keyword(state: State<'_, AppState>, campaign_id: String, keyword: String) -> AppResult<Keyword> {
    svc::add_secondary(&state, &campaign_id, &keyword).await
}

#[tauri::command]
pub async fn edit_campaign_keyword(state: State<'_, AppState>, campaign_id: String, id: String, keyword: String) -> AppResult<()> {
    svc::edit_keyword(&state, &campaign_id, &id, &keyword).await
}
