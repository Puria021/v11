//! OpenAI-style Images API (`/images/generations`). Works with OpenAI and compatible gateways;
//! accepts both `b64_json` and `url` responses.

use super::{sniff_mime, GeneratedImage, ImageProvider};
use crate::providers::ai::http::{build_client, map_send_error, send_json};
use crate::providers::{ProviderError, ProviderErrorKind};
use async_trait::async_trait;
use base64::Engine;
use serde_json::json;
use std::time::Duration;

const MAX_IMAGE_BYTES: usize = 25 * 1024 * 1024;

pub struct OpenAiImage {
    base: String,
    api_key: String,
    model: String,
    output_format: String,
    image_quality: String,
    http: reqwest::Client,
}

impl OpenAiImage {
    pub fn new(base: &str, api_key: &str, model: &str, timeout: Duration, proxy: Option<&str>, output_format: &str, image_quality: &str) -> Result<Self, ProviderError> {
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            output_format: output_format.to_string(),
            image_quality: image_quality.to_string(),
            http: build_client(timeout, proxy)?,
        })
    }
}

fn bad(msg: &str) -> ProviderError {
    ProviderError::new(ProviderErrorKind::InvalidResponse, msg)
}

#[async_trait]
impl ImageProvider for OpenAiImage {
    async fn generate(&self, prompt: &str, size: &str) -> Result<GeneratedImage, ProviderError> {
        let is_gpt_image = self.model.starts_with("gpt-image-");
        let is_dalle3 = self.model == "dall-e-3";
        let is_dalle2 = self.model == "dall-e-2";
        let selected_size = if size == "template" { "auto" } else { size };
        let allowed_gpt = ["auto", "1024x1024", "1024x1536", "1536x1024"];
        let allowed_dalle3 = ["1024x1024", "1792x1024", "1024x1792"];
        let allowed_dalle2 = ["256x256", "512x512", "1024x1024"];
        if is_gpt_image && !allowed_gpt.contains(&selected_size) {
            return Err(bad("ابعاد انتخاب‌شده برای مدل GPT Image پشتیبانی نمی‌شود. از Auto یا 1024x1024 یا 1536x1024 یا 1024x1536 استفاده کنید."));
        }
        if is_dalle3 && !allowed_dalle3.contains(&selected_size) {
            return Err(bad("ابعاد DALL-E 3 باید 1024x1024 یا 1792x1024 یا 1024x1792 باشد."));
        }
        if is_dalle2 && !allowed_dalle2.contains(&selected_size) {
            return Err(bad("ابعاد DALL-E 2 باید 256x256 یا 512x512 یا 1024x1024 باشد."));
        }
        let mut body = json!({"model": self.model, "prompt": prompt, "n": 1});
        if !(selected_size == "auto" && !is_gpt_image) {
            body["size"] = json!(selected_size);
        }
        if is_gpt_image {
            body["output_format"] = json!(self.output_format);
            body["quality"] = json!(self.image_quality);
        } else if is_dalle3 {
            if self.image_quality != "auto" {
                return Err(bad("مدل DALL-E 3 از Low/Medium/High پشتیبانی نمی‌کند؛ کیفیت را روی Auto قرار دهید."));
            }
            if self.output_format != "png" {
                return Err(bad("فرمت JPEG و WebP برای DALL-E 3 قابل درخواست نیست. PNG را انتخاب کنید."));
            }
        } else if is_dalle2 && (self.output_format != "png" || self.image_quality != "auto") {
            return Err(bad("برای DALL-E 2 فرمت PNG و کیفیت Auto را انتخاب کنید."));
        }
        let value = send_json(
            self.http.post(format!("{}/images/generations", self.base)).bearer_auth(&self.api_key).json(&body),
        )
        .await?;
        let item = value.pointer("/data/0").ok_or_else(|| bad("پاسخ سرویس تصویر خالی بود."))?;

        let bytes = if let Some(b64) = item.get("b64_json").and_then(|v| v.as_str()) {
            base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .map_err(|_| bad("داده‌ی تصویر نامعتبر بود."))?
        } else if let Some(url) = item.get("url").and_then(|v| v.as_str()) {
            let resp = self.http.get(url).send().await.map_err(map_send_error)?;
            if !resp.status().is_success() {
                return Err(ProviderError::new(ProviderErrorKind::Remote, "دانلود تصویر تولیدشده ناموفق بود."));
            }
            resp.bytes().await.map_err(map_send_error)?.to_vec()
        } else {
            return Err(bad("پاسخ سرویس تصویر شامل تصویر نبود."));
        };
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(bad("حجم تصویر بیش از حد مجاز است."));
        }
        let mime = sniff_mime(&bytes).ok_or_else(|| bad("فرمت تصویر دریافتی پشتیبانی نمی‌شود."))?;
        Ok(GeneratedImage { bytes, mime: mime.to_string() })
    }
}
