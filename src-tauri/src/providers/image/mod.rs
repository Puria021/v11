pub mod openai_image;

use crate::providers::ProviderError;
use async_trait::async_trait;

pub struct GeneratedImage {
    pub bytes: Vec<u8>,
    pub mime: String,
}

impl GeneratedImage {
    pub fn extension(&self) -> &'static str {
        match self.mime.as_str() {
            "image/jpeg" => "jpg",
            "image/webp" => "webp",
            _ => "png",
        }
    }
}

#[async_trait]
pub trait ImageProvider: Send + Sync {
    async fn generate(&self, prompt: &str, size: &str) -> Result<GeneratedImage, ProviderError>;
}

/// Identifies PNG / JPEG / WEBP from magic bytes.
pub fn sniff_mime(b: &[u8]) -> Option<&'static str> {
    if b.len() >= 8 && b.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if b.len() >= 3 && b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}
