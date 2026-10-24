use crate::error::AppResult;
use crate::state::AppState;
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::net::TcpStream;

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub projects: i64,
    pub campaigns: i64,
    pub keywords_researched: i64,
    pub keywords_approved: i64,
    pub article_plans: i64,
    pub articles_ready: i64,
    pub articles_generating: i64,
    pub drafts_created: i64,
    pub errors: i64,
}

pub async fn dashboard_stats(state: &AppState) -> AppResult<DashboardStats> {
    Ok(sqlx::query_as::<_, DashboardStats>(
        "SELECT
          (SELECT COUNT(*) FROM projects) AS projects,
          (SELECT COUNT(*) FROM campaigns) AS campaigns,
          (SELECT COUNT(*) FROM keywords) AS keywords_researched,
          (SELECT COUNT(*) FROM keywords WHERE is_selected = 1) AS keywords_approved,
          (SELECT COUNT(*) FROM article_plans) AS article_plans,
          (SELECT COUNT(*) FROM article_plans WHERE status = 'TITLE_APPROVED') AS articles_ready,
          (SELECT COUNT(*) FROM article_plans WHERE status IN ('QUEUED','GENERATING','IMAGE_GENERATING','UPLOADING')) AS articles_generating,
          (SELECT COUNT(*) FROM article_plans WHERE status = 'DRAFT_CREATED') AS drafts_created,
          (SELECT COUNT(*) FROM article_plans WHERE status = 'FAILED') AS errors",
    )
    .fetch_one(&state.db)
    .await?)
}

/// Best-effort connectivity probe: any successful TCP connect counts as online.
pub async fn probe_internet() -> bool {
    const TARGETS: [&str; 3] = ["1.1.1.1:443", "8.8.8.8:53", "9.9.9.9:443"];
    for t in TARGETS {
        if let Ok(Ok(_)) = tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(t)).await {
            return true;
        }
    }
    false
}

pub async fn check_internet(state: &AppState) -> bool {
    let online = probe_internet().await;
    state.online.store(online, Ordering::Relaxed);
    online
}
