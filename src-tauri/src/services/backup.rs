//! Export / import of projects as a single `.aiproj` file (a zip with JSON + generated images).
//! API keys and passwords are never exported: they live in the Windows Credential Manager only.

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use serde::Serialize;
use serde_json::{json, Map, Value};
use sqlx::{Column, Row, SqlitePool, TypeInfo, ValueRef};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const FORMAT: &str = "aiproj";
const VERSION: i64 = 1;
const MAX_DATA_JSON: u64 = 512 * 1024 * 1024;

const P: &str = "(? IS NULL OR project_id = ?)";

fn tables() -> Vec<(&'static str, String)> {
    let by_plan = format!("plan_id IN (SELECT id FROM article_plans WHERE {P})");
    vec![
        ("article_templates", "SELECT * FROM article_templates".into()),
        ("ai_providers", "SELECT * FROM ai_providers WHERE id NOT IN ('builtin-text','builtin-image')".into()),
        ("image_providers", "SELECT * FROM image_providers WHERE id NOT IN ('builtin-text','builtin-image')".into()),
        ("projects", "SELECT * FROM projects WHERE (? IS NULL OR id = ?)".into()),
        ("wordpress_connections", format!("SELECT * FROM wordpress_connections WHERE {P}")),
        ("wp_terms", format!("SELECT * FROM wp_terms WHERE {P}")),
        ("campaigns", format!("SELECT * FROM campaigns WHERE {P}")),
        ("keywords", format!("SELECT * FROM keywords WHERE {P}")),
        ("article_plans", format!("SELECT * FROM article_plans WHERE {P}")),
        ("article_titles", format!("SELECT * FROM article_titles WHERE {by_plan}")),
        ("article_contents", format!("SELECT * FROM article_contents WHERE {by_plan}")),
        ("article_images", format!("SELECT * FROM article_images WHERE {by_plan}")),
        ("wordpress_posts", format!("SELECT * FROM wordpress_posts WHERE {P}")),
    ]
}

async fn dump(db: &SqlitePool, sql: &str, project: &Option<String>) -> AppResult<Vec<Value>> {
    let mut q = sqlx::query(sql);
    for _ in 0..sql.matches('?').count() {
        q = q.bind(project.clone());
    }
    let rows = q.fetch_all(db).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let mut m = Map::new();
        for (i, col) in row.columns().iter().enumerate() {
            let raw = row.try_get_raw(i)?;
            let v = if raw.is_null() {
                Value::Null
            } else {
                match col.type_info().name() {
                    "INTEGER" | "BOOLEAN" | "INT8" | "INT" => json!(row.try_get::<i64, _>(i)?),
                    "REAL" | "FLOAT" | "DOUBLE" => json!(row.try_get::<f64, _>(i)?),
                    _ => match row.try_get::<String, _>(i) {
                        Ok(s) => Value::String(s),
                        Err(_) => Value::Null,
                    },
                }
            };
            m.insert(col.name().to_string(), v);
        }
        out.push(Value::Object(m));
    }
    Ok(out)
}

fn zerr<E: std::fmt::Display>(e: E) -> AppError {
    tracing::error!("backup error: {e}");
    AppError::internal("خواندن یا نوشتن فایل پشتیبان ناموفق بود.")
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub bytes: u64,
    pub projects: usize,
    pub campaigns: usize,
    pub articles: usize,
    pub images: usize,
}

pub async fn export(state: &AppState, path: Option<String>, project_id: Option<String>) -> AppResult<ExportResult> {
    let project = project_id.filter(|s| !s.is_empty());
    let mut data = Map::new();
    for (name, sql) in tables() {
        data.insert(name.to_string(), Value::Array(dump(&state.db, &sql, &project).await?));
    }
    let rows = |t: &str| data.get(t).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    if let Some(pid) = &project {
        if rows("projects").is_empty() {
            return Err(AppError::not_found(format!("پروژه‌ای با شناسه‌ی {pid} پیدا نشد.")));
        }
    }
    let count_by = |t: &str, id: &str| rows(t).iter().filter(|r| r["project_id"] == *id).count();
    let projects: Vec<Value> = rows("projects")
        .iter()
        .map(|p| {
            let id = p["id"].as_str().unwrap_or("");
            json!({"id": id, "name": p["name"], "campaigns": count_by("campaigns", id), "articles": count_by("article_plans", id)})
        })
        .collect();
    let image_files: Vec<String> = rows("article_images").iter().filter_map(|r| r["local_path"].as_str().map(String::from)).collect();
    let (stamp,): (String,) = sqlx::query_as("SELECT strftime('%Y%m%d-%H%M%S','now')").fetch_one(&state.db).await?;
    let (created,): (String,) = sqlx::query_as("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')").fetch_one(&state.db).await?;
    let manifest = json!({
        "format": FORMAT, "version": VERSION, "appVersion": env!("CARGO_PKG_VERSION"), "createdAt": created,
        "scope": if project.is_some() { "project" } else { "all" },
        "projects": projects,
        "templates": rows("article_templates").len(), "aiProviders": rows("ai_providers").len(),
        "imageProviders": rows("image_providers").len(), "images": image_files.len(),
    });

    let target: PathBuf = match path.filter(|p| !p.trim().is_empty()) {
        Some(p) => PathBuf::from(p),
        None => state.data_dir.join("exports").join(format!("backup-{stamp}.aiproj")),
    };
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = std::fs::File::create(&target).map_err(zerr)?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file("manifest.json", opts).map_err(zerr)?;
    zip.write_all(manifest.to_string().as_bytes()).map_err(zerr)?;
    zip.start_file("data.json", opts).map_err(zerr)?;
    zip.write_all(Value::Object(data.clone()).to_string().as_bytes()).map_err(zerr)?;
    let mut included = 0;
    for rel in &image_files {
        if let Ok(bytes) = std::fs::read(state.data_dir.join(rel)) {
            zip.start_file(format!("files/{rel}"), opts.compression_method(CompressionMethod::Stored)).map_err(zerr)?;
            zip.write_all(&bytes).map_err(zerr)?;
            included += 1;
        }
    }
    zip.finish().map_err(zerr)?;
    let bytes = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
    Ok(ExportResult {
        path: target.display().to_string(),
        bytes,
        projects: rows("projects").len(),
        campaigns: rows("campaigns").len(),
        articles: rows("article_plans").len(),
        images: included,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewProject {
    pub id: String,
    pub name: String,
    pub campaigns: i64,
    pub articles: i64,
    pub exists: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreview {
    pub created_at: String,
    pub app_version: String,
    pub scope: String,
    pub projects: Vec<PreviewProject>,
    pub templates: i64,
    pub ai_providers: i64,
    pub image_providers: i64,
    pub images: i64,
}

fn open(path: &str) -> AppResult<ZipArchive<std::fs::File>> {
    let f = std::fs::File::open(path).map_err(|_| AppError::validation("فایل پشتیبان باز نشد."))?;
    ZipArchive::new(f).map_err(|_| AppError::validation("این فایل یک پشتیبان معتبر (.aiproj) نیست."))
}

fn read_manifest(zip: &mut ZipArchive<std::fs::File>) -> AppResult<Value> {
    let mut s = String::new();
    zip.by_name("manifest.json")
        .map_err(|_| AppError::validation("این فایل یک پشتیبان معتبر (.aiproj) نیست."))?
        .read_to_string(&mut s)
        .map_err(zerr)?;
    let m: Value = serde_json::from_str(&s).map_err(|_| AppError::validation("فایل پشتیبان آسیب دیده است."))?;
    if m["format"] != FORMAT {
        return Err(AppError::validation("این فایل یک پشتیبان معتبر (.aiproj) نیست."));
    }
    if m["version"].as_i64().unwrap_or(i64::MAX) > VERSION {
        return Err(AppError::validation("این پشتیبان با نسخه‌ی جدیدتری از برنامه ساخته شده است."));
    }
    Ok(m)
}

pub async fn preview(state: &AppState, path: &str) -> AppResult<BackupPreview> {
    let mut zip = open(path)?;
    let m = read_manifest(&mut zip)?;
    let mut projects = Vec::new();
    for p in m["projects"].as_array().cloned().unwrap_or_default() {
        let id = p["id"].as_str().unwrap_or("").to_string();
        let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM projects WHERE id=?").bind(&id).fetch_optional(&state.db).await?;
        projects.push(PreviewProject {
            name: p["name"].as_str().unwrap_or("").to_string(),
            campaigns: p["campaigns"].as_i64().unwrap_or(0),
            articles: p["articles"].as_i64().unwrap_or(0),
            exists: exists.is_some(),
            id,
        });
    }
    Ok(BackupPreview {
        created_at: m["createdAt"].as_str().unwrap_or("").to_string(),
        app_version: m["appVersion"].as_str().unwrap_or("").to_string(),
        scope: m["scope"].as_str().unwrap_or("").to_string(),
        projects,
        templates: m["templates"].as_i64().unwrap_or(0),
        ai_providers: m["aiProviders"].as_i64().unwrap_or(0),
        image_providers: m["imageProviders"].as_i64().unwrap_or(0),
        images: m["images"].as_i64().unwrap_or(0),
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub projects_imported: usize,
    pub projects_skipped: Vec<String>,
    pub campaigns: usize,
    pub articles: usize,
    pub files: usize,
}

fn safe_relative(p: &str) -> bool {
    let path = Path::new(p);
    p.starts_with("projects/") && path.components().all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// `mode`: "skip" keeps projects that already exist; "replace" deletes and re-imports them.
pub async fn import(state: &AppState, path: &str, mode: &str) -> AppResult<ImportResult> {
    if !matches!(mode, "skip" | "replace") {
        return Err(AppError::validation("حالت ورود نامعتبر است."));
    }
    let mut zip = open(path)?;
    read_manifest(&mut zip)?;
    let data: Value = {
        let entry = zip.by_name("data.json").map_err(|_| AppError::validation("فایل پشتیبان آسیب دیده است."))?;
        if entry.size() > MAX_DATA_JSON {
            return Err(AppError::validation("فایل پشتیبان بیش از حد بزرگ است."));
        }
        serde_json::from_reader(entry).map_err(|_| AppError::validation("فایل پشتیبان آسیب دیده است."))?
    };
    let rows = |t: &str| data.get(t).and_then(|v| v.as_array()).cloned().unwrap_or_default();

    // Decide which projects come in.
    let mut accepted: HashSet<String> = HashSet::new();
    let mut skipped = Vec::new();
    for p in rows("projects") {
        let id = p["id"].as_str().unwrap_or("").to_string();
        let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM projects WHERE id=?").bind(&id).fetch_optional(&state.db).await?;
        if exists.is_some() && mode == "skip" {
            skipped.push(p["name"].as_str().unwrap_or("").to_string());
        } else {
            accepted.insert(id);
        }
    }
    let plan_ids: HashSet<String> = rows("article_plans")
        .iter()
        .filter(|r| r["project_id"].as_str().map(|p| accepted.contains(p)).unwrap_or(false))
        .filter_map(|r| r["id"].as_str().map(String::from))
        .collect();

    let mut tx = state.db.begin().await?;
    if mode == "replace" {
        for id in &accepted {
            sqlx::query("DELETE FROM projects WHERE id=?").bind(id).execute(&mut *tx).await?;
        }
    }
    let (mut campaigns, mut articles) = (0usize, 0usize);
    let mut files: Vec<String> = Vec::new();
    for (table, _) in tables() {
        let global = matches!(table, "article_templates" | "ai_providers" | "image_providers");
        let allowed: Vec<(String,)> = sqlx::query_as(&format!("SELECT name FROM pragma_table_info('{table}')")).fetch_all(&mut *tx).await?;
        let allowed: HashSet<String> = allowed.into_iter().map(|a| a.0).collect();
        for row in rows(table) {
            let Value::Object(m) = &row else { continue };
            let keep = if global {
                true
            } else if table == "projects" {
                m.get("id").and_then(|v| v.as_str()).map(|i| accepted.contains(i)).unwrap_or(false)
            } else if matches!(table, "article_titles" | "article_contents" | "article_images") {
                m.get("plan_id").and_then(|v| v.as_str()).map(|i| plan_ids.contains(i)).unwrap_or(false)
            } else {
                m.get("project_id").and_then(|v| v.as_str()).map(|i| accepted.contains(i)).unwrap_or(false)
            };
            if !keep {
                continue;
            }
            if m.keys().any(|k| !allowed.contains(k)) {
                return Err(AppError::validation("فایل پشتیبان ستون ناشناخته دارد و وارد نشد."));
            }
            let cols: Vec<&String> = m.keys().collect();
            let sql = format!(
                "INSERT {} INTO \"{}\" ({}) VALUES ({})",
                if global { "OR IGNORE" } else { "" },
                table,
                cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(","),
                vec!["?"; cols.len()].join(",")
            );
            let mut q = sqlx::query(&sql);
            for c in &cols {
                q = match &m[*c] {
                    Value::Null => q.bind(None::<String>),
                    Value::Bool(b) => q.bind(*b as i64),
                    Value::Number(n) => match n.as_i64() {
                        Some(i) => q.bind(i),
                        None => q.bind(n.as_f64()),
                    },
                    Value::String(s) => q.bind(s.clone()),
                    other => q.bind(other.to_string()),
                };
            }
            q.execute(&mut *tx).await?;
            match table {
                "campaigns" => campaigns += 1,
                "article_plans" => articles += 1,
                "article_images" => {
                    if let Some(p) = m.get("local_path").and_then(|v| v.as_str()) {
                        files.push(p.to_string());
                    }
                }
                _ => {}
            }
        }
    }
    tx.commit().await?;

    let mut extracted = 0;
    for rel in files {
        if !safe_relative(&rel) {
            continue;
        }
        if let Ok(mut entry) = zip.by_name(&format!("files/{rel}")) {
            let mut buf = Vec::new();
            if entry.read_to_end(&mut buf).is_ok() {
                let dest = state.data_dir.join(&rel);
                if let Some(dir) = dest.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(dest, buf)?;
                extracted += 1;
            }
        }
    }
    Ok(ImportResult { projects_imported: accepted.len(), projects_skipped: skipped, campaigns, articles, files: extracted })
}
