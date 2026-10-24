//! Paid search metrics: real Google Ads data through DataForSEO.
//! Does not infer popularity, competition or CPC from AI answers.
use crate::error::{AppError, AppResult};
use crate::providers::ai::http::build_client;
use crate::services::campaigns;
use crate::state::AppState;
use crate::util::text::normalize;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const LOGIN_KEY: &str = "dataforseo-api-login";
const PASSWORD_KEY: &str = "dataforseo-api-password";
const CONFIG_KEY: &str = "dataforseo-metrics-config";
const ENDPOINT: &str = "https://api.dataforseo.com/v3/keywords_data/google_ads/search_volume/live";

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct MetricsConfig {
    pub configured: bool,
    pub location_code: Option<i64>,
    pub language_code: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMetricsConfig {
    pub login: Option<String>,
    pub password: Option<String>,
    pub location_code: Option<i64>,
    pub language_code: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsResult {
    pub requested: usize,
    pub updated: usize,
    pub without_data: usize,
    pub cost_usd: f64,
}

pub async fn get(state: &AppState) -> AppResult<MetricsConfig> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value_json FROM settings WHERE key=?")
        .bind(CONFIG_KEY).fetch_optional(&state.db).await?;
    let mut config = row.and_then(|r| serde_json::from_str::<MetricsConfig>(&r.0).ok()).unwrap_or_default();
    config.configured = state.secrets.get(LOGIN_KEY)?.is_some() && state.secrets.get(PASSWORD_KEY)?.is_some();
    Ok(config)
}

pub async fn save(state: &AppState, input: SaveMetricsConfig) -> AppResult<MetricsConfig> {
    if let Some(id) = input.location_code {
        if !(1..=999_999).contains(&id) { return Err(AppError::validation("کد کشور یا شهر نامعتبر است.")); }
    }
    if !input.language_code.trim().is_empty() && (input.language_code.len() > 6 ||
        !input.language_code.chars().all(|ch| ch.is_ascii_alphabetic() || ch == '-')) {
        return Err(AppError::validation("کد زبان نامعتبر است؛ مانند fa یا en."));
    }
    if let Some(l) = input.login.as_deref().filter(|s| !s.trim().is_empty()) { state.secrets.set(LOGIN_KEY, l.trim())?; }
    if let Some(p) = input.password.as_deref().filter(|s| !s.trim().is_empty()) { state.secrets.set(PASSWORD_KEY, p)?; }
    if state.secrets.get(LOGIN_KEY)?.is_none() || state.secrets.get(PASSWORD_KEY)?.is_none() {
        return Err(AppError::validation("نام کاربری و رمز API سرویس DataForSEO الزامی است."));
    }
    let settings = MetricsConfig { configured: true, location_code: input.location_code, language_code: input.language_code.trim().to_string() };
    sqlx::query("INSERT INTO settings (key,value_json) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json")
        .bind(CONFIG_KEY).bind(serde_json::to_string(&settings).unwrap_or_default()).execute(&state.db).await?;
    Ok(settings)
}

pub async fn fetch(state: &AppState, campaign_id: &str) -> AppResult<MetricsResult> {
    campaigns::get(state, campaign_id).await?;
    let login = state.secrets.get(LOGIN_KEY)?.ok_or_else(|| AppError::validation("DataForSEO را در بخش ارائه‌دهندگان کلمه کلیدی تنظیم کنید."))?;
    let pass = state.secrets.get(PASSWORD_KEY)?.ok_or_else(|| AppError::validation("رمز DataForSEO ذخیره نشده است."))?;
    let cfg = get(state).await?;
    let records: Vec<(String, String)> = sqlx::query_as("SELECT id, keyword FROM keywords WHERE campaign_id=? AND role='candidate' ORDER BY rowid LIMIT 1000")
        .bind(campaign_id).fetch_all(&state.db).await?;
    if records.is_empty() { return Err(AppError::validation("ابتدا کلمات پیشنهادی کمپین را ایجاد یا وارد کنید.")); }
    let valid: Vec<&(String,String)> = records.iter().filter(|r| r.1.chars().count() <= 80 && r.1.split_whitespace().count() <= 10).collect();
    if valid.is_empty() { return Err(AppError::validation("عبارات برای سرویس Google Ads باید حداکثر ۸۰ حرف و ۱۰ کلمه باشند.")); }
    let mut request = json!({"keywords": valid.iter().map(|r| &r.1).collect::<Vec<_>>(), "search_partners": false});
    if let Some(location) = cfg.location_code { request["location_code"] = json!(location); }
    if !cfg.language_code.is_empty() { request["language_code"] = json!(cfg.language_code); }
    let http = build_client(std::time::Duration::from_secs(120), None).map_err(AppError::from)?;
    let resp = http.post(ENDPOINT).basic_auth(&login, Some(&pass)).json(&json!([request])).send().await
        .map_err(|_| AppError::from_parts("NETWORK", "ارتباط با DataForSEO برقرار نشد.".into(), true))?;
    let status = resp.status();
    let body: Value = resp.json().await.map_err(|_| AppError::validation("پاسخ DataForSEO قابل خواندن نیست."))?;
    if !status.is_success() || body.get("status_code").and_then(Value::as_i64) != Some(20000) {
        let message = body.get("status_message").and_then(Value::as_str).unwrap_or("درخواست توسط DataForSEO رد شد");
        return Err(AppError::validation(format!("DataForSEO: {} ({})", message, status)));
    }
    let task = body.pointer("/tasks/0").ok_or_else(|| AppError::validation("پاسخ DataForSEO فاقد task است."))?;
    if task.get("status_code").and_then(Value::as_i64) != Some(20000) {
        return Err(AppError::validation(format!("DataForSEO: {}", task.get("status_message").and_then(Value::as_str).unwrap_or("task failed"))));
    }
    let results = task.get("result").and_then(Value::as_array).ok_or_else(|| AppError::validation("نتیجه داده‌های کلمات کلیدی خالی است."))?;
    let mut updated = 0;
    let mut tx = state.db.begin().await?;
    for result in results {
        let Some(keyword) = result.get("keyword").and_then(Value::as_str) else { continue; };
        let Some((id, _)) = valid.iter().copied().find(|r| normalize(&r.1) == normalize(keyword)) else { continue; };
        let vol = result.get("search_volume").and_then(Value::as_i64);
        let competition = result.get("competition_index").and_then(Value::as_f64);
        let cpc = result.get("cpc").and_then(Value::as_f64);
        if vol.is_none() && competition.is_none() && cpc.is_none() { continue; }
        let monthly: Vec<i64> = result.get("monthly_searches").and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|r| r.get("search_volume").and_then(Value::as_i64)).collect()).unwrap_or_default();
        sqlx::query("UPDATE keywords SET source='keyword_api', search_volume=?, competition=?, cpc=?, trend_json=? WHERE id=? AND campaign_id=?")
            .bind(vol).bind(competition).bind(cpc).bind(serde_json::to_string(&monthly).unwrap_or_default())
            .bind(id).bind(campaign_id).execute(&mut *tx).await?;
        updated += 1;
    }
    tx.commit().await?;
    Ok(MetricsResult { requested: valid.len(), updated, without_data: valid.len().saturating_sub(updated),
        cost_usd: task.get("cost").and_then(Value::as_f64).unwrap_or(0.0) })
}
