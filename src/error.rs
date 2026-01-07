use crate::cache_error::CacheError;
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

    // Remote sync errors
    #[error("invalid remote reference '{0}': {1}")]
    InvalidRemoteRef(String, String),

    #[error("remote issue not found: {0}")]
    RemoteIssueNotFound(String),

    #[error("ticket already linked to remote: {0}")]
    AlreadyLinked(String),

    #[error("ticket not linked to any remote")]
    NotLinked,

    #[error("configuration error: {0}")]
    Config(String),

    #[error("authentication error: {0}")]
    Auth(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    // Cache errors
    #[error("cache error: {0}")]
    Cache(#[from] CacheError),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, JanusError>;
