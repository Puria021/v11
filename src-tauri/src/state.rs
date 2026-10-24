use crate::db;
use crate::error::AppResult;
use crate::events::{EventSink, NoopEvents};
use crate::limits::RateLimits;
use crate::secrets::SecretStore;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

/// Shared application state, managed by Tauri and passed to services.
#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub secrets: Arc<dyn SecretStore>,
    pub data_dir: PathBuf,
    pub online: Arc<AtomicBool>,
    pub events: Arc<dyn EventSink>,
    pub limits: Arc<RateLimits>,
    /// Wakes the job scheduler when new work is queued.
    pub wake: Arc<Notify>,
    /// Base delay for retry backoff (5s in production; tiny in tests).
    pub backoff_base: Duration,
}

impl AppState {
    pub async fn init(data_dir: PathBuf, secrets: Arc<dyn SecretStore>, events: Arc<dyn EventSink>) -> AppResult<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let pool = db::connect(&data_dir.join("data").join("app.db")).await?;
        crate::services::network::initialize(&pool).await?;
        Ok(Self {
            db: pool,
            secrets,
            data_dir,
            online: Arc::new(AtomicBool::new(true)),
            events,
            limits: Arc::new(RateLimits::new()),
            wake: Arc::new(Notify::new()),
            backoff_base: Duration::from_secs(5),
        })
    }

    /// For tests: in-memory DB, temp data dir, events discarded.
    pub async fn for_tests(secrets: Arc<dyn SecretStore>) -> AppResult<Self> {
        Self::for_tests_with_events(secrets, Arc::new(NoopEvents)).await
    }

    pub async fn for_tests_with_events(secrets: Arc<dyn SecretStore>, events: Arc<dyn EventSink>) -> AppResult<Self> {
        let pool = db::connect_memory().await?;
        let dir = std::env::temp_dir().join(format!("mam-test-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            db: pool,
            secrets,
            data_dir: dir,
            online: Arc::new(AtomicBool::new(true)),
            events,
            limits: Arc::new(RateLimits::new()),
            wake: Arc::new(Notify::new()),
            backoff_base: Duration::from_millis(10),
        })
    }
}
