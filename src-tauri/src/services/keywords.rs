use crate::error::{AppError, AppResult};
use crate::services::{ai, campaigns, campaigns::CampaignView, logs};
use crate::state::AppState;
use crate::util::json::string_items;
use crate::util::text::{guess_intent, jaccard, language_name, normalize, parse_number, trigrams, valid_intent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{QueryBuilder, Sqlite, SqliteConnection};
use std::collections::HashSet;

const MAX_CSV_BYTES: usize = 5 * 1024 * 1024;
const MAX_CSV_ROWS: usize = 20_000;

// ---------------------------------------------------------------- DTOs

#[derive(Debug, sqlx::FromRow)]
struct KeywordRow {
    id: String,
    keyword: String,
    role: String,
    source: String,
    search_volume: Option<i64>,
    competition: Option<f64>,
    cpc: Option<f64>,
    trend_json: Option<String>,
    related_json: Option<String>,
    intent: Option<String>,
    is_selected: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Keyword {
    pub id: String,
    pub keyword: String,
    pub role: String,
    pub source: String,
    pub search_volume: Option<i64>,
    pub competition: Option<f64>,
    pub cpc: Option<f64>,
    pub trend: Option<Value>,
    pub related: Vec<String>,
    pub intent: Option<String>,
    pub is_selected: bool,
}

impl From<KeywordRow> for Keyword {
    fn from(r: KeywordRow) -> Self {
        Keyword {
            id: r.id,
            keyword: r.keyword,
            role: r.role,
            source: r.source,
            search_volume: r.search_volume,
            competition: r.competition,
            cpc: r.cpc,
            trend: r.trend_json.and_then(|t| serde_json::from_str(&t).ok()),
            related: r.related_json.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default(),
            intent: r.intent,
            is_selected: r.is_selected != 0,
        }
    }
}

const KW_COLS: &str =
    "id, keyword, role, source, search_volume, competition, cpc, trend_json, related_json, intent, is_selected";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: usize,
    pub updated: usize,
    pub skipped: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestResult {
    pub requested: usize,
    pub added: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeywordQuery {
    pub campaign_id: String,
    pub role: Option<String>,
    pub search: Option<String>,
    pub source: Option<String>,
    pub selected_only: Option<bool>,
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeywordPage {
    pub items: Vec<Keyword>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SimilarityWarning {
    pub a_id: String,
    pub a_keyword: String,
    pub b_id: String,
    pub b_keyword: String,
    pub score: f64,
    pub same_intent: bool,
    pub message: String,
}

// ---------------------------------------------------------------- insert helper

pub struct NewKeyword {
    pub keyword: String,
    pub role: &'static str,
    pub source: &'static str,
    pub search_volume: Option<i64>,
    pub competition: Option<f64>,
    pub cpc: Option<f64>,
    pub trend_json: Option<String>,
    pub related_json: Option<String>,
    pub intent: Option<String>,
    pub is_selected: bool,
}

impl NewKeyword {
    pub fn plain(keyword: &str, role: &'static str, source: &'static str, intent: Option<String>) -> Self {
        Self {
            keyword: keyword.trim().to_string(),
            role,
            source,
            search_volume: None,
            competition: None,
            cpc: None,
            trend_json: None,
            related_json: None,
            intent,
            is_selected: false,
        }
    }
}

/// Inserts a keyword unless the same normalized text already exists for (campaign, role).
/// Returns Ok(true) when a row was added. AI-sourced rows can never carry search metrics.
pub async fn insert_keyword(
    conn: &mut SqliteConnection,
    c: &CampaignView,
    nk: NewKeyword,
) -> AppResult<bool> {
    if nk.source == "ai" && (nk.search_volume.is_some() || nk.competition.is_some() || nk.cpc.is_some()) {
        return Err(AppError::internal("AI نباید داده‌ی جستجو تولید کند."));
    }
    let norm = normalize(&nk.keyword);
    if norm.is_empty() {
        return Ok(false);
    }
    let intent = nk
        .intent
        .as_deref()
        .and_then(valid_intent)
        .unwrap_or_else(|| guess_intent(&nk.keyword));
    let res = sqlx::query(
        "INSERT OR IGNORE INTO keywords (id, project_id, campaign_id, keyword, norm, role, source, search_volume, competition,
           cpc, trend_json, related_json, country, language, intent, is_selected)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(ulid::Ulid::new().to_string())
    .bind(&c.project_id)
    .bind(&c.id)
    .bind(&nk.keyword)
    .bind(&norm)
    .bind(nk.role)
    .bind(nk.source)
    .bind(nk.search_volume)
    .bind(nk.competition)
    .bind(nk.cpc)
    .bind(&nk.trend_json)
    .bind(&nk.related_json)
    .bind(&c.country)
    .bind(&c.language)
    .bind(intent)
    .bind(nk.is_selected as i64)
    .execute(&mut *conn)
    .await?;
    Ok(res.rows_affected() > 0)
}

// ---------------------------------------------------------------- CSV

pub struct CsvRow {
    pub keyword: String,
    pub volume: Option<i64>,
    pub competition: Option<f64>,
    pub cpc: Option<f64>,
    pub trend: Option<Vec<f64>>,
    pub related: Vec<String>,
}

fn split_line(line: &str, delim: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_q {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_q = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_q = true;
        } else if c == delim {
            out.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    out.push(cur.trim().to_string());
    out
}

fn header_kind(h: &str) -> Option<&'static str> {
    let n = normalize(h).replace(' ', "_");
    match n.as_str() {
        "keyword" | "keywords" | "کلمه_کلیدی" | "کلمه" | "عبارت" | "query" => Some("keyword"),
        "search_volume" | "volume" | "avg_monthly_searches" | "حجم_جستجو" | "حجم" => Some("volume"),
        "competition" | "competition_(indexed_value)" | "competition_level" | "رقابت" | "difficulty" | "kd" => Some("competition"),
        "cpc" | "top_of_page_bid_(low_range)" | "avg_cpc" | "cost_per_click" | "هزینه_کلیک" => Some("cpc"),
        "trend" | "روند" => Some("trend"),
        "related" | "related_keywords" | "کلمات_مرتبط" | "مرتبط" => Some("related"),
        _ => None,
    }
}

/// Parses CSV text (comma / semicolon / tab separated). A header row with a keyword column is
/// expected; a file with a single column and no recognised header is read as one keyword per line.
pub fn parse_csv(text: &str) -> AppResult<Vec<CsvRow>> {
    let text = text.trim_start_matches('\u{feff}');
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return Err(AppError::validation("فایل CSV خالی است."));
    }
    if lines.len() > MAX_CSV_ROWS + 1 {
        return Err(AppError::validation("تعداد ردیف‌های CSV بیش از حد مجاز است."));
    }
    let delim = [',', ';', '\t']
        .into_iter()
        .max_by_key(|d| lines[0].matches(*d).count())
        .unwrap_or(',');
    let first = split_line(lines[0], delim);
    let kinds: Vec<Option<&'static str>> = first.iter().map(|h| header_kind(h)).collect();
    let has_header = kinds.contains(&Some("keyword"));
    if !has_header && first.len() > 1 {
        return Err(AppError::validation(
            "ستون «keyword» (یا «کلمه کلیدی») در سطر اول CSV پیدا نشد.",
        ));
    }
    let col = |k: &str| kinds.iter().position(|x| *x == Some(k));
    let (ik, iv, ic, ip, it, ir) = (col("keyword"), col("volume"), col("competition"), col("cpc"), col("trend"), col("related"));

    let body = if has_header { &lines[1..] } else { &lines[..] };
    let mut rows = Vec::new();
    for line in body {
        let f = split_line(line, delim);
        let keyword = if has_header { ik.and_then(|i| f.get(i)).cloned().unwrap_or_default() } else { f[0].clone() };
        if keyword.trim().is_empty() {
            continue;
        }
        let get = |i: Option<usize>| i.and_then(|i| f.get(i)).map(|s| s.as_str());
        rows.push(CsvRow {
            keyword: keyword.trim().to_string(),
            volume: get(iv).and_then(parse_number).map(|v| v.round() as i64).filter(|v| *v >= 0),
            competition: get(ic).and_then(parse_number),
            cpc: get(ip).and_then(parse_number),
            trend: get(it).map(|s| s.split(['|', ' ']).filter_map(parse_number).collect::<Vec<_>>()).filter(|v| !v.is_empty()),
            related: get(ir)
                .map(|s| s.split(['|', '،']).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
                .unwrap_or_default(),
        });
    }
    if rows.is_empty() {
        return Err(AppError::validation("هیچ کلمه‌ی کلیدی معتبری در CSV پیدا نشد."));
    }
    Ok(rows)
}

/// Convenience wrapper for tests.
pub async fn import_keywords_csv_for_test(state: &AppState, campaign_id: &str, csv: &str) -> ImportResult {
    import_csv(state, campaign_id, csv).await.expect("csv import")
}

// ---------------------------------------------------------------- commands' logic

pub async fn save_primary(state: &AppState, campaign_id: &str, seeds: Vec<String>) -> AppResult<Vec<Keyword>> {
    let c = campaigns::get(state, campaign_id).await?;
    let mut seen = HashSet::new();
    let cleaned: Vec<String> = seeds
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && seen.insert(normalize(s)))
        .collect();
    if cleaned.is_empty() {
        return Err(AppError::validation("حداقل یک کلمه‌ی کلیدی اصلی وارد کنید."));
    }
    if cleaned.len() > 50 {
        return Err(AppError::validation("حداکثر ۵۰ کلمه‌ی کلیدی اصلی مجاز است."));
    }
    let mut tx = state.db.begin().await?;
    let keep: HashSet<String> = cleaned.iter().map(|s| normalize(s)).collect();
    let existing: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id, norm FROM keywords WHERE campaign_id = ? AND role = 'primary'")
            .bind(campaign_id)
            .fetch_all(&mut *tx)
            .await?;
    for (id, norm) in existing {
        if !norm.as_deref().map(|n| keep.contains(n)).unwrap_or(false) {
            sqlx::query("DELETE FROM keywords WHERE id = ?").bind(id).execute(&mut *tx).await?;
        }
    }
    for s in &cleaned {
        let mut nk = NewKeyword::plain(s, "primary", "user", None);
        nk.is_selected = true;
        insert_keyword(&mut tx, &c, nk).await?;
    }
    tx.commit().await?;
    campaigns::set_step(state, campaign_id, 4).await?;
    campaigns::touch_project(state, &c.project_id).await;
    let rows = sqlx::query_as::<_, KeywordRow>(&format!(
        "SELECT {KW_COLS} FROM keywords WHERE campaign_id = ? AND role = 'primary' ORDER BY rowid"
    ))
    .bind(campaign_id)
    .fetch_all(&state.db)
    .await?;
    Ok(rows.into_iter().map(Keyword::from).collect())
}

pub async fn import_csv(state: &AppState, campaign_id: &str, csv: &str) -> AppResult<ImportResult> {
    if csv.len() > MAX_CSV_BYTES {
        return Err(AppError::validation("حجم فایل CSV بیش از ۵ مگابایت است."));
    }
    let c = campaigns::get(state, campaign_id).await?;
    let rows = parse_csv(csv)?;
    let mut res = ImportResult { imported: 0, updated: 0, skipped: 0 };
    let mut tx = state.db.begin().await?;
    for r in rows {
        let norm = normalize(&r.keyword);
        if norm.is_empty() {
            res.skipped += 1;
            continue;
        }
        let trend_json = r.trend.as_ref().map(|t| serde_json::to_string(t).unwrap_or_default());
        let related_json = if r.related.is_empty() { None } else { Some(serde_json::to_string(&r.related).unwrap_or_default()) };
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT id FROM keywords WHERE campaign_id=? AND role='candidate' AND norm=?")
                .bind(campaign_id)
                .bind(&norm)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some((id,)) = existing {
            // The user's file is the authority for metrics; it also upgrades an AI-sourced row to 'user'.
            sqlx::query(
                "UPDATE keywords SET source='user', search_volume=COALESCE(?, search_volume), competition=COALESCE(?, competition),
                 cpc=COALESCE(?, cpc), trend_json=COALESCE(?, trend_json), related_json=COALESCE(?, related_json) WHERE id=?",
            )
            .bind(r.volume)
            .bind(r.competition)
            .bind(r.cpc)
            .bind(&trend_json)
            .bind(&related_json)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            res.updated += 1;
        } else {
            let mut nk = NewKeyword::plain(&r.keyword, "candidate", "user", None);
            nk.search_volume = r.volume;
            nk.competition = r.competition;
            nk.cpc = r.cpc;
            nk.trend_json = trend_json;
            nk.related_json = related_json;
            if insert_keyword(&mut tx, &c, nk).await? {
                res.imported += 1;
            } else {
                res.skipped += 1;
            }
        }
    }
    tx.commit().await?;
    logs::record(state, "INFO", Some(&c.project_id), Some(&c.id), "keyword_csv_import", "SUCCESS",
        &format!("imported={} updated={} skipped={}", res.imported, res.updated, res.skipped)).await;
    campaigns::touch_project(state, &c.project_id).await;
    Ok(res)
}

pub async fn add_manual(state: &AppState, campaign_id: &str, keyword: &str) -> AppResult<Keyword> {
    let c = campaigns::get(state, campaign_id).await?;
    let kw = keyword.trim();
    if kw.is_empty() || kw.chars().count() > 200 {
        return Err(AppError::validation("کلمه‌ی کلیدی نامعتبر است."));
    }
    let mut conn = state.db.acquire().await?;
    let mut nk = NewKeyword::plain(kw, "candidate", "user", None);
    nk.is_selected = true;
    if !insert_keyword(&mut conn, &c, nk).await? {
        return Err(AppError::conflict("این کلمه‌ی کلیدی قبلاً در کمپین وجود دارد."));
    }
    let row = sqlx::query_as::<_, KeywordRow>(&format!(
        "SELECT {KW_COLS} FROM keywords WHERE campaign_id=? AND role='candidate' AND norm=?"
    ))
    .bind(campaign_id)
    .bind(normalize(kw))
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.into())
}

/// Add a secondary phrase manually without exceeding the campaign target.
pub async fn add_secondary(state: &AppState, campaign_id: &str, keyword: &str) -> AppResult<Keyword> {
    let c = campaigns::get(state, campaign_id).await?;
    let kw = keyword.trim();
    if kw.is_empty() || kw.chars().count() > 200 { return Err(AppError::validation("کلمه کلیدی باید بین ۱ تا ۲۰۰ نویسه باشد.")); }
    let (plans,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM article_plans WHERE campaign_id=?").bind(campaign_id).fetch_one(&state.db).await?;
    if plans > 0 { return Err(AppError::conflict("برای ویرایش کلمات، ابتدا برنامه مقالات را بازنشانی کنید.")); }
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM keywords WHERE campaign_id=? AND role='secondary'").bind(campaign_id).fetch_one(&state.db).await?;
    if count >= c.target_count { return Err(AppError::conflict("ظرفیت کلمات ثانویه تکمیل شده؛ ابتدا موردی را حذف کنید.")); }
    let mut conn = state.db.acquire().await?;
    if !insert_keyword(&mut conn, &c, NewKeyword::plain(kw, "secondary", "user", None)).await? {
        return Err(AppError::conflict("این کلمه قبلاً در کلمات ثانویه ثبت شده است."));
    }
    let row: KeywordRow = sqlx::query_as(&format!("SELECT {KW_COLS} FROM keywords WHERE campaign_id=? AND role='secondary' AND norm=?"))
        .bind(campaign_id).bind(normalize(kw)).fetch_one(&mut *conn).await?;
    Ok(row.into())
}

/// Edit manual or AI secondary/candidate phrase; protects existing plans and uniqueness.
pub async fn edit_keyword(state: &AppState, campaign_id: &str, id: &str, keyword: &str) -> AppResult<()> {
    let kw = keyword.trim();
    if kw.is_empty() || kw.chars().count() > 200 { return Err(AppError::validation("عبارت نامعتبر است.")); }
    let old: Option<(String, String)> = sqlx::query_as("SELECT keyword, role FROM keywords WHERE id=? AND campaign_id=?")
        .bind(id).bind(campaign_id).fetch_optional(&state.db).await?;
    let (previous, role) = old.ok_or_else(|| AppError::not_found("کلمه مورد نظر پیدا نشد."))?;
    if role != "secondary" && role != "candidate" { return Err(AppError::validation("ویرایش کلمه اصلی از مرحله کلمات اصلی انجام می‌شود.")); }
    let (plans,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM article_plans WHERE campaign_id=?").bind(campaign_id).fetch_one(&state.db).await?;
    if plans > 0 { return Err(AppError::conflict("برای ویرایش کلمات، ابتدا برنامه مقالات را بازنشانی کنید.")); }
    if normalize(&previous) == normalize(kw) && previous == kw { return Ok(()); }
    // The unique index on (campaign_id, norm, role) protects normalized duplicates.
    let clash: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM keywords WHERE campaign_id=? AND role=? AND norm=? AND id<>?")
        .bind(campaign_id).bind(&role).bind(normalize(kw)).bind(id).fetch_one(&state.db).await?;
    if clash.0 > 0 { return Err(AppError::conflict("این کلمه قبلاً ثبت شده است.")); }
    sqlx::query("UPDATE keywords SET keyword=?, norm=?, source='user', intent=? WHERE id=? AND campaign_id=?")
        .bind(kw).bind(normalize(kw)).bind(crate::util::text::guess_intent(kw)).bind(id).bind(campaign_id).execute(&state.db).await?;
    Ok(())
}

pub async fn primaries(state: &AppState, campaign_id: &str) -> AppResult<Vec<String>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT keyword FROM keywords WHERE campaign_id=? AND role='primary' ORDER BY rowid")
            .bind(campaign_id)
            .fetch_all(&state.db)
            .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn ai_suggest(state: &AppState, campaign_id: &str, count: usize) -> AppResult<SuggestResult> {
    let c = campaigns::get(state, campaign_id).await?;
    let provider = campaigns::require_ai_provider(&c)?;
    let seeds = primaries(state, campaign_id).await?;
    if seeds.is_empty() {
        return Err(AppError::validation("ابتدا کلمات کلیدی اصلی را ذخیره کنید."));
    }
    let count = count.clamp(1, c.target_count.clamp(1, 2000) as usize);
    let system = "You are an SEO keyword strategist. Reply with ONLY valid JSON, no commentary.";
    let user = format!(
        "Language: {lang}. Target country: {country}.\nMain keywords: {seeds}\n\nSuggest {count} related keyword ideas people may search for. \
         Do NOT include search volumes or any numbers about popularity — you do not have that data.\n\
         Return a JSON array: [{{\"keyword\":\"...\",\"intent\":\"informational|commercial|transactional|navigational\"}}]",
        lang = language_name(&c.language),
        country = c.country,
        seeds = serde_json::to_string(&seeds).unwrap_or_default(),
    );
    let v = match ai::complete_json(state, &provider, system, &user, Some(2500)).await {
        Ok(v) => v,
        Err(e) => {
            logs::record(state, "ERROR", Some(&c.project_id), Some(&c.id), "keyword_ai_suggest", "FAILED", &e.message).await;
            return Err(e);
        }
    };
    let items = string_items(&v, &["keyword", "text"], "intent");
    let mut tx = state.db.begin().await?;
    let mut added = 0;
    for (kw, intent) in items.iter().take(count * 2) {
        if kw.chars().count() > 200 {
            continue;
        }
        if insert_keyword(&mut tx, &c, NewKeyword::plain(kw, "candidate", "ai", intent.clone())).await? {
            added += 1;
        }
    }
    tx.commit().await?;
    logs::record(state, "INFO", Some(&c.project_id), Some(&c.id), "keyword_ai_suggest", "SUCCESS", &format!("added={added}")).await;
    Ok(SuggestResult { requested: count, added })
}

pub async fn list(state: &AppState, q: KeywordQuery) -> AppResult<KeywordPage> {
    let page = q.page.unwrap_or(1).max(1);
    let page_size = q.page_size.unwrap_or(100).clamp(1, 500);
    let sort_col = match q.sort_by.as_deref() {
        Some("keyword") => "keyword",
        Some("searchVolume") => "search_volume",
        Some("competition") => "competition",
        Some("cpc") => "cpc",
        Some("intent") => "intent",
        _ => "rowid",
    };
    let dir = if q.sort_dir.as_deref() == Some("desc") { "DESC" } else { "ASC" };

    fn filters<'a>(qb: &mut QueryBuilder<'a, Sqlite>, q: &'a KeywordQuery, search_norm: &'a Option<String>) {
        qb.push(" WHERE campaign_id = ").push_bind(&q.campaign_id);
        if let Some(r) = q.role.as_deref().filter(|r| !r.is_empty()) {
            qb.push(" AND role = ").push_bind(r);
        }
        if let Some(s) = q.source.as_deref().filter(|s| !s.is_empty()) {
            qb.push(" AND source = ").push_bind(s);
        }
        if q.selected_only == Some(true) {
            qb.push(" AND is_selected = 1");
        }
        if let Some(n) = search_norm {
            qb.push(" AND norm LIKE ").push_bind(format!("%{n}%"));
        }
    }
    let search_norm = q.search.as_deref().map(normalize).filter(|s| !s.is_empty());

    let mut count_qb = QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM keywords");
    filters(&mut count_qb, &q, &search_norm);
    let total: i64 = count_qb.build_query_scalar().fetch_one(&state.db).await?;

    let mut qb = QueryBuilder::<Sqlite>::new(format!("SELECT {KW_COLS} FROM keywords"));
    filters(&mut qb, &q, &search_norm);
    // NULL metrics always sort last so real data is visible first.
    qb.push(format!(" ORDER BY ({sort_col} IS NULL), {sort_col} {dir}, rowid ASC LIMIT "));
    qb.push_bind(page_size).push(" OFFSET ").push_bind((page - 1) * page_size);
    let rows: Vec<KeywordRow> = qb.build_query_as().fetch_all(&state.db).await?;
    Ok(KeywordPage { items: rows.into_iter().map(Keyword::from).collect(), total, page, page_size })
}

pub async fn select(state: &AppState, campaign_id: &str, ids: &[String], selected: bool) -> AppResult<usize> {
    let mut tx = state.db.begin().await?;
    let mut n = 0;
    for id in ids {
        let r = sqlx::query("UPDATE keywords SET is_selected=? WHERE id=? AND campaign_id=? AND role='candidate'")
            .bind(selected as i64)
            .bind(id)
            .bind(campaign_id)
            .execute(&mut *tx)
            .await?;
        n += r.rows_affected() as usize;
    }
    tx.commit().await?;
    Ok(n)
}

pub async fn delete(state: &AppState, campaign_id: &str, ids: &[String]) -> AppResult<usize> {
    let mut tx = state.db.begin().await?;
    let mut n = 0;
    for id in ids {
        let used: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM article_plans WHERE keyword_id = ?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if used.0 > 0 {
            return Err(AppError::conflict("این کلمه در برنامه‌ی مقالات استفاده شده است؛ ابتدا برنامه‌ی مقالات را بازنشانی کنید."));
        }
        let r = sqlx::query("DELETE FROM keywords WHERE id=? AND campaign_id=? AND role IN ('candidate','secondary')")
            .bind(id)
            .bind(campaign_id)
            .execute(&mut *tx)
            .await?;
        n += r.rows_affected() as usize;
    }
    tx.commit().await?;
    Ok(n)
}

/// Pairs of primary/secondary keywords that look like the same topic. A pair is reported when its
/// trigram similarity is >= `threshold` and they share an intent, or when it is >= 0.85 regardless.
pub async fn similarity_warnings(state: &AppState, campaign_id: &str, threshold: f64) -> AppResult<Vec<SimilarityWarning>> {
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, keyword, intent FROM keywords WHERE campaign_id=? AND role IN ('primary','secondary') ORDER BY rowid",
    )
    .bind(campaign_id)
    .fetch_all(&state.db)
    .await?;
    let grams: Vec<_> = rows.iter().map(|r| trigrams(&normalize(&r.1))).collect();
    let mut out = Vec::new();
    for i in 0..rows.len() {
        for j in (i + 1)..rows.len() {
            let score = jaccard(&grams[i], &grams[j]);
            let same = rows[i].2.is_some() && rows[i].2 == rows[j].2;
            if (same && score >= threshold) || score >= 0.85 {
                out.push(SimilarityWarning {
                    a_id: rows[i].0.clone(),
                    a_keyword: rows[i].1.clone(),
                    b_id: rows[j].0.clone(),
                    b_keyword: rows[j].1.clone(),
                    score: (score * 100.0).round() / 100.0,
                    same_intent: same,
                    message: "این دو Keyword ممکن است برای یک Search Intent باشند.".into(),
                });
            }
        }
    }
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_with_header_and_persian_numbers() {
        let rows = parse_csv("keyword;search_volume;competition;cpc;related\n\"خرید لپ تاپ\";۱٬۲۰۰;0.4;0.5;لپ تاپ ارزان|لپ تاپ گیمینگ\nبهترین لپ تاپ;;;;\n").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].volume, Some(1200));
        assert_eq!(rows[0].competition, Some(0.4));
        assert_eq!(rows[0].related.len(), 2);
        assert_eq!(rows[1].volume, None);
    }

    #[test]
    fn csv_single_column_and_errors() {
        assert_eq!(parse_csv("a\nb\nc").unwrap().len(), 3);
        assert!(parse_csv("foo,bar\n1,2").is_err());
        assert!(parse_csv("   \n").is_err());
    }
}
