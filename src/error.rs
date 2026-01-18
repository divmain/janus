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

    #[error("rate limit exceeded. Please wait {0} seconds before retrying.")]
    RateLimited(u64),

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

    // Hook errors
    #[error("pre-hook '{hook_name}' failed with exit code {exit_code}: {message}")]
    PreHookFailed {
        hook_name: String,
        exit_code: i32,
        message: String,
    },

    #[error("post-hook '{hook_name}' failed: {message}")]
    PostHookFailed { hook_name: String, message: String },

    #[error("hook script not found: {0}")]
    HookScriptNotFound(std::path::PathBuf),

    #[error("hook '{hook_name}' timed out after {seconds} seconds")]
    HookTimeout { hook_name: String, seconds: u64 },

    #[error("invalid hook event '{0}'")]
    InvalidHookEvent(String),

    #[error("hook recipe '{0}' not found")]
    HookRecipeNotFound(String),

    #[error("failed to fetch hook: {0}")]
    HookFetchFailed(String),

    #[error("hook security violation: {0}")]
    HookSecurity(String),

    #[error("{0}")]
    Other(String),

    #[error("cache data integrity error: {0}")]
    CacheDataIntegrity(String),
}

pub type Result<T> = std::result::Result<T, JanusError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_pre_hook_failed_error_message() {
        let error = JanusError::PreHookFailed {
            hook_name: "pre-write.sh".to_string(),
            exit_code: 42,
            message: "validation failed".to_string(),
        };
        let msg = error.to_string();
        assert!(msg.contains("pre-write.sh"));
        assert!(msg.contains("42"));
        assert!(msg.contains("validation failed"));
    }

    #[test]
    fn test_post_hook_failed_error_message() {
        let error = JanusError::PostHookFailed {
            hook_name: "post-write.sh".to_string(),
            message: "notification failed".to_string(),
        };
        let msg = error.to_string();
        assert!(msg.contains("post-write.sh"));
        assert!(msg.contains("notification failed"));
    }

    #[test]
    fn test_hook_script_not_found_error_message() {
        let error = JanusError::HookScriptNotFound(PathBuf::from("/path/to/missing.sh"));
        let msg = error.to_string();
        assert!(msg.contains("missing.sh"));
        assert!(msg.contains("not found"));
    }

    #[test]
    fn test_hook_timeout_error_message() {
        let error = JanusError::HookTimeout {
            hook_name: "slow-hook.sh".to_string(),
            seconds: 30,
        };
        let msg = error.to_string();
        assert!(msg.contains("slow-hook.sh"));
        assert!(msg.contains("30"));
        assert!(msg.contains("timed out"));
    }

    #[test]
    fn test_invalid_hook_event_error_message() {
        let error = JanusError::InvalidHookEvent("bad_event".to_string());
        let msg = error.to_string();
        assert!(msg.contains("bad_event"));
        assert!(msg.contains("invalid"));
    }

    #[test]
    fn test_hook_recipe_not_found_error_message() {
        let error = JanusError::HookRecipeNotFound("nonexistent-recipe".to_string());
        let msg = error.to_string();
        assert!(msg.contains("nonexistent-recipe"));
        assert!(msg.contains("not found"));
    }

    #[test]
    fn test_hook_fetch_failed_error_message() {
        let error = JanusError::HookFetchFailed("connection refused".to_string());
        let msg = error.to_string();
        assert!(msg.contains("connection refused"));
        assert!(msg.contains("failed to fetch"));
    }
}
