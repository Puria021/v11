//! SQLite-backed job queue with per-queue concurrency, pause/cancel via campaign status,
//! exponential backoff and crash recovery.

use crate::error::{AppError, AppResult};
use crate::services::generation;
use crate::services::logs;
use crate::services::upload;
use crate::state::AppState;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const QUEUES: [&str; 3] = ["content", "image", "wordpress"];

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Job {
    pub id: i64,
    pub queue: String,
    #[sqlx(rename = "type")]
    pub jtype: String,
    pub project_id: Option<String>,
    pub campaign_id: Option<String>,
    pub plan_id: Option<String>,
    pub retry_count: i64,
    pub max_retries: i64,
}

pub struct NewJob {
    pub queue: &'static str,
    pub jtype: &'static str,
    pub project_id: String,
    pub campaign_id: String,
    pub plan_id: String,
    pub max_retries: i64,
}

/// Adds a job unless an active job with the same (type, plan) already exists.
pub async fn enqueue(state: &AppState, j: NewJob) -> AppResult<Option<i64>> {
    let dedupe = format!("{}:{}", j.jtype, j.plan_id);
    // Uploads run as soon as they are queued; upload_at is the WordPress publish date, not a delay.
    let run_after: Option<String> = None;
    let r = sqlx::query(
        "INSERT OR IGNORE INTO jobs (queue, type, project_id, campaign_id, plan_id, max_retries, dedupe_key, run_after)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(j.queue)
    .bind(j.jtype)
    .bind(&j.project_id)
    .bind(&j.campaign_id)
    .bind(&j.plan_id)
    .bind(j.max_retries)
    .bind(&dedupe)
    .bind(run_after)
    .execute(&state.db)
    .await?;
    state.wake.notify_one();
    Ok(if r.rows_affected() == 1 { Some(r.last_insert_rowid()) } else { None })
}

async fn concurrency(state: &AppState, queue: &str) -> usize {
    let key = format!("queue.concurrency.{queue}");
    let row: Option<(String,)> = sqlx::query_as("SELECT value_json FROM settings WHERE key=?")
        .bind(&key)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    row.and_then(|r| r.0.trim().parse::<usize>().ok()).unwrap_or(match queue { "content" => 3, "image" => 2, _ => 3 }).clamp(1, 10)
}

/// On startup: jobs that were mid-flight when the app died go back to the queue.
pub async fn recover(state: &AppState) -> AppResult<u64> {
    let r = sqlx::query("UPDATE jobs SET status='RETRY_PENDING', run_after=NULL, started_at=NULL WHERE status='PROCESSING'")
        .execute(&state.db)
        .await?;
    sqlx::query("UPDATE article_plans SET status='QUEUED' WHERE status='GENERATING'").execute(&state.db).await?;
    sqlx::query("UPDATE article_plans SET status='READY_TO_UPLOAD' WHERE status='UPLOADING'").execute(&state.db).await?;
    sqlx::query("UPDATE article_plans SET status='CONTENT_GENERATED' WHERE status='IMAGE_GENERATING'")
        .execute(&state.db)
        .await?;
    Ok(r.rows_affected())
}

#[derive(Clone, Default)]
pub struct Scheduler {
    running: Arc<Mutex<HashMap<&'static str, usize>>>,
}

const RUNNABLE: &str = "j.status IN ('PENDING','RETRY_PENDING')
  AND (j.run_after IS NULL OR j.run_after <= strftime('%Y-%m-%dT%H:%M:%fZ','now'))
  AND (c.status IS NULL OR c.status NOT IN ('PAUSED','CANCELLED'))";

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    fn running_count(&self, q: &'static str) -> usize {
        *self.running.lock().unwrap().get(q).unwrap_or(&0)
    }

    pub fn total_running(&self) -> usize {
        self.running.lock().unwrap().values().sum()
    }

    /// Claims runnable jobs up to each queue's free capacity and spawns them. Returns how many started.
    pub async fn tick(&self, state: &AppState) -> AppResult<usize> {
        let mut started = 0;
        for q in QUEUES {
            let cap = concurrency(state, q).await;
            let free = cap.saturating_sub(self.running_count(q));
            if free == 0 {
                continue;
            }
            let ids: Vec<(i64,)> = sqlx::query_as(&format!(
                "SELECT j.id FROM jobs j LEFT JOIN campaigns c ON c.id=j.campaign_id
                 WHERE j.queue=? AND {RUNNABLE} ORDER BY j.priority DESC, j.id LIMIT ?"
            ))
            .bind(q)
            .bind(free as i64)
            .fetch_all(&state.db)
            .await?;
            for (id,) in ids {
                let claimed = sqlx::query(
                    "UPDATE jobs SET status='PROCESSING', started_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
                     WHERE id=? AND status IN ('PENDING','RETRY_PENDING')",
                )
                .bind(id)
                .execute(&state.db)
                .await?;
                if claimed.rows_affected() != 1 {
                    continue;
                }
                let job = sqlx::query_as::<_, Job>(
                    "SELECT id, queue, type, project_id, campaign_id, plan_id, retry_count, max_retries FROM jobs WHERE id=?",
                )
                .bind(id)
                .fetch_one(&state.db)
                .await?;
                *self.running.lock().unwrap().entry(q).or_insert(0) += 1;
                started += 1;
                let (st, sch) = (state.clone(), self.clone());
                tokio::spawn(async move {
                    emit_job(&st, &job, "PROCESSING", None);
                    let outcome = run_job(&st, &job).await;
                    finish(&st, &job, outcome).await;
                    {
                        let mut m = sch.running.lock().unwrap();
                        if let Some(n) = m.get_mut(job.queue.as_str()) {
                            *n = n.saturating_sub(1);
                        }
                    }
                    st.wake.notify_one();
                });
            }
        }
        Ok(started)
    }

    /// Background loop used by the app.
    pub async fn run(self, state: AppState) {
        loop {
            if let Err(e) = self.tick(&state).await {
                tracing::error!("scheduler tick failed: {e}");
            }
            let _ = tokio::time::timeout(Duration::from_secs(1), state.wake.notified()).await;
        }
    }

    /// Test helper: runs until no job is waiting or running (paused/cancelled campaigns are ignored).
    pub async fn drain(&self, state: &AppState, timeout: Duration) -> AppResult<()> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            self.tick(state).await?;
            let (n,): (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM jobs j LEFT JOIN campaigns c ON c.id=j.campaign_id
                 WHERE j.status IN ('PENDING','RETRY_PENDING','PROCESSING') AND (c.status IS NULL OR c.status NOT IN ('PAUSED','CANCELLED'))",
            )
            .fetch_one(&state.db)
            .await?;
            if n == 0 && self.total_running() == 0 {
                return Ok(());
            }
            if tokio::time::Instant::now() > deadline {
                return Err(AppError::internal("drain timed out"));
            }
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    }
}

async fn run_job(state: &AppState, job: &Job) -> Result<(), AppError> {
    match job.jtype.as_str() {
        "gen_content" => generation::run_content(state, job).await,
        "gen_image" => generation::run_image(state, job).await,
        "upload_wp" => upload::run_upload(state, job).await,
        other => Err(AppError::internal(format!("unknown job type {other}"))),
    }
}

fn emit_job(state: &AppState, job: &Job, status: &str, error: Option<&str>) {
    state.events.emit(
        "job:updated",
        json!({"jobId": job.id, "queue": job.queue, "type": job.jtype, "status": status,
               "planId": job.plan_id, "campaignId": job.campaign_id, "error": error}),
    );
}

async fn finish(state: &AppState, job: &Job, outcome: Result<(), AppError>) {
    let stage = match job.jtype.as_str() {
        "gen_image" => "image",
        "upload_wp" => "upload",
        _ => "content",
    };
    match outcome {
        Ok(()) => {
            let _ = sqlx::query("UPDATE jobs SET status='DONE', error=NULL, completed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?")
                .bind(job.id)
                .execute(&state.db)
                .await;
            emit_job(state, job, "DONE", None);
        }
        Err(e) if e.code == "CANCELLED" || e.code == "DUPLICATE_BLOCKED" => {
            let _ = sqlx::query("UPDATE jobs SET status='CANCELLED', completed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?")
                .bind(job.id)
                .execute(&state.db)
                .await;
            emit_job(state, job, "CANCELLED", None);
        }
        Err(e) if e.retryable && job.retry_count < job.max_retries => {
            let mult = if e.code == "RATE_LIMITED" { 4.0 } else { 1.0 };
            let secs = (state.backoff_base.as_secs_f64() * mult * 2f64.powi(job.retry_count as i32)).min(300.0);
            let _ = sqlx::query(
                "UPDATE jobs SET status='RETRY_PENDING', retry_count=retry_count+1, error_kind=?, error=?,
                 run_after=strftime('%Y-%m-%dT%H:%M:%fZ','now', '+' || ? || ' seconds') WHERE id=?",
            )
            .bind(e.code)
            .bind(&e.message)
            .bind(format!("{secs:.3}"))
            .bind(job.id)
            .execute(&state.db)
            .await;
            if let Some(pid) = &job.plan_id {
                // Back to a waiting state so the UI doesn't show it as actively generating.
                let waiting = match stage {
                    "image" => "CONTENT_GENERATED",
                    "upload" => "READY_TO_UPLOAD",
                    _ => "QUEUED",
                };
                let _ = sqlx::query("UPDATE article_plans SET status=?, last_error=? WHERE id=? AND status IN ('GENERATING','IMAGE_GENERATING','UPLOADING')")
                    .bind(waiting)
                    .bind(&e.message)
                    .bind(pid)
                    .execute(&state.db)
                    .await;
            }
            logs::record(state, "WARN", job.project_id.as_deref(), job.campaign_id.as_deref(), &job.jtype, "WARN",
                &format!("retry {} in {:.0}s: {}", job.retry_count + 1, secs, e.message)).await;
            emit_job(state, job, "RETRY_PENDING", Some(&e.message));
            state.wake.notify_one();
        }
        Err(e) => {
            let _ = sqlx::query("UPDATE jobs SET status='FAILED', error_kind=?, error=?, completed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?")
                .bind(e.code)
                .bind(&e.message)
                .bind(job.id)
                .execute(&state.db)
                .await;
            if let Some(pid) = &job.plan_id {
                generation::mark_failed(state, pid, stage, &e.message).await;
            }
            logs::record(state, "ERROR", job.project_id.as_deref(), job.campaign_id.as_deref(), &job.jtype, "FAILED", &e.message).await;
            emit_job(state, job, "FAILED", Some(&e.message));
        }
    }
    if let Some(cid) = &job.campaign_id {
        generation::emit_progress(state, cid).await;
        maybe_notify(state, cid).await;
    }
}

/// Emits one `notify` event when a run has finished (nothing left queued or running).
async fn maybe_notify(state: &AppState, campaign_id: &str) {
    let outstanding: Option<(i64,)> = sqlx::query_as("SELECT COUNT(*) FROM jobs WHERE campaign_id=? AND status IN ('PENDING','PROCESSING','RETRY_PENDING')")
        .bind(campaign_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    if outstanding.map(|o| o.0).unwrap_or(1) > 0 {
        return;
    }
    // The flag makes this fire exactly once per run even when the last jobs finish together.
    let claimed = sqlx::query("UPDATE campaigns SET notified=1 WHERE id=? AND notified=0 AND status IN ('IN_PROGRESS','COMPLETED')")
        .bind(campaign_id)
        .execute(&state.db)
        .await
        .map(|r| r.rows_affected() == 1)
        .unwrap_or(false);
    if !claimed {
        return;
    }
    let count = |sql: &'static str| {
        let (db, id) = (state.db.clone(), campaign_id.to_string());
        async move { sqlx::query_as::<_, (i64,)>(sql).bind(id).fetch_one(&db).await.map(|r| r.0).unwrap_or(0) }
    };
    let failed = count("SELECT COUNT(*) FROM article_plans WHERE campaign_id=? AND status='FAILED'").await;
    let total = count("SELECT COUNT(*) FROM article_plans WHERE campaign_id=?").await;
    let drafts = count("SELECT COUNT(*) FROM article_plans WHERE campaign_id=? AND status='DRAFT_CREATED'").await;
    let done = count("SELECT COUNT(*) FROM article_contents x JOIN article_plans p ON p.id=x.plan_id WHERE p.campaign_id=?").await;
    let (kind, text) = if failed > 0 {
        ("errors", format!("{failed} مقاله با خطا مواجه شدند."))
    } else if total > 0 && drafts == total {
        ("campaign_done", format!("همه‌ی {total} مقاله در وردپرس به صورت Draft ساخته شدند."))
    } else {
        ("campaign_done", format!("تولید {done} مقاله به پایان رسید."))
    };
    state.events.emit("notify", json!({"kind": kind, "campaignId": campaign_id, "text": text}));
}
