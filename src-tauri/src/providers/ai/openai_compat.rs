//! OpenAI-compatible Chat Completions client (OpenAI, OpenRouter, local gateways, ...).

use super::http::{build_client, empty_reply, send_json};
use super::{AiProvider, TextRequest};
use crate::providers::ProviderError;
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

#[async_trait]
impl AiProvider for OpenAiCompatible {
    async fn generate_text(&self, req: TextRequest) -> Result<String, ProviderError> {
        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": req.system},
                {"role": "user", "content": req.user}
            ],
            "temperature": req.temperature.unwrap_or(self.temperature),
            "max_tokens": req.max_tokens.unwrap_or(self.max_tokens),
        });
        let value = send_json(
            self.http.post(format!("{}/chat/completions", self.base)).bearer_auth(&self.api_key).json(&body),
        )
        .await?;
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
        Ok(out)
    }
}
