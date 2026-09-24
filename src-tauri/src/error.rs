/// AppVault unified error type.
///
/// All library errors funnel through here so that Tauri commands can
/// serialize them to the frontend as structured JSON.

use serde::Serialize;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", content = "detail")]
pub enum AppError {
    // ── I/O ───────────────────────────────────────────────────────
    #[error("I/O error: {0}")]
    Io(String),

    // ── Database ──────────────────────────────────────────────────
    #[error("Database error: {0}")]
    Database(String),

    // ── Compression ───────────────────────────────────────────────
    #[error("Compression error: {0}")]
    Compression(String),

    // ── Integrity ─────────────────────────────────────────────────
    #[error("Integrity check failed: {0}")]
    IntegrityFailure(String),

    // ── Application state ─────────────────────────────────────────
    #[error("Application is currently running: {0}")]
    AppRunning(String),

    #[error("Application not found: {0}")]
    AppNotFound(String),

    #[error("Application is unsupported: {0}")]
    AppUnsupported(String),

    // ── Storage ───────────────────────────────────────────────────
    #[error("Insufficient storage: need {required} bytes, have {available} bytes")]
    InsufficientStorage { required: u64, available: u64 },

    // ── Permissions ───────────────────────────────────────────────
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    // ── Configuration ─────────────────────────────────────────────
    #[error("Configuration error: {0}")]
    Config(String),

    // ── Security ──────────────────────────────────────────────────
    #[error("Security violation: {0}")]
    Security(String),

    // ── Operations ────────────────────────────────────────────────
    #[error("Operation already in progress for: {0}")]
    AlreadyInProgress(String),

    #[error("Operation cancelled")]
    Cancelled,

    // ── Generic ───────────────────────────────────────────────────
    #[error("{0}")]
    Other(String),
}

// Allow ? from std::io::Error
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

// Allow ? from rusqlite::Error
impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Database(e.to_string())
    }
}

// Allow ? from anyhow::Error
impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Other(e.to_string())
    }
}

// Allow ? from serde_json::Error
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Other(format!("Serialization error: {e}"))
    }
}

// Tauri can serialize AppError directly when commands return `Result<T, AppError>`.
// No custom `InvokeError` conversion is needed here; the blanket Tauri conversion
// would conflict with the framework implementation and cause compile errors.
