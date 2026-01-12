use thiserror::Error;

/// Format the ImportFailed error message with issues
fn format_import_failed(message: &str, issues: &[String]) -> String {
    if issues.is_empty() {
        format!("plan import failed: {message}")
    } else {
        format!(
            "plan import failed: {message}\n  - {}",
            issues.join("\n  - ")
        )
    }
}

/// Format the InvalidField error message
fn format_invalid_field(field: &str, valid_fields: &[String]) -> String {
    format!(
        "invalid field '{}': must be one of: {}",
        field,
        valid_fields.join(", ")
    )
}

/// Format the InvalidFieldValue error message
fn format_invalid_field_value(field: &str, value: &str, valid_values: &[String]) -> String {
    format!(
        "invalid value '{}' for field '{}': must be one of: {}",
        value,
        field,
        valid_values.join(", ")
    )
}

#[derive(Error, Debug)]
pub enum JanusError {
    #[error("ticket '{0}' not found")]
    TicketNotFound(String),

    #[error("ambiguous ID '{0}' matches multiple tickets")]
    AmbiguousId(String),

    #[error("plan '{0}' not found")]
    PlanNotFound(String),

    #[error("ambiguous plan ID '{0}' matches multiple plans")]
    AmbiguousPlanId(String),

    #[error("phase '{0}' not found in plan")]
    PhaseNotFound(String),

    #[error("ticket '{0}' is already in this plan")]
    TicketAlreadyInPlan(String),

    #[error("ticket '{0}' is already in phase '{1}'")]
    TicketAlreadyInPhase(String, String),

    #[error("ticket '{0}' not found in plan")]
    TicketNotInPlan(String),

    #[error("phase '{0}' contains tickets - use --force or --migrate")]
    PhaseNotEmpty(String),

    #[error("cannot add ticket to simple plan with --phase option")]
    SimpleplanNoPhase,

    #[error("phased plan requires --phase option")]
    PhasedPlanRequiresPhase,

    #[error("cannot move ticket in a simple plan (no phases)")]
    CannotMoveInSimplePlan,

    #[error("invalid ticket format: {0}")]
    InvalidFormat(String),

    #[error("invalid status '{0}'")]
    InvalidStatus(String),

    #[error("{}", format_invalid_field(.field, .valid_fields))]
    InvalidField {
        field: String,
        valid_fields: Vec<String>,
    },

    #[error("{}", format_invalid_field_value(.field, .value, .valid_values))]
    InvalidFieldValue {
        field: String,
        value: String,
        valid_values: Vec<String>,
    },

    #[error("invalid prefix '{0}': {1}")]
    InvalidPrefix(String, String),

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
    #[error("cache database corrupted: {0}")]
    CacheCorrupted(String),

    #[error("cache database version mismatch: expected {expected}, found {found}")]
    CacheVersionMismatch { expected: String, found: String },

    #[error("cannot access cache directory: {0}")]
    CacheAccessDenied(std::path::PathBuf),

    #[error("cache database error: {0}")]
    CacheDatabase(#[from] turso::Error),

    #[error("cache serde JSON error: {0}")]
    CacheSerdeJson(serde_json::Error),

    #[error("cache operation failed: {0}")]
    CacheOther(String),

    // Plan import errors
    #[error("{}", format_import_failed(.message, .issues))]
    ImportFailed {
        message: String,
        issues: Vec<String>,
    },

    #[error("plan with title '{0}' already exists ({1})")]
    DuplicatePlanTitle(String, String), // title, existing plan ID

    #[error("--verbose-phase can only be used with phased plans")]
    VerbosePhaseRequiresPhasedPlan,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, JanusError>;
