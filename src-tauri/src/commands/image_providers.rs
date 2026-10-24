use crate::error::AppResult;
use crate::services::image_providers::{self as svc, ImageProviderView, SaveImageProvider};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn list_image_providers(state: State<'_, AppState>) -> AppResult<Vec<ImageProviderView>> {
    svc::list(&state).await
}

#[tauri::command]
pub async fn save_image_provider(state: State<'_, AppState>, input: SaveImageProvider) -> AppResult<ImageProviderView> {
    svc::save(&state, input).await
}

#[tauri::command]
pub async fn delete_image_provider(state: State<'_, AppState>, id: String) -> AppResult<()> {
    svc::delete(&state, &id).await
}

#[tauri::command]
pub async fn test_image_provider(state: State<'_, AppState>, id: String) -> AppResult<String> {
    let bytes = svc::test(&state, &id).await?;
    Ok(format!("اتصال موفق بود؛ تصویر آزمایشی با حجم {bytes} بایت ساخته شد. توجه: درخواست ممکن است هزینه داشته باشد."))
}
