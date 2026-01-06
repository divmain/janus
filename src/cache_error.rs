use thiserror::Error;

#[derive(Error, Debug)]
pub enum CacheError {
    #[error("cache database corrupted: {0}")]
    Corrupted(String),

    #[error("cache database version mismatch: expected {expected}, found {found}")]
    VersionMismatch { expected: String, found: String },

    #[error("cannot access cache directory: {0}")]
    AccessDenied(std::path::PathBuf),

    #[error("database error: {0}")]
    Database(#[from] turso::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde JSON error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("cache operation failed: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CacheError>;
