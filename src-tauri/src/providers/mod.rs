//! External-service adapters. Each family (WordPress now; AI / Keyword / Image later)
//! is a trait + adapters, so a new provider never touches the core.

pub mod ai;
pub mod image;
pub mod wordpress;

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderErrorKind {
    AuthInvalid,
    RateLimited,
    Timeout,
    Network,
    InvalidResponse,
    NotFound,
    Remote,
}

#[derive(Debug, Clone)]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub message: String,
    pub status: Option<u16>,
    pub retry_after_secs: Option<u64>,
    /// Parsed error body from the remote service, when it sent JSON.
    pub body: Option<serde_json::Value>,
}

impl ProviderError {
    pub fn new(kind: ProviderErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), status: None, retry_after_secs: None, body: None }
    }
    pub fn retryable(&self) -> bool {
        matches!(
            self.kind,
            ProviderErrorKind::RateLimited
                | ProviderErrorKind::Timeout
                | ProviderErrorKind::Network
                | ProviderErrorKind::Remote
        )
    }
}

impl From<ProviderError> for AppError {
    fn from(e: ProviderError) -> Self {
        let retryable = e.retryable();
        let code = match e.kind {
            ProviderErrorKind::AuthInvalid => "AUTH_INVALID",
            ProviderErrorKind::RateLimited => "RATE_LIMITED",
            ProviderErrorKind::Timeout => "TIMEOUT",
            ProviderErrorKind::Network => "NETWORK",
            ProviderErrorKind::InvalidResponse => "INVALID_RESPONSE",
            ProviderErrorKind::NotFound => "NOT_FOUND",
            ProviderErrorKind::Remote => "REMOTE",
        };
        AppError::from_parts(code, e.message, retryable)
    }
}
