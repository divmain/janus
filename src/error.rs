use thiserror::Error;

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

fn format_retry_errors(attempts: &u32, errors: &[String]) -> String {
    let mut msg = format!("operation failed after {attempts} attempts:");
    for (i, error) in errors.iter().enumerate() {
        msg.push_str(&format!("\n  - Attempt {}: {}", i + 1, error));
    }
    msg
}

/// Format GraphQL errors with their details
fn format_graphql_errors(errors: &[GraphQlError]) -> String {
    let mut msg = "GraphQL errors:".to_string();
    for (i, error) in errors.iter().enumerate() {
        msg.push_str(&format!("\n  [{}] ", i + 1));
        if let Some(code) = &error.code {
            msg.push_str(&format!("[{code}] "));
        }
        msg.push_str(&error.message);
        if let Some(path) = &error.path {
            msg.push_str(&format!(" (path: {path})"));
        }
    }
    msg
}

/// Single GraphQL error with structured details
#[derive(Debug, Clone)]
pub struct GraphQlError {
    pub message: String,
    pub code: Option<String>,
    pub path: Option<String>,
}

#[derive(Error, Debug)]
pub enum JanusError {
    #[error("ticket '{0}' not found")]
    TicketNotFound(String),

    #[error("{}", format_ambiguous_id(.0, .1))]
    AmbiguousId(String, Vec<String>),

    #[error("plan '{0}' not found")]
    PlanNotFound(String),

    #[error(
        "invalid ticket ID '{0}': must contain only alphanumeric characters, hyphens, and underscores"
    )]
    InvalidTicketId(String),

    #[error(
        "invalid plan ID '{0}': must start with 'plan-' and contain only alphanumeric characters and hyphens"
    )]
    InvalidPlanId(String),

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

    #[error(
        "empty YAML frontmatter: the frontmatter block between '---' delimiters is empty. Required fields (e.g. id, status) must be provided"
    )]
    EmptyFrontmatter,

    #[error("invalid ticket format: {0}")]
    InvalidFormat(String),

    #[error("invalid status '{0}'")]
    InvalidStatus(String),

    #[error("invalid field name: '{0}'")]
    InvalidFieldName(String),

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

    #[error("Failed to {operation} {item_type} at {path}: {source}")]
    StorageError {
        operation: &'static str,
        item_type: &'static str,
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

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

    #[error("{}", format_graphql_errors(.errors))]
    GraphQlErrors {
        errors: Vec<GraphQlError>,
        partial_data: bool,
    },

    #[error("rate limit exceeded. Please wait {0} seconds before retrying.")]
    RateLimited(u64),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    // Plan import errors
    #[error("{}", format_import_failed(.message, .issues))]
    ImportFailed {
        message: String,
        issues: Vec<String>,
    },

    #[error("{}", format_retry_errors(.attempts, .errors))]
    RetryFailed { attempts: u32, errors: Vec<String> },

    #[error("plan with title '{0}' already exists ({1})")]
    DuplicatePlanTitle(String, String), // title, existing plan ID

    #[error("failed to load {} plan file(s):\n{}", .0.len(), .0.join("\n"))]
    PlanLoadFailed(Vec<String>),

    #[error("failed to load {} ticket file(s):\n{}", .0.len(), .0.join("\n"))]
    TicketLoadFailed(Vec<String>),

    #[error("--verbose-phase can only be used with phased plans")]
    VerbosePhaseRequiresPhasedPlan,

    #[error(
        "--raw cannot be used with other formatting flags (--json, --tickets-only, --phases-only)"
    )]
    RawWithOtherFlags,

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
    #[error("{0} cannot be empty")]
    ValidationEmpty(String), // field name

    #[error(
        "{0} '{1}' contains invalid characters. Use only letters, numbers, hyphens, and underscores"
    )]
    ValidationInvalidCharacters(String, String), // field name, value

    #[error("ticket ID cannot be empty")]
    EmptyTicketId,

    #[error("ticket ID must contain only alphanumeric characters, hyphens, and underscores")]
    InvalidTicketIdCharacters,

    #[error("ticket cannot be its own parent")]
    SelfParentTicket,

    #[error("No tickets exist in the system. Create a ticket first.")]
    EmptyTicketMap,

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

    #[error(
        "invalid size: {0}. Must be one of: xsmall (xs), small (s), medium (m), large (l), xlarge (xl)"
    )]
    InvalidSize(String),

    #[error("invalid sort field: {0}. Must be one of: priority, created, id")]
    InvalidSortField(String),

    #[error("plan has no tickets section")]
    PlanNoTicketsSection,

    #[error("plan has no tickets section or phases")]
    PlanNoTicketsOrPhases,

    #[error("reordered list must contain the same tickets")]
    ReorderTicketMismatch,

    #[error("reordered list must contain the same phases")]
    ReorderPhaseMismatch,

    #[error("editor exited with code {0}")]
    EditorFailed(i32),

    #[error("Cannot open editor in non-interactive mode: {0}")]
    InteractiveTerminalRequired(std::path::PathBuf),

    #[error(
        "Note text cannot be empty. Provide text as an argument or pipe from stdin: echo 'note text' | janus add-note <id>"
    )]
    EmptyNote,

    #[error("ticket title cannot be empty")]
    EmptyTitle,

    #[error("closing a ticket requires either --summary <TEXT> or --no-summary")]
    SummaryRequired,

    #[error("cannot {operation} immutable field '{field}'")]
    ImmutableField { field: String, operation: String },

    #[error("{0}")]
    ConflictingFlags(String),

    #[error("{0}")]
    IdGenerationFailed(String),

    #[error("ticket '{id}' is missing required field '{field}' (file may be corrupted)")]
    CorruptedTicket { id: String, field: String },

    #[error("TUI error: {0}")]
    TuiError(String),

    #[error("MCP server error: {0}")]
    McpServerError(String),

    #[error("filesystem watcher error: {0}")]
    WatcherError(String),

    #[error("{0}")]
    ConfirmationRequired(String),

    #[error("a ticket cannot depend on itself")]
    SelfDependency,

    #[error("EOF on stdin")]
    EofOnStdin,

    #[error("Invalid graph format '{0}'. Must be 'dot' or 'mermaid'")]
    InvalidGraphFormat(String),

    #[error("unsupported sync field: {0}")]
    UnsupportedSyncField(String),

    #[error("could not find ticket or plan with ID '{0}'")]
    ItemNotFound(String),

    #[error("{0}")]
    InvalidInput(String),

    #[error("internal error: {0}")]
    InternalError(String),

    #[error("{0}")]
    Other(String),

    #[error("embedding model error: {0}")]
    EmbeddingModel(String),

    #[error("semantic search not available: embeddings not generated")]
    EmbeddingsNotAvailable,

    #[error("embedding generation timed out after {seconds} seconds")]
    EmbeddingTimeout { seconds: u64 },

    #[error("blocking task failed: {0}")]
    BlockingTaskFailed(String),

    #[error("parse error: {0}")]
    ParseError(String),

    #[error(
        "invalid ticket ID format '{0}': must be non-empty and match '<prefix>-<hash>' pattern"
    )]
    InvalidTicketIdFormat(String),

    #[error("invalid plan ID format '{0}': must be non-empty and match 'plan-<hash>' pattern")]
    InvalidPlanIdFormat(String),

    #[error("invalid timestamp '{0}': must be a valid ISO 8601 / RFC 3339 timestamp")]
    InvalidTimestamp(String),
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
    fn test_retry_failed_error_message() {
        let errors = vec![
            "timeout: connection timed out".to_string(),
            "429: rate limit exceeded".to_string(),
            "401: authentication failed".to_string(),
        ];
        let error = JanusError::RetryFailed {
            attempts: 3,
            errors,
        };
        let msg = error.to_string();
        assert!(msg.contains("failed after 3 attempts"));
        assert!(msg.contains("Attempt 1: timeout"));
        assert!(msg.contains("Attempt 2: 429: rate limit"));
        assert!(msg.contains("Attempt 3: 401: authentication"));
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

    #[test]
    fn test_reorder_phase_mismatch_error_message() {
        let error = JanusError::ReorderPhaseMismatch;
        let msg = error.to_string();
        assert!(msg.contains("phases"));
        assert!(
            !msg.contains("tickets"),
            "phase mismatch error should not mention tickets"
        );
    }

    #[test]
    fn test_invalid_size_error_message() {
        let error = JanusError::InvalidSize("huge".to_string());
        let msg = error.to_string();
        assert!(msg.contains("invalid size"));
        assert!(msg.contains("huge"));
        assert!(msg.contains("xsmall"));
        assert!(msg.contains("xs"));
        assert!(msg.contains("small"));
        assert!(msg.contains("s"));
        assert!(msg.contains("medium"));
        assert!(msg.contains("m"));
        assert!(msg.contains("large"));
        assert!(msg.contains("l"));
        assert!(msg.contains("xlarge"));
        assert!(msg.contains("xl"));
    }
}
