//! OpenAI-compatible Chat Completions client (OpenAI, OpenRouter, local gateways, ...).

use super::http::{build_client, empty_reply, map_send_error, status_error};
use super::{AiProvider, TextRequest};
use crate::providers::{ProviderError, ProviderErrorKind};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Duration;

pub struct OpenAiCompatible {
    base: String,
    api_key: String,
    model: String,
    temperature: f64,
    max_tokens: u32,
    http: reqwest::Client,
}

impl OpenAiCompatible {
    pub fn new(
        base: &str,
        api_key: &str,
        model: &str,
        temperature: f64,
        max_tokens: u32,
        timeout: Duration,
        proxy: Option<&str>,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            temperature,
            max_tokens,
            http: build_client(timeout, proxy)?,
        })
    }
}

/// Total time allowed for one streamed article. Long articles stream for minutes, so this is
/// much larger than the per-request timeout; the gateway sees data flowing and does not cut it.
const STREAM_TOTAL_TIMEOUT: Duration = Duration::from_secs(900);

#[async_trait]
impl AiProvider for OpenAiCompatible {
    async fn generate_text(&self, req: TextRequest) -> Result<String, ProviderError> {
        // Streamed (stream: true): tokens arrive in chunks, which avoids 504 from gateways that cut
        // long silent responses. The text is assembled from the `data:` lines.
        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": req.system},
                {"role": "user", "content": req.user}
            ],
            "temperature": req.temperature.unwrap_or(self.temperature),
            "max_tokens": req.max_tokens.unwrap_or(self.max_tokens),
            "stream": true,
        });
        let mut resp = self
            .http
            .post(format!("{}/chat/completions", self.base))
            .bearer_auth(&self.api_key)
            .json(&body)
            .timeout(STREAM_TOTAL_TIMEOUT)
            .send()
            .await
            .map_err(map_send_error)?;

        let status = resp.status();
        if !status.is_success() {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let text = resp.text().await.unwrap_or_default();
            let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            return Err(status_error(status, retry_after, value));
        }

        // A plain JSON reply (no event stream) is accepted too.
        let is_stream = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.contains("text/event-stream"))
            .unwrap_or(false);
        if !is_stream {
            let text = resp.text().await.map_err(|_| {
                ProviderError::new(ProviderErrorKind::Network, "دریافت پاسخ سرویس ناتمام ماند.")
            })?;
            let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            let out = match value.pointer("/choices/0/message/content") {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Array(parts)) => parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join(""),
                _ => String::new(),
            };
            if out.trim().is_empty() {
                return Err(empty_reply());
            }
            return Ok(out);
        }

        let mut out = String::new();
        let mut pending: Vec<u8> = Vec::new();
        loop {
            let chunk = resp.chunk().await.map_err(|_| {
                ProviderError::new(ProviderErrorKind::Network, "دریافت پاسخ سرویس ناتمام ماند.")
            })?;
            let Some(chunk) = chunk else { break };
            pending.extend_from_slice(&chunk);
            while let Some(pos) = pending.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = pending.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&line);
                let Some(data) = line.trim().strip_prefix("data:") else { continue };
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
                if v.get("error").is_some() {
                    return Err(status_error(reqwest::StatusCode::BAD_GATEWAY, None, v));
                }
                if let Some(piece) = v.pointer("/choices/0/delta/content").and_then(|c| c.as_str()) {
                    out.push_str(piece);
                }
            }
        }
        if out.trim().is_empty() {
            return Err(empty_reply());
        }
        Ok(out)
    }
}
