//! Application error type. Serialized to the frontend as
//! `{ code, message, retryable, details? }`. Messages are user-facing (Persian)
//! and MUST NOT contain secrets.

use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    fn new(code: &'static str, message: impl Into<String>, retryable: bool) -> Self {
        Self { code, message: message.into(), retryable, details: None }
    }
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::new("VALIDATION", msg, false)
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new("NOT_FOUND", msg, false)
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::new("CONFLICT", msg, false)
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new("INTERNAL", msg, false)
    }
    pub fn secret_store(msg: impl Into<String>) -> Self {
        Self::new("INTERNAL", msg, false)
    }
    pub fn offline() -> Self {
        Self::new("OFFLINE", "اتصال اینترنت برقرار نیست. این عملیات نیاز به اینترنت دارد.", true)
    }
    pub fn from_parts(code: &'static str, message: String, retryable: bool) -> Self {
        Self::new(code, message, retryable)
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!("database error: {e}");
        Self::new("DB", "خطا در پایگاه داده محلی.", false)
    }
}

impl From<sqlx::migrate::MigrateError> for AppError {
    fn from(e: sqlx::migrate::MigrateError) -> Self {
        // No tracing subscriber is installed in the release app; keep the real reason in startup.log.
        crate::crash_log(&format!("migration error: {e:?}"));
        Self::new("DB", "خطا در به‌روزرسانی ساختار پایگاه داده.", false)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        tracing::error!("io error: {e}");
        Self::new("INTERNAL", "خطا در دسترسی به فایل‌های برنامه.", false)
    }
}
