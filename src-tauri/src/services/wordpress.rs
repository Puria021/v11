use crate::error::{AppError, AppResult};
use crate::providers::wordpress::rest::RestWordPressClient;
use crate::providers::wordpress::{Taxonomies, Term, WordPressClient};
use crate::providers::ProviderError;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use std::time::Duration;
use url::Url;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveConnection {
    pub project_id: String,
    pub api_url: String,
    pub username: String,
    /// Optional on update: when empty the stored secret is kept.
    pub application_password: Option<String>,
    pub proxy_url: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct ConnRow {
    id: String,
    project_id: String,
    api_url: String,
    username: String,
    secret_ref: String,
    proxy_url: Option<String>,
    status: String,
    last_error: Option<String>,
    wp_version: Option<String>,
    site_name: Option<String>,
    wp_user_name: Option<String>,
    media_capable: Option<i64>,
    last_tested_at: Option<String>,
    last_synced_at: Option<String>,
}

/// What the UI sees. Never contains the password — only `has_secret`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WpConnection {
    pub project_id: String,
    pub api_url: String,
    pub username: String,
    pub proxy_url: Option<String>,
    pub has_secret: bool,
    pub status: String,
    pub last_error: Option<String>,
    pub wp_version: Option<String>,
    pub site_name: Option<String>,
    pub wp_user_name: Option<String>,
    pub media_capable: Option<bool>,
    pub last_tested_at: Option<String>,
    pub last_synced_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub success: bool,
    pub site: String,
    pub user: String,
    pub wp_version: Option<String>,
    pub media_capable: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub categories: usize,
    pub tags: usize,
    pub authors: usize,
    pub post_types: usize,
}

/// Accepts a site URL or an API URL and returns the `.../wp-json` base.
pub fn normalize_api_url(raw: &str) -> AppResult<String> {
    let raw = raw.trim();
    let candidate = if raw.contains("://") { raw.to_string() } else { format!("https://{raw}") };
    let u = Url::parse(&candidate).map_err(|_| AppError::validation("آدرس API وردپرس نامعتبر است."))?;
    if !matches!(u.scheme(), "http" | "https") || u.host_str().is_none() {
        return Err(AppError::validation("آدرس باید با http یا https شروع شود."));
    }
    let mut s = u.as_str().trim_end_matches('/').to_string();
    if let Some(pos) = s.find("/wp-json") {
        s.truncate(pos + "/wp-json".len());
    } else {
        s.push_str("/wp-json");
    }
    Ok(s)
}

fn secret_key(project_id: &str) -> String {
    format!("wp-app-password:{project_id}")
}

fn to_dto(row: ConnRow, has_secret: bool) -> WpConnection {
    WpConnection {
        project_id: row.project_id,
        api_url: row.api_url,
        username: row.username,
        proxy_url: row.proxy_url,
        has_secret,
        status: row.status,
        last_error: row.last_error,
        wp_version: row.wp_version,
        site_name: row.site_name,
        wp_user_name: row.wp_user_name,
        media_capable: row.media_capable.map(|v| v != 0),
        last_tested_at: row.last_tested_at,
        last_synced_at: row.last_synced_at,
    }
}

async fn load_row(state: &AppState, project_id: &str) -> AppResult<Option<ConnRow>> {
    Ok(sqlx::query_as::<_, ConnRow>(
        "SELECT id, project_id, api_url, username, secret_ref, proxy_url, status, last_error, wp_version,
                site_name, wp_user_name, media_capable, last_tested_at, last_synced_at
         FROM wordpress_connections WHERE project_id = ?",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await?)
}

pub async fn get_connection(state: &AppState, project_id: &str) -> AppResult<Option<WpConnection>> {
    match load_row(state, project_id).await? {
        None => Ok(None),
        Some(row) => {
            let has = state.secrets.get(&row.secret_ref)?.is_some();
            Ok(Some(to_dto(row, has)))
        }
    }
}

pub async fn save_connection(state: &AppState, input: SaveConnection) -> AppResult<WpConnection> {
    let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM projects WHERE id = ?")
        .bind(&input.project_id)
        .fetch_optional(&state.db)
        .await?;
    if exists.is_none() {
        return Err(AppError::not_found("پروژه پیدا نشد."));
    }
    let api_url = normalize_api_url(&input.api_url)?;
    // Username is optional: empty means the secret is the MyArticleManager Bridge token.
    let username = input.username.trim().to_string();
    let key = secret_key(&input.project_id);
    match input.application_password.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(pw) => state.secrets.set(&key, pw)?,
        None => {
            if state.secrets.get(&key)?.is_none() {
                return Err(AppError::validation("توکن افزونه‌ی MyArticleManager Bridge (یا Application Password) الزامی است."));
            }
        }
    }
    let proxy = input.proxy_url.as_deref().map(str::trim).filter(|p| !p.is_empty());

    sqlx::query(
        "INSERT INTO wordpress_connections (id, project_id, api_url, username, secret_ref, proxy_url, status)
         VALUES (?, ?, ?, ?, ?, ?, 'UNTESTED')
         ON CONFLICT(project_id) DO UPDATE SET
           api_url = excluded.api_url, username = excluded.username, secret_ref = excluded.secret_ref,
           proxy_url = excluded.proxy_url, status = 'UNTESTED', last_error = NULL,
           updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
    )
    .bind(ulid::Ulid::new().to_string())
    .bind(&input.project_id)
    .bind(&api_url)
    .bind(&username)
    .bind(&key)
    .bind(proxy)
    .execute(&state.db)
    .await?;

    get_connection(state, &input.project_id)
        .await?
        .ok_or_else(|| AppError::internal("ذخیره‌ی اتصال ناموفق بود."))
}

pub(crate) async fn build_client(state: &AppState, project_id: &str) -> AppResult<(RestWordPressClient, String)> {
    let row = load_row(state, project_id)
        .await?
        .ok_or_else(|| AppError::validation("ابتدا اطلاعات اتصال وردپرس را ذخیره کنید."))?;
    let password = state
        .secrets
        .get(&row.secret_ref)?
        .ok_or_else(|| AppError::validation("Application Password ذخیره نشده است؛ دوباره وارد کنید."))?;
    let client = RestWordPressClient::new(
        &row.api_url,
        &row.username,
        &password,
        row.proxy_url.as_deref(),
        Duration::from_secs(60),
    )?;
    Ok((client, row.id))
}

/// If the request failed on the network and we know we're offline, say so clearly.
pub(crate) fn refine(state: &AppState, e: ProviderError) -> AppError {
    use crate::providers::ProviderErrorKind::{Network, Timeout};
    if matches!(e.kind, Network | Timeout) && !state.online.load(Ordering::Relaxed) {
        return AppError::offline();
    }
    e.into()
}

pub async fn test_connection(state: &AppState, project_id: &str) -> AppResult<TestResult> {
    let (client, _) = build_client(state, project_id).await?;
    match client.test_connection().await {
        Ok(info) => {
            sqlx::query(
                "UPDATE wordpress_connections SET status='CONNECTED', last_error=NULL, site_name=?, wp_user_name=?,
                 wp_version=?, media_capable=?, last_tested_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                 updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE project_id=?",
            )
            .bind(&info.site_name)
            .bind(&info.user_name)
            .bind(&info.wp_version)
            .bind(info.media_capable as i64)
            .bind(project_id)
            .execute(&state.db)
            .await?;
            Ok(TestResult {
                success: true,
                site: info.site_name,
                user: info.user_name,
                wp_version: info.wp_version,
                media_capable: info.media_capable,
            })
        }
        Err(e) => {
            let err = refine(state, e);
            sqlx::query(
                "UPDATE wordpress_connections SET status='FAILED', last_error=?,
                 last_tested_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE project_id=?",
            )
            .bind(&err.message)
            .bind(project_id)
            .execute(&state.db)
            .await?;
            Err(err)
        }
    }
}

pub async fn sync_taxonomies(state: &AppState, project_id: &str) -> AppResult<SyncResult> {
    let (client, _) = build_client(state, project_id).await?;
    let tax: Taxonomies = client.fetch_taxonomies().await.map_err(|e| refine(state, e))?;

    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM wp_terms WHERE project_id = ?").bind(project_id).execute(&mut *tx).await?;
    for (kind, list) in [
        ("category", &tax.categories),
        ("tag", &tax.tags),
        ("author", &tax.authors),
        ("post_type", &tax.post_types),
    ] {
        for t in list.iter() {
            sqlx::query(
                "INSERT OR REPLACE INTO wp_terms (id, project_id, kind, wp_id, name, slug, parent_wp_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(ulid::Ulid::new().to_string())
            .bind(project_id)
            .bind(kind)
            .bind(t.wp_id)
            .bind(&t.name)
            .bind(&t.slug)
            .bind(t.parent_wp_id)
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query(
        "UPDATE wordpress_connections SET last_synced_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE project_id=?",
    )
    .bind(project_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(SyncResult {
        categories: tax.categories.len(),
        tags: tax.tags.len(),
        authors: tax.authors.len(),
        post_types: tax.post_types.len(),
    })
}

#[derive(Debug, sqlx::FromRow)]
struct TermRow {
    wp_id: i64,
    name: String,
    slug: Option<String>,
    parent_wp_id: Option<i64>,
}

pub async fn list_terms(state: &AppState, project_id: &str, kind: &str) -> AppResult<Vec<Term>> {
    if !matches!(kind, "category" | "tag" | "author" | "post_type") {
        return Err(AppError::validation("نوع نامعتبر است."));
    }
    let rows = sqlx::query_as::<_, TermRow>(
        "SELECT wp_id, name, slug, parent_wp_id FROM wp_terms WHERE project_id = ? AND kind = ? ORDER BY name",
    )
    .bind(project_id)
    .bind(kind)
    .fetch_all(&state.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Term { wp_id: r.wp_id, name: r.name, slug: r.slug, parent_wp_id: r.parent_wp_id })
        .collect())
}
