pub mod anthropic;
pub mod gemini;
pub mod http;
pub mod openai_compat;

use crate::providers::ProviderError;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct TextRequest {
    pub system: String,
    pub user: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn generate_text(&self, req: TextRequest) -> Result<String, ProviderError>;
    // Phase 4 adds structured output + image-prompt helpers on top of this.
}
