//! Anthropic Messages API client.

use super::http::{build_client, empty_reply, send_json};
use super::{AiProvider, TextRequest};
use crate::providers::ProviderError;
use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;

pub struct Anthropic {
    base: String,
    api_key: String,
    model: String,
    temperature: f64,
    max_tokens: u32,
    http: reqwest::Client,
}

impl Anthropic {
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

#[async_trait]
impl AiProvider for Anthropic {
    async fn generate_text(&self, req: TextRequest) -> Result<String, ProviderError> {
        let body = json!({
            "model": self.model,
            "max_tokens": req.max_tokens.unwrap_or(self.max_tokens),
            // Anthropic accepts 0..=1 only.
            "temperature": req.temperature.unwrap_or(self.temperature).clamp(0.0, 1.0),
            "system": req.system,
            "messages": [{"role": "user", "content": req.user}],
        });
        let value = send_json(
            self.http
                .post(format!("{}/v1/messages", self.base))
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&body),
        )
        .await?;
        let out: String = value
            .get("content")
            .and_then(|c| c.as_array())
            .map(|parts| {
                parts
                    .iter()
                    .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        if out.trim().is_empty() {
            return Err(empty_reply());
        }
        Ok(out)
    }
}
