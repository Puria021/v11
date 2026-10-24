use crate::state::AppState;

/// Best-effort write to system_logs. Never fails the caller. Messages must already be free of secrets.
pub async fn record(
    state: &AppState,
    level: &str,
    project_id: Option<&str>,
    campaign_id: Option<&str>,
    action: &str,
    status: &str,
    message: &str,
) {
    let _ = sqlx::query(
        "INSERT INTO system_logs (level, project_id, campaign_id, action, status, message) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(level)
    .bind(project_id)
    .bind(campaign_id)
    .bind(action)
    .bind(status)
    .bind(message)
    .execute(&state.db)
    .await;
}

/// Same as `record` but tied to an article.
pub async fn record_plan(state: &AppState, level: &str, project_id: &str, campaign_id: &str, plan_id: &str, action: &str, status: &str, message: &str) {
    let _ = sqlx::query(
        "INSERT INTO system_logs (level, project_id, campaign_id, plan_id, action, status, message) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(level)
    .bind(project_id)
    .bind(campaign_id)
    .bind(plan_id)
    .bind(action)
    .bind(status)
    .bind(message)
    .execute(&state.db)
    .await;
}

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Sqlite};

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LogQuery {
    pub project_id: Option<String>,
    pub campaign_id: Option<String>,
    pub article_no: Option<i64>,
    pub level: Option<String>,
    pub status: Option<String>,
    pub action: Option<String>,
    pub search: Option<String>,
    /// YYYY-MM-DD (UTC), inclusive.
    pub from: Option<String>,
    pub to: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct LogRow {
    pub id: i64,
    pub at: String,
    pub level: String,
    pub project_name: Option<String>,
    pub campaign_name: Option<String>,
    pub article_no: Option<i64>,
    pub action: String,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPage {
    pub items: Vec<LogRow>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

const FROM: &str = " FROM system_logs l LEFT JOIN projects p ON p.id=l.project_id LEFT JOIN campaigns c ON c.id=l.campaign_id LEFT JOIN article_plans a ON a.id=l.plan_id";

fn filters<'a>(qb: &mut QueryBuilder<'a, Sqlite>, q: &'a LogQuery) {
    qb.push(" WHERE 1=1");
    let nz = |o: &'a Option<String>| o.as_deref().filter(|s| !s.is_empty());
    if let Some(v) = nz(&q.project_id) { qb.push(" AND l.project_id = ").push_bind(v); }
    if let Some(v) = nz(&q.campaign_id) { qb.push(" AND l.campaign_id = ").push_bind(v); }
    if let Some(n) = q.article_no { qb.push(" AND a.article_no = ").push_bind(n); }
    if let Some(v) = nz(&q.level) { qb.push(" AND l.level = ").push_bind(v); }
    if let Some(v) = nz(&q.status) { qb.push(" AND l.status = ").push_bind(v); }
    if let Some(v) = nz(&q.action) { qb.push(" AND l.action = ").push_bind(v); }
    if let Some(v) = nz(&q.from) { qb.push(" AND substr(l.at,1,10) >= ").push_bind(v); }
    if let Some(v) = nz(&q.to) { qb.push(" AND substr(l.at,1,10) <= ").push_bind(v); }
    if let Some(v) = nz(&q.search) { qb.push(" AND l.message LIKE ").push_bind(format!("%{v}%")); }
}

const COLS: &str = "SELECT l.id, l.at, l.level, p.name AS project_name, c.name AS campaign_name, a.article_no AS article_no, l.action, l.status, l.message";

pub async fn list(state: &AppState, q: &LogQuery) -> AppResult<LogPage> {
    let page = q.page.unwrap_or(1).max(1);
    let page_size = q.page_size.unwrap_or(100).clamp(1, 500);
    let mut cqb = QueryBuilder::<Sqlite>::new(format!("SELECT COUNT(*){FROM}"));
    filters(&mut cqb, q);
    let total: i64 = cqb.build_query_scalar().fetch_one(&state.db).await?;
    let mut qb = QueryBuilder::<Sqlite>::new(format!("{COLS}{FROM}"));
    filters(&mut qb, q);
    qb.push(" ORDER BY l.id DESC LIMIT ").push_bind(page_size).push(" OFFSET ").push_bind((page - 1) * page_size);
    let items: Vec<LogRow> = qb.build_query_as().fetch_all(&state.db).await?;
    Ok(LogPage { items, total, page, page_size })
}

pub async fn actions(state: &AppState) -> AppResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT DISTINCT action FROM system_logs ORDER BY action").fetch_all(&state.db).await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

fn csv_cell(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Writes the filtered log to a UTF-8 (BOM) CSV inside the app data folder and returns its path.
pub async fn export(state: &AppState, q: &LogQuery) -> AppResult<String> {
    let mut qb = QueryBuilder::<Sqlite>::new(format!("{COLS}{FROM}"));
    filters(&mut qb, q);
    qb.push(" ORDER BY l.id ASC LIMIT 200000");
    let rows: Vec<LogRow> = qb.build_query_as().fetch_all(&state.db).await?;
    let mut out = String::from("\u{feff}time,level,project,campaign,article,action,status,message\n");
    for r in rows {
        out.push_str(&[
            csv_cell(&r.at), csv_cell(&r.level), csv_cell(r.project_name.as_deref().unwrap_or("")),
            csv_cell(r.campaign_name.as_deref().unwrap_or("")), csv_cell(&r.article_no.map(|n| n.to_string()).unwrap_or_default()),
            csv_cell(&r.action), csv_cell(&r.status), csv_cell(r.message.as_deref().unwrap_or("")),
        ].join(","));
        out.push('\n');
    }
    let (stamp,): (String,) = sqlx::query_as("SELECT strftime('%Y%m%d-%H%M%S','now')").fetch_one(&state.db).await?;
    let dir = state.data_dir.join("exports");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("logs-{stamp}.csv"));
    std::fs::write(&path, out).map_err(|_| AppError::internal("نوشتن فایل لاگ ناموفق بود."))?;
    Ok(path.display().to_string())
}
