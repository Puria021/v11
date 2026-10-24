//! SQLite connection + migrations.

use crate::error::AppResult;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub async fn connect(path: &Path) -> AppResult<SqlitePool> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new().max_connections(5).connect_with(opts).await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

/// In-memory database (single connection so every query sees the same data). For tests.
pub async fn connect_memory() -> AppResult<SqlitePool> {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .map_err(sqlx::Error::from)?
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}
