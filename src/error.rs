use thiserror::Error;

/// Check if an error indicates database corruption.
///
/// This uses typed error matching on the turso::Error enum to detect corruption
/// errors reliably without depending on error message strings. It inspects both
/// direct turso errors and wrapped errors within JanusError::CacheDatabase.
pub fn is_corruption_error(error: &JanusError) -> bool {
    match error {
        JanusError::CacheDatabase(turso_err) => {
            matches!(
                turso_err,
                turso::Error::Corrupt(_) | turso::Error::NotAdb(_)
            )
        }
        JanusError::CacheCorrupted(_) => true,
        _ => false,
    }
}

/// Check if an error indicates a permission/access denied error.
///
/// This uses typed error matching on the turso::Error enum to detect permission
/// errors reliably without depending on error message strings. It inspects both
/// direct turso errors and wrapped errors within JanusError::CacheDatabase.
pub fn is_permission_error(error: &JanusError) -> bool {
    match error {
        JanusError::CacheDatabase(turso_err) => {
            matches!(turso_err, turso::Error::IoError(kind) if *kind == std::io::ErrorKind::PermissionDenied)
        }
        JanusError::CacheAccessDenied(_) => true,
        JanusError::Io(io_err) => io_err.kind() == std::io::ErrorKind::PermissionDenied,
        _ => false,
    }
}

/// Generic helper to format error messages with a prefix, a key, and a list of items
fn format_error_with_list(prefix: &str, key: &str, label: &str, items: &[String]) -> String {
    format!("{prefix} '{key}': {label} {}", items.join(", "))
}

/// Format with two keys before the list (e.g., "invalid value 'X' for field 'Y'")
fn format_error_double_key(
    prefix1: &str,
    key1: &str,
    prefix2: &str,
    key2: &str,
    label: &str,
    items: &[String],
) -> String {
    format!(
        "{prefix1} '{key1}' {prefix2} '{key2}': {label} {}",
        items.join(", ")
    )
}

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
    format_error_with_list("invalid field", field, "must be one of:", valid_fields)
}

/// Format the InvalidFieldValue error message
fn format_invalid_field_value(field: &str, value: &str, valid_values: &[String]) -> String {
    format_error_double_key(
        "invalid value",
        value,
        "for field",
        field,
        "must be one of:",
        valid_values,
    )
}

/// Format the AmbiguousId error message
fn format_ambiguous_id(id: &str, matches: &[String]) -> String {
    format_error_with_list("ambiguous ID", id, "matches multiple tickets:", matches)
}

/// Format the AmbiguousPlanId error message
fn format_ambiguous_plan_id(id: &str, matches: &[String]) -> String {
    format_error_with_list("ambiguous plan ID", id, "matches multiple plans:", matches)
}

#[derive(Error, Debug)]
pub enum JanusError {
    #[error("ticket '{0}' not found")]
    TicketNotFound(String),

    #[error("{}", format_ambiguous_id(.0, .1))]
    AmbiguousId(String, Vec<String>),

    #[error("plan '{0}' not found")]
    PlanNotFound(String),

    #[error("{}", format_ambiguous_plan_id(.0, .1))]
    AmbiguousPlanId(String, Vec<String>),

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

    #[error("cache access failed at {0}: {1}")]
    CacheAccessFailed(std::path::PathBuf, String),

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

    #[error("invalid event type '{0}'")]
    InvalidEventType(String),

    #[error("invalid actor '{0}'. Must be one of: cli, mcp, hook")]
    InvalidActor(String),

    #[error("hook recipe '{0}' not found")]
    HookRecipeNotFound(String),

    #[error("failed to fetch hook: {0}")]
    HookFetchFailed(String),

    #[error("hook security violation: {0}")]
    HookSecurity(String),

    // Validation errors
    #[error("ticket ID cannot be empty")]
    EmptyTicketId,

    #[error("ticket ID must contain only alphanumeric characters, hyphens, and underscores")]
    InvalidTicketIdCharacters,

    #[error("ticket cannot be its own parent")]
    SelfParentTicket,

    #[error("cannot link a ticket to itself: {0}. Links must be between different tickets.")]
    SelfLink(String),

    // Business logic errors
    #[error("dependency '{0}' not found in ticket")]
    DependencyNotFound(String),

    #[error("circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("link not found between tickets")]
    LinkNotFound,

    #[error("at least {expected} ticket IDs are required, got {provided}")]
    InsufficientTicketIds { expected: usize, provided: usize },

    #[error("unknown array field: {0}")]
    UnknownArrayField(String),

    #[error("invalid ticket type: {0}")]
    InvalidTicketType(String),

    #[error("invalid entity type: {0}. Must be one of: ticket, plan")]
    InvalidEntityType(String),

    #[error("invalid priority: {0}")]
    InvalidPriority(String),

    #[error("plan has no tickets section")]
    PlanNoTicketsSection,

    #[error("plan has no tickets section or phases")]
    PlanNoTicketsOrPhases,

    #[error("reordered list must contain the same tickets")]
    ReorderTicketMismatch,

    #[error("operation requires an interactive terminal")]
    InteractiveTerminalRequired,

    #[error(
        "Note text cannot be empty. Provide text as an argument or pipe from stdin: echo 'note text' | janus add-note <id>"
    )]
    EmptyNote,

    #[error("closing a ticket requires either --summary <TEXT> or --no-summary")]
    SummaryRequired,

    #[error("{0}")]
    Other(String),

    #[error("cache data integrity error: {0}")]
    CacheDataIntegrity(String),

    #[error("failed to extract column {column} from database row: {error}")]
    CacheColumnExtraction { column: usize, error: String },
}

pub type Result<T> = std::result::Result<T, JanusError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_is_corruption_error() {
        use std::io::ErrorKind;

        // Should match CacheDatabase with corruption error variants
        assert!(is_corruption_error(&JanusError::CacheDatabase(
            turso::Error::Corrupt("database corrupted".to_string())
        )));
        assert!(is_corruption_error(&JanusError::CacheDatabase(
            turso::Error::NotAdb("not a database".to_string())
        )));

        // Should match CacheCorrupted variant
        assert!(is_corruption_error(&JanusError::CacheCorrupted(
            "corrupted".to_string()
        )));

        // Should not match unrelated errors
        assert!(!is_corruption_error(&JanusError::CacheDatabase(
            turso::Error::Busy("database is locked".to_string())
        )));
        assert!(!is_corruption_error(&JanusError::CacheDatabase(
            turso::Error::IoError(ErrorKind::NotFound)
        )));
        assert!(!is_corruption_error(&JanusError::TicketNotFound(
            "test".to_string()
        )));
    }

    #[test]
    fn test_is_permission_error() {
        use std::io::ErrorKind;

        // Should match CacheDatabase with permission denied errors
        assert!(is_permission_error(&JanusError::CacheDatabase(
            turso::Error::IoError(ErrorKind::PermissionDenied)
        )));

        // Should match CacheAccessDenied variant
        assert!(is_permission_error(&JanusError::CacheAccessDenied(
            PathBuf::from("/test")
        )));

        // Should match IO error with PermissionDenied
        assert!(is_permission_error(&JanusError::Io(std::io::Error::new(
            ErrorKind::PermissionDenied,
            "denied"
        ))));

        // Should not match other IO errors
        assert!(!is_permission_error(&JanusError::CacheDatabase(
            turso::Error::IoError(ErrorKind::NotFound)
        )));
        assert!(!is_permission_error(&JanusError::Io(std::io::Error::new(
            ErrorKind::NotFound,
            "not found"
        ))));

        // Should not match non-IO errors
        assert!(!is_permission_error(&JanusError::CacheDatabase(
            turso::Error::Corrupt("corrupted".to_string())
        )));
        assert!(!is_permission_error(&JanusError::TicketNotFound(
            "test".to_string()
        )));
    }

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

    #[test]
    fn test_ambiguous_id_error_message() {
        let matches = vec![
            "j-abc1".to_string(),
            "j-abc2".to_string(),
            "j-abc3".to_string(),
        ];
        let error = JanusError::AmbiguousId("j-abc".to_string(), matches);
        let msg = error.to_string();
        assert!(msg.contains("j-abc"));
        assert!(msg.contains("j-abc1"));
        assert!(msg.contains("j-abc2"));
        assert!(msg.contains("j-abc3"));
        assert!(msg.contains("ambiguous ID"));
    }

    #[test]
    fn test_ambiguous_plan_id_error_message() {
        let matches = vec!["plan-alpha".to_string(), "plan-beta".to_string()];
        let error = JanusError::AmbiguousPlanId("plan".to_string(), matches);
        let msg = error.to_string();
        assert!(msg.contains("plan"));
        assert!(msg.contains("plan-alpha"));
        assert!(msg.contains("plan-beta"));
        assert!(msg.contains("ambiguous plan ID"));
    }
}
