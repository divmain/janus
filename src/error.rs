use thiserror::Error;

#[derive(Error, Debug)]
pub enum JanusError {
    #[error("ticket '{0}' not found")]
    TicketNotFound(String),

    #[error("ambiguous ID '{0}' matches multiple tickets")]
    AmbiguousId(String),

    #[error("invalid ticket format: {0}")]
    InvalidFormat(String),

    #[error("invalid status '{0}'")]
    InvalidStatus(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parse error: {0}")]
    YamlParse(#[from] serde_yaml_ng::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("jq filter error: {0}")]
    JqFilter(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, JanusError>;
