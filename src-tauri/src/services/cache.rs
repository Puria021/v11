//! Removes locally cached media and temporary files from the app data folder.
//!
//! Images of articles that already exist in WordPress are no longer needed locally, so they are deleted
//! and their database link is cleared. Images of articles that still have to be uploaded are kept,
//! otherwise those uploads would fail.

use crate::error::AppResult;
use crate::state::AppState;
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheReport {
    pub files_removed: usize,
    pub bytes_freed: u64,
    /// Images kept because their article is not in WordPress yet.
    pub kept_pending: usize,
}

pub async fn clear(state: &AppState) -> AppResult<CacheReport> {
    let mut report = CacheReport { files_removed: 0, bytes_freed: 0, kept_pending: 0 };

    // 1) Images of articles already in WordPress.
    let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT i.id, i.local_path, p.status, w.upload_status
         FROM article_images i
         JOIN article_plans p ON p.id = i.plan_id
         LEFT JOIN wordpress_posts w ON w.plan_id = p.id
         WHERE i.local_path IS NOT NULL",
    )
    .fetch_all(&state.db)
    .await?;
    for (id, rel, status, upload) in rows {
        let in_wordpress = status == "DRAFT_CREATED" || upload.as_deref() == Some("UPLOADED");
        if !in_wordpress {
            report.kept_pending += 1;
            continue;
        }
        remove_file(state, &rel, &mut report);
        sqlx::query("UPDATE article_images SET local_path=NULL WHERE id=?").bind(&id).execute(&state.db).await?;
    }

    // 2) Files in the projects folder that no database row refers to any more.
    let referenced: HashSet<String> = sqlx::query_as::<_, (String,)>("SELECT local_path FROM article_images WHERE local_path IS NOT NULL")
        .fetch_all(&state.db)
        .await?
        .into_iter()
        .map(|r| r.0.replace('\\', "/"))
        .collect();
    let mut files = Vec::new();
    walk(&state.data_dir.join("projects"), &mut files);
    for abs in files {
        let rel = relative(&state.data_dir, &abs);
        if !referenced.contains(&rel) {
            remove_path(&abs, &mut report);
        }
    }

    // 3) Temporary and cache folders.
    for dir in ["tmp", "cache"] {
        let p = state.data_dir.join(dir);
        if p.is_dir() {
            let mut inner = Vec::new();
            walk(&p, &mut inner);
            for f in inner {
                remove_path(&f, &mut report);
            }
            let _ = std::fs::remove_dir_all(&p);
        }
    }
    Ok(report)
}

fn relative(root: &Path, abs: &Path) -> String {
    abs.strip_prefix(root).unwrap_or(abs).to_string_lossy().replace('\\', "/")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() { walk(&p, out) } else { out.push(p) }
    }
}

fn remove_file(state: &AppState, rel: &str, report: &mut CacheReport) {
    remove_path(&state.data_dir.join(rel), report);
}

fn remove_path(abs: &Path, report: &mut CacheReport) {
    let size = std::fs::metadata(abs).map(|m| m.len()).unwrap_or(0);
    if std::fs::remove_file(abs).is_ok() {
        report.files_removed += 1;
        report.bytes_freed += size;
    }
}
