use crate::error::{AppError, AppResult};
use crate::providers::ai::anthropic::Anthropic;
use crate::providers::ai::gemini::Gemini;
use crate::providers::ai::openai_compat::OpenAiCompatible;
use crate::providers::ai::{AiProvider, TextRequest};
use crate::providers::{ProviderError, ProviderErrorKind};
use crate::state::AppState;
use crate::util::json::extract_json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use url::Url;

const OPENAI_BASE: &str = "https://api.openai.com/v1";
const ANTHROPIC_BASE: &str = "https://api.anthropic.com";
const GEMINI_BASE: &str = "https://generativelanguage.googleapis.com";

#[derive(Debug, sqlx::FromRow)]
struct Row {
    id: String,
    name: String,
    kind: String,
    base_url: Option<String>,
    model: String,
    temperature: f64,
    max_tokens: i64,
    timeout_sec: i64,
    proxy_url: Option<String>,
    secret_ref: Option<String>,
    rpm_limit: Option<i64>,
    max_concurrent: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderView {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: Option<String>,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: i64,
    pub timeout_sec: i64,
    pub proxy_url: Option<String>,
    pub rpm_limit: Option<i64>,
    pub max_concurrent: i64,
    pub has_secret: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAiProvider {
    pub id: Option<String>,
    pub name: String,
    pub kind: String,
    pub base_url: Option<String>,
    pub model: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
    pub timeout_sec: Option<i64>,
    pub proxy_url: Option<String>,
    pub rpm_limit: Option<i64>,
    pub max_concurrent: Option<i64>,
    /// Empty/None on update keeps the stored key.
    pub api_key: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTest {
    pub ok: bool,
    pub latency_ms: u64,
}

const COLS: &str = "id, name, kind, base_url, model, temperature, max_tokens, timeout_sec, proxy_url, secret_ref, rpm_limit, max_concurrent";

fn secret_key(id: &str) -> String {
    format!("ai-key:{id}")
}

fn view(state: &AppState, r: Row) -> AppResult<AiProviderView> {
    let has_secret = match &r.secret_ref {
        Some(k) => state.secrets.get(k)?.is_some(),
        None => false,
    };
    Ok(AiProviderView {
        id: r.id,
        name: r.name,
        kind: r.kind,
        base_url: r.base_url,
        model: r.model,
        temperature: r.temperature,
        max_tokens: r.max_tokens,
        timeout_sec: r.timeout_sec,
        proxy_url: r.proxy_url,
        rpm_limit: r.rpm_limit,
        max_concurrent: r.max_concurrent,
        has_secret,
    })
}

pub async fn list(state: &AppState) -> AppResult<Vec<AiProviderView>> {
    let rows = sqlx::query_as::<_, Row>(&format!("SELECT {COLS} FROM ai_providers WHERE id NOT IN ('builtin-text','builtin-image') ORDER BY created_at, rowid"))
        .fetch_all(&state.db)
        .await?;
    rows.into_iter().map(|r| view(state, r)).collect()
}

pub async fn save(state: &AppState, input: SaveAiProvider) -> AppResult<AiProviderView> {
    if input.id.as_deref().map(super::license::is_builtin).unwrap_or(false) {
        return Err(AppError::validation("این ارائه‌دهنده توسط برنامه مدیریت می‌شود."));
    }
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::validation("نام ارائه‌دهنده الزامی است."));
    }
    if !matches!(input.kind.as_str(), "openai" | "openai_compatible" | "anthropic" | "gemini") {
        return Err(AppError::validation("نوع ارائه‌دهنده نامعتبر است."));
    }
    let model = input.model.trim().to_string();
    if model.is_empty() {
        return Err(AppError::validation("نام مدل الزامی است."));
    }
    let base_url = match input.base_url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(u) => {
            let parsed = Url::parse(u).map_err(|_| AppError::validation("Base URL نامعتبر است."))?;
            if !matches!(parsed.scheme(), "http" | "https") {
                return Err(AppError::validation("Base URL باید با http یا https شروع شود."));
            }
            Some(u.trim_end_matches('/').to_string())
        }
        None if input.kind == "openai_compatible" => {
            return Err(AppError::validation("برای OpenAI Compatible، Base URL الزامی است."))
        }
        None => None,
    };
    let temperature = input.temperature.unwrap_or(0.7);
    let max_tokens = input.max_tokens.unwrap_or(4000);
    let timeout = input.timeout_sec.unwrap_or(120);
    if !(0.0..=2.0).contains(&temperature) {
        return Err(AppError::validation("Temperature باید بین ۰ و ۲ باشد."));
    }
    if !(1..=32000).contains(&max_tokens) {
        return Err(AppError::validation("Max Tokens باید بین ۱ و ۳۲۰۰۰ باشد."));
    }
    if !(5..=600).contains(&timeout) {
        return Err(AppError::validation("Timeout باید بین ۵ تا ۶۰۰ ثانیه باشد."));
    }
    let proxy = input.proxy_url.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let rpm = input.rpm_limit.filter(|r| *r > 0);
    if rpm.map(|r| r > 10_000).unwrap_or(false) {
        return Err(AppError::validation("محدودیت درخواست در دقیقه نامعتبر است."));
    }
    let max_conc = input.max_concurrent.unwrap_or(3);
    if !(1..=20).contains(&max_conc) {
        return Err(AppError::validation("حداکثر درخواست همزمان باید بین ۱ تا ۲۰ باشد."));
    }

    let id = match &input.id {
        Some(id) => {
            let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM ai_providers WHERE id = ?")
                .bind(id)
                .fetch_optional(&state.db)
                .await?;
            exists.ok_or_else(|| AppError::not_found("ارائه‌دهنده پیدا نشد."))?;
            id.clone()
        }
        None => ulid::Ulid::new().to_string(),
    };
    let key = secret_key(&id);
    match input.api_key.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(k) => state.secrets.set(&key, k)?,
        None => {
            if state.secrets.get(&key)?.is_none() {
                return Err(AppError::validation("کلید API الزامی است."));
            }
        }
    }

    sqlx::query(
        "INSERT INTO ai_providers (id, name, kind, base_url, model, temperature, max_tokens, timeout_sec, proxy_url, secret_ref, rpm_limit, max_concurrent)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, kind=excluded.kind, base_url=excluded.base_url,
           model=excluded.model, temperature=excluded.temperature, max_tokens=excluded.max_tokens,
           timeout_sec=excluded.timeout_sec, proxy_url=excluded.proxy_url, secret_ref=excluded.secret_ref,
           rpm_limit=excluded.rpm_limit, max_concurrent=excluded.max_concurrent,
           updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
    )
    .bind(&id)
    .bind(&name)
    .bind(&input.kind)
    .bind(&base_url)
    .bind(&model)
    .bind(temperature)
    .bind(max_tokens)
    .bind(timeout)
    .bind(proxy)
    .bind(&key)
    .bind(rpm)
    .bind(max_conc)
    .execute(&state.db)
    .await?;

    let row = sqlx::query_as::<_, Row>(&format!("SELECT {COLS} FROM ai_providers WHERE id = ?"))
        .bind(&id)
        .fetch_one(&state.db)
        .await?;
    view(state, row)
}

pub async fn delete(state: &AppState, id: &str) -> AppResult<()> {
    if super::license::is_builtin(id) {
        return Err(AppError::validation("این ارائه‌دهنده توسط برنامه مدیریت می‌شود."));
    }
    let res = sqlx::query("DELETE FROM ai_providers WHERE id = ?").bind(id).execute(&state.db).await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("ارائه‌دهنده پیدا نشد."));
    }
    state.secrets.delete(&secret_key(id))?;
    Ok(())
}

struct Built {
    provider: Box<dyn AiProvider>,
    id: String,
    rpm: Option<i64>,
    max_conc: i64,
    /// The provider's own "Max Tokens" setting; it is the ceiling for every request.
    max_tokens: u32,
}

async fn build(state: &AppState, id: &str) -> AppResult<Built> {
    let r = sqlx::query_as::<_, Row>(&format!("SELECT {COLS} FROM ai_providers WHERE id = ?"))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::validation("ارائه‌دهنده‌ی AI انتخاب‌شده پیدا نشد."))?;
    let key = r
        .secret_ref
        .as_deref()
        .map(|k| state.secrets.get(k))
        .transpose()?
        .flatten()
        .ok_or_else(|| AppError::validation(format!("کلید API برای «{}» ذخیره نشده است.", r.name)))?;
    let timeout = Duration::from_secs(r.timeout_sec as u64);
    let proxy = r.proxy_url.as_deref();
    let max_tokens = r.max_tokens as u32;
    let provider: Box<dyn AiProvider> = match r.kind.as_str() {
        "anthropic" => {
            let base = r.base_url.clone().unwrap_or_else(|| ANTHROPIC_BASE.to_string());
            Box::new(Anthropic::new(&base, &key, &r.model, r.temperature, max_tokens, timeout, proxy)?)
        }
        "gemini" => {
            let base = r.base_url.clone().unwrap_or_else(|| GEMINI_BASE.to_string());
            Box::new(Gemini::new(&base, &key, &r.model, r.temperature, max_tokens, timeout, proxy)?)
        }
        _ => {
            let base = r.base_url.clone().unwrap_or_else(|| OPENAI_BASE.to_string());
            Box::new(OpenAiCompatible::new(&base, &key, &r.model, r.temperature, max_tokens, timeout, proxy)?)
        }
    };
    Ok(Built { provider, id: r.id, rpm: r.rpm_limit, max_conc: r.max_concurrent, max_tokens })
}

impl Built {
    /// One rate-limited request.
    async fn call(&self, state: &AppState, req: TextRequest) -> Result<String, ProviderError> {
        let _permit = state.limits.acquire(&self.id, self.rpm, self.max_conc).await;
        self.provider.generate_text(req).await
    }
}

fn refine(state: &AppState, e: ProviderError) -> AppError {
    if matches!(e.kind, ProviderErrorKind::Network | ProviderErrorKind::Timeout) && !state.online.load(Ordering::Relaxed) {
        return AppError::offline();
    }
    e.into()
}

pub async fn test(state: &AppState, id: &str) -> AppResult<ProviderTest> {
    let p = build(state, id).await?;
    let t = Instant::now();
    p.call(state, TextRequest {
        system: "You are a connectivity check.".into(),
        user: "Reply with the single word OK.".into(),
        temperature: Some(0.0),
        max_tokens: Some(16),
    })
    .await
    .map_err(|e| refine(state, e))?;
    Ok(ProviderTest { ok: true, latency_ms: t.elapsed().as_millis() as u64 })
}

/// Runs a prompt and parses JSON out of the reply. One automatic retry if the reply isn't valid JSON;
/// transient provider errors (rate limit / timeout / 5xx) are retried twice with a short backoff.
pub async fn complete_json(
    state: &AppState,
    provider_id: &str,
    system: &str,
    user: &str,
    max_tokens: Option<u32>,
) -> AppResult<Value> {
    let p = build(state, provider_id).await?;
    let max_tokens = Some(max_tokens.map_or(p.max_tokens, |m| m.min(p.max_tokens)));
    let mut reminder = "";
    for attempt in 0..2 {
        let req = TextRequest {
            system: format!("{system}{reminder}"),
            user: user.to_string(),
            temperature: None,
            max_tokens,
        };
        let mut tries = 0;
        let text = loop {
            match p.call(state, req.clone()).await {
                Ok(t) => break t,
                Err(e) if e.retryable() && tries < 2 => {
                    tries += 1;
                    let wait = match e.retry_after_secs {
                        Some(s) => Duration::from_secs(s.min(20)),
                        None => state.backoff_base.mul_f64(2f64.powi(tries as i32 - 1)).min(Duration::from_secs(20)),
                    };
                    tokio::time::sleep(wait).await;
                }
                Err(e) => return Err(refine(state, e)),
            }
        };
        if let Some(v) = extract_json(&text) {
            return Ok(v);
        }
        if attempt == 0 {
            reminder = "\n\nIMPORTANT: your previous reply was not valid JSON. Reply with ONLY the JSON, no prose, no code fences.";
        }
    }
    Err(AppError::from_parts(
        "INVALID_RESPONSE",
        "پاسخ AI در قالب JSON معتبر نبود. دوباره تلاش کنید یا مدل دیگری انتخاب کنید.".into(),
        true,
    ))
}
