//! App-wide preferences stored in the `settings` table.

use crate::error::AppResult;
use crate::state::AppState;
use serde::{Deserialize, Serialize};

const CLASSIC_EDITOR_KEY: &str = "prefs.classic_editor";

#[derive(Debug, Default, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct AppPrefs {
    /// Send articles to WordPress as one Classic block, so they open in the classic editor.
    pub classic_editor: bool,
}

async fn read_flag(state: &AppState, key: &str) -> AppResult<bool> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value_json FROM settings WHERE key=?").bind(key).fetch_optional(&state.db).await?;
    Ok(matches!(row, Some((v,)) if v == "true"))
}

pub async fn load(state: &AppState) -> AppResult<AppPrefs> {
    Ok(AppPrefs { classic_editor: read_flag(state, CLASSIC_EDITOR_KEY).await? })
}

pub async fn save(state: &AppState, prefs: AppPrefs) -> AppResult<AppPrefs> {
    let value = if prefs.classic_editor { "true" } else { "false" };
    sqlx::query("INSERT INTO settings (key, value_json) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')")
        .bind(CLASSIC_EDITOR_KEY)
        .bind(value)
        .execute(&state.db)
        .await?;
    load(state).await
}

/// Wraps the post body as a Classic block when the classic-editor option is on.
pub async fn wrap_content(state: &AppState, html: String) -> AppResult<String> {
    if load(state).await?.classic_editor {
        Ok(format!("<!-- wp:freeform -->\n{html}\n<!-- /wp:freeform -->"))
    } else {
        Ok(html)
    }
}
