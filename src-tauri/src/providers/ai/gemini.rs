//! Google Gemini (generateContent) client.

use super::http::{build_client, empty_reply, send_json};
use super::{AiProvider, TextRequest};
use crate::providers::{ProviderError, ProviderErrorKind};
use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;

pub struct Gemini {
    base: String,
    api_key: String,
    model: String,
    temperature: f64,
    max_tokens: u32,
    http: reqwest::Client,
}

impl Gemini {
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
            model: model.trim_start_matches("models/").to_string(),
            temperature,
            max_tokens,
            http: build_client(timeout, proxy)?,
        })
    }
}

#[async_trait]
impl AiProvider for Gemini {
    async fn generate_text(&self, req: TextRequest) -> Result<String, ProviderError> {
        let body = json!({
            "systemInstruction": {"parts": [{"text": req.system}]},
            "contents": [{"role": "user", "parts": [{"text": req.user}]}],
            "generationConfig": {
                "temperature": req.temperature.unwrap_or(self.temperature),
                "maxOutputTokens": req.max_tokens.unwrap_or(self.max_tokens),
            }
        });
        let value = send_json(
            self.http
                .post(format!("{}/v1beta/models/{}:generateContent", self.base, self.model))
                .header("x-goog-api-key", &self.api_key)
                .json(&body),
        )
        .await?;
        let out: String = value
            .pointer("/candidates/0/content/parts")
            .and_then(|p| p.as_array())
            .map(|parts| parts.iter().filter_map(|p| p.get("text").and_then(|t| t.as_str())).collect::<Vec<_>>().join(""))
            .unwrap_or_default();
        if out.trim().is_empty() {
            if value.pointer("/promptFeedback/blockReason").is_some() {
                return Err(ProviderError::new(ProviderErrorKind::InvalidResponse, "درخواست توسط فیلتر ایمنی Gemini مسدود شد."));
            }
            return Err(empty_reply());
        }
        Ok(out)
    }
}
