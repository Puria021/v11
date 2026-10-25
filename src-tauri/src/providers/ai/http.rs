//! Shared HTTP plumbing for provider adapters (client building, error mapping).

use crate::providers::{ProviderError, ProviderErrorKind};
use serde_json::Value;
use std::time::Duration;

pub fn build_client(timeout: Duration, proxy: Option<&str>) -> Result<reqwest::Client, ProviderError> {
    let mut b = reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(20))
        .user_agent("MyArticleManager/0.1");
    b = crate::services::network::apply_http(b, proxy)?;
    b.build()
        .map_err(|_| ProviderError::new(ProviderErrorKind::Network, "ساخت کلاینت HTTP ناموفق بود."))
}

pub fn map_send_error(e: reqwest::Error) -> ProviderError {
    if e.is_timeout() {
        ProviderError::new(ProviderErrorKind::Timeout, "زمان انتظار پاسخ سرویس تمام شد.")
    } else if e.is_connect() {
        ProviderError::new(ProviderErrorKind::Network, "اتصال به سرویس برقرار نشد. اینترنت یا Proxy را بررسی کنید.")
    } else {
        ProviderError::new(ProviderErrorKind::Network, "خطای شبکه هنگام ارتباط با سرویس.")
    }
}

fn provider_message(body: &Value) -> Option<String> {
    let m = body.pointer("/error/message").and_then(|v| v.as_str())?;
    Some(m.chars().take(160).collect())
}

/// Sends the request and returns the parsed JSON body, mapping HTTP failures to `ProviderError`.
pub async fn send_json(req: reqwest::RequestBuilder) -> Result<Value, ProviderError> {
    let resp = req.send().await.map_err(map_send_error)?;
    let status = resp.status();
    let retry_after = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let text = resp
        .text()
        .await
        .map_err(|_| ProviderError::new(ProviderErrorKind::Network, "دریافت پاسخ سرویس ناتمام ماند."))?;
    let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);

    if !status.is_success() {
        return Err(status_error(status, retry_after, value));
    }
    Ok(value)
}

/// Maps a non-success HTTP response to a provider error (shared by JSON and streamed calls).
pub fn status_error(status: reqwest::StatusCode, retry_after: Option<u64>, value: Value) -> ProviderError {
    let extra = provider_message(&value).map(|m| format!(" ({m})")).unwrap_or_default();
    let mut err = match status.as_u16() {
        401 | 403 => ProviderError::new(ProviderErrorKind::AuthInvalid, "کلید API نامعتبر است یا دسترسی ندارد."),
        404 => ProviderError::new(ProviderErrorKind::NotFound, format!("آدرس API یا نام مدل پیدا نشد{extra}")),
        429 => ProviderError::new(ProviderErrorKind::RateLimited, "محدودیت تعداد درخواست یا اعتبار سرویس؛ کمی بعد دوباره تلاش کنید."),
        400 | 422 => ProviderError::new(ProviderErrorKind::InvalidResponse, format!("درخواست توسط سرویس رد شد{extra}")),
        s if s >= 500 => ProviderError::new(ProviderErrorKind::Remote, format!("خطای سمت سرویس (کد {s}).")),
        s => ProviderError::new(ProviderErrorKind::Remote, format!("پاسخ نامنتظره از سرویس (کد {s}).")),
    };
    err.status = Some(status.as_u16());
    err.retry_after_secs = retry_after;
    err.body = Some(value);
    err
}

pub fn empty_reply() -> ProviderError {
    ProviderError::new(ProviderErrorKind::InvalidResponse, "سرویس AI پاسخ خالی برگرداند.")
}
