use crate::error::AppResult;
use crate::services::metrics::{self as svc, MetricsConfig, MetricsResult, SaveMetricsConfig};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_metrics_config(state: State<'_, AppState>) -> AppResult<MetricsConfig> { svc::get(&state).await }
#[tauri::command]
pub async fn save_metrics_config(state: State<'_, AppState>, input: SaveMetricsConfig) -> AppResult<MetricsConfig> { svc::save(&state, input).await }
#[tauri::command]
pub async fn fetch_keyword_metrics(state: State<'_, AppState>, campaign_id: String) -> AppResult<MetricsResult> { svc::fetch(&state, &campaign_id).await }
