use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;

use crate::error::JanusError;
use crate::hooks::HookContext;

// Legacy constants - kept for backward compatibility
pub const TICKETS_DIR: &str = ".janus";
pub const TICKETS_ITEMS_DIR: &str = ".janus/items";
pub const PLANS_DIR: &str = ".janus/plans";

/// Returns the root Janus directory path.
///
/// Resolution order:
/// 1. `JANUS_ROOT` environment variable (if set)
/// 2. Current working directory + `.janus`
pub fn janus_root() -> PathBuf {
    if let Ok(root) = std::env::var("JANUS_ROOT") {
        PathBuf::from(root)
    } else {
        PathBuf::from(".janus")
    }
}

/// Returns the path to the tickets items directory.
pub fn tickets_items_dir() -> PathBuf {
    janus_root().join("items")
}

/// Returns the path to the plans directory.
pub fn plans_dir() -> PathBuf {
    janus_root().join("plans")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    #[default]
    New,
    Next,
    InProgress,
    Complete,
    Cancelled,
}

impl TicketStatus {
    /// Returns true if this status represents a terminal state (complete or cancelled).
    /// Terminal states indicate no further work is expected on the ticket.
    ///
    /// This method delegates to `crate::status::is_terminal()` for centralized status logic.
    pub fn is_terminal(self) -> bool {
        crate::status::is_terminal(self)
    }

    /// Returns true if this status indicates work has not yet started (new or next).
    /// These are pre-work states where the ticket is queued but not actively being worked on.
    ///
    /// This method delegates to `crate::status::is_not_started()` for centralized status logic.
    pub fn is_not_started(self) -> bool {
        crate::status::is_not_started(self)
    }
}

enum_display!(
    TicketStatus,
    {
        New => "new",
        Next => "next",
        InProgress => "in_progress",
        Complete => "complete",
        Cancelled => "cancelled",
    }
);

impl FromStr for TicketStatus {
    type Err = JanusError;

    /// Parses a ticket status from a string.
    ///
    /// Parsing is case-insensitive: "new", "NEW", and "New" all parse to TicketStatus::New.
    /// Valid values: "new", "next", "in_progress", "complete", "cancelled"
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "new" => Ok(TicketStatus::New),
            "next" => Ok(TicketStatus::Next),
            "in_progress" => Ok(TicketStatus::InProgress),
            "complete" => Ok(TicketStatus::Complete),
            "cancelled" => Ok(TicketStatus::Cancelled),
            _ => Err(JanusError::InvalidStatus(s.to_string())),
        }
    }
}

pub const VALID_STATUSES: &[&str] = &["new", "next", "in_progress", "complete", "cancelled"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TicketType {
    Bug,
    Feature,
    #[default]
    Task,
    Epic,
    Chore,
}

enum_display_fromstr!(
    TicketType,
    JanusError::InvalidTicketType,
    {
        Bug => "bug",
        Feature => "feature",
        Task => "task",
        Epic => "epic",
        Chore => "chore",
    }
);

pub const VALID_TYPES: &[&str] = &["bug", "feature", "task", "epic", "chore"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityType {
    Ticket,
    Plan,
}

enum_display_fromstr!(
    EntityType,
    JanusError::InvalidEntityType,
    {
        Ticket => "ticket",
        Plan => "plan",
    }
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TicketPriority {
    #[serde(rename = "0")]
    P0,
    #[serde(rename = "1")]
    P1,
    #[default]
    #[serde(rename = "2")]
    P2,
    #[serde(rename = "3")]
    P3,
    #[serde(rename = "4")]
    P4,
}

impl TicketPriority {
    pub fn as_num(&self) -> u8 {
        match self {
            TicketPriority::P0 => 0,
            TicketPriority::P1 => 1,
            TicketPriority::P2 => 2,
            TicketPriority::P3 => 3,
            TicketPriority::P4 => 4,
        }
    }
}

enum_display_fromstr!(
    TicketPriority,
    JanusError::InvalidPriority,
    {
        P0 => "0",
        P1 => "1",
        P2 => "2",
        P3 => "3",
        P4 => "4",
    }
);

pub const VALID_PRIORITIES: &[&str] = &["0", "1", "2", "3", "4"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketField {
    Id,
    Uuid,
    Status,
    Deps,
    Links,
    Created,
    Type,
    Priority,
    ExternalRef,
    Remote,
    Parent,
    SpawnedFrom,
    SpawnContext,
    Depth,
    Triaged,
}

impl TicketField {
    pub fn is_immutable(&self) -> bool {
        matches!(self, TicketField::Id | TicketField::Uuid)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TicketField::Id => "id",
            TicketField::Uuid => "uuid",
            TicketField::Status => "status",
            TicketField::Deps => "deps",
            TicketField::Links => "links",
            TicketField::Created => "created",
            TicketField::Type => "type",
            TicketField::Priority => "priority",
            TicketField::ExternalRef => "external-ref",
            TicketField::Remote => "remote",
            TicketField::Parent => "parent",
            TicketField::SpawnedFrom => "spawned-from",
            TicketField::SpawnContext => "spawn-context",
            TicketField::Depth => "depth",
            TicketField::Triaged => "triaged",
        }
    }

    pub fn all() -> &'static [Self] {
        use TicketField::*;
        &[
            Id,
            Uuid,
            Status,
            Deps,
            Links,
            Created,
            Type,
            Priority,
            ExternalRef,
            Remote,
            Parent,
            SpawnedFrom,
            SpawnContext,
            Depth,
            Triaged,
        ]
    }
}

impl std::fmt::Display for TicketField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for TicketField {
    type Err = crate::error::JanusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "id" => Ok(TicketField::Id),
            "uuid" => Ok(TicketField::Uuid),
            "status" => Ok(TicketField::Status),
            "deps" => Ok(TicketField::Deps),
            "links" => Ok(TicketField::Links),
            "created" => Ok(TicketField::Created),
            "type" => Ok(TicketField::Type),
            "priority" => Ok(TicketField::Priority),
            "external-ref" => Ok(TicketField::ExternalRef),
            "remote" => Ok(TicketField::Remote),
            "parent" => Ok(TicketField::Parent),
            "spawned-from" => Ok(TicketField::SpawnedFrom),
            "spawn-context" => Ok(TicketField::SpawnContext),
            "depth" => Ok(TicketField::Depth),
            "triaged" => Ok(TicketField::Triaged),
            _ => Err(JanusError::InvalidField {
                field: s.to_string(),
                valid_fields: TicketField::all()
                    .iter()
                    .map(|f| f.as_str().to_string())
                    .collect(),
            }),
        }
    }
}

pub const VALID_TICKET_FIELDS: &[&str] = &[
    "id",
    "uuid",
    "status",
    "deps",
    "links",
    "created",
    "type",
    "priority",
    "external-ref",
    "remote",
    "parent",
    "spawned-from",
    "spawn-context",
    "depth",
    "triaged",
];

pub const IMMUTABLE_TICKET_FIELDS: &[&str] = &["id", "uuid"];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TicketMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,

    #[serde(skip)]
    pub title: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TicketStatus>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deps: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,

    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub ticket_type: Option<TicketType>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<TicketPriority>,

    #[serde(rename = "external-ref", skip_serializing_if = "Option::is_none")]
    pub external_ref: Option<String>,

    /// Remote issue reference (e.g., "github:owner/repo/123" or "linear:org/PROJ-123")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,

    /// ID of the parent ticket that spawned this one (decomposition provenance)
    #[serde(rename = "spawned-from", skip_serializing_if = "Option::is_none")]
    pub spawned_from: Option<String>,

    /// Brief context explaining why this ticket was created from the parent
    #[serde(rename = "spawn-context", skip_serializing_if = "Option::is_none")]
    pub spawn_context: Option<String>,

    /// Auto-computed decomposition depth (0 = root ticket, parent.depth + 1 otherwise)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,

    /// Whether the ticket has been triaged (reviewed and assessed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triaged: Option<bool>,

    #[serde(skip)]
    pub file_path: Option<PathBuf>,

    /// Completion summary extracted from `## Completion Summary` section in body
    /// Only meaningful for tickets with status: complete
    #[serde(skip)]
    pub completion_summary: Option<String>,

    /// Ticket body content (only populated during cache sync, not persisted to YAML)
    #[serde(skip)]
    pub body: Option<String>,
}

impl TicketMetadata {
    /// Get priority as a number for sorting (defaults to 2)
    pub fn priority_num(&self) -> u8 {
        self.priority.map(|p| p.as_num()).unwrap_or(2)
    }

    /// Compute the effective depth of this ticket.
    ///
    /// Returns the explicit depth if set, otherwise infers from spawned_from:
    /// - No spawned_from -> depth 0 (root)
    /// - Has spawned_from -> depth 1 (child, unless parent depth is known)
    pub fn compute_depth(&self) -> u32 {
        self.depth.unwrap_or_else(|| {
            // If no explicit depth, infer: if no spawned_from, it's depth 0
            if self.spawned_from.is_none() { 0 } else { 1 }
        })
    }

    /// Get the item ID
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// Get the item UUID
    pub fn uuid(&self) -> Option<&str> {
        self.uuid.as_deref()
    }

    /// Get the item title
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Get the file path
    pub fn file_path(&self) -> Option<&PathBuf> {
        self.file_path.as_ref()
    }

    /// Get the item type
    pub fn item_type(&self) -> EntityType {
        EntityType::Ticket
    }

    /// Build a hook context from this metadata
    pub fn hook_context(&self) -> HookContext {
        let mut ctx = HookContext::new().with_item_type(self.item_type());

        if let Some(id) = self.id() {
            ctx = ctx.with_item_id(id);
        }

        if let Some(fp) = self.file_path() {
            ctx = ctx.with_file_path(fp);
        }

        ctx
    }
}

/// Helper struct for tickets with computed blockers
#[derive(Debug, Clone)]
pub struct TicketWithBlockers {
    pub metadata: TicketMetadata,
    pub open_blockers: Vec<String>,
}

pub fn validate_field_name(field: &str, operation: &str) -> crate::error::Result<()> {
    let parsed = field.parse::<TicketField>()?;

    if parsed.is_immutable() {
        return Err(JanusError::Other(format!(
            "cannot {} immutable field '{}'",
            operation, parsed
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_janus_root_default() {
        // Clear JANUS_ROOT to test default behavior
        // SAFETY: We use #[serial] to ensure single-threaded access
        unsafe { std::env::remove_var("JANUS_ROOT") };
        let root = janus_root();
        assert_eq!(root, PathBuf::from(".janus"));
    }

    #[test]
    #[serial]
    fn test_janus_root_with_env_var() {
        // SAFETY: We use #[serial] to ensure single-threaded access
        unsafe { std::env::set_var("JANUS_ROOT", "/custom/path/.janus") };
        let root = janus_root();
        assert_eq!(root, PathBuf::from("/custom/path/.janus"));
        unsafe { std::env::remove_var("JANUS_ROOT") };
    }

    #[test]
    #[serial]
    fn test_tickets_items_dir_default() {
        // SAFETY: We use #[serial] to ensure single-threaded access
        unsafe { std::env::remove_var("JANUS_ROOT") };
        let dir = tickets_items_dir();
        assert_eq!(dir, PathBuf::from(".janus/items"));
    }

    #[test]
    #[serial]
    fn test_tickets_items_dir_with_env_var() {
        // SAFETY: We use #[serial] to ensure single-threaded access
        unsafe { std::env::set_var("JANUS_ROOT", "/custom/path/.janus") };
        let dir = tickets_items_dir();
        assert_eq!(dir, PathBuf::from("/custom/path/.janus/items"));
        unsafe { std::env::remove_var("JANUS_ROOT") };
    }

    #[test]
    #[serial]
    fn test_plans_dir_default() {
        // SAFETY: We use #[serial] to ensure single-threaded access
        unsafe { std::env::remove_var("JANUS_ROOT") };
        let dir = plans_dir();
        assert_eq!(dir, PathBuf::from(".janus/plans"));
    }

    #[test]
    #[serial]
    fn test_plans_dir_with_env_var() {
        // SAFETY: We use #[serial] to ensure single-threaded access
        unsafe { std::env::set_var("JANUS_ROOT", "/custom/path/.janus") };
        let dir = plans_dir();
        assert_eq!(dir, PathBuf::from("/custom/path/.janus/plans"));
        unsafe { std::env::remove_var("JANUS_ROOT") };
    }

    #[test]
    fn test_ticket_status_is_terminal() {
        assert!(TicketStatus::Complete.is_terminal());
        assert!(TicketStatus::Cancelled.is_terminal());
        assert!(!TicketStatus::New.is_terminal());
        assert!(!TicketStatus::Next.is_terminal());
        assert!(!TicketStatus::InProgress.is_terminal());
    }

    #[test]
    fn test_ticket_status_is_not_started() {
        assert!(TicketStatus::New.is_not_started());
        assert!(TicketStatus::Next.is_not_started());
        assert!(!TicketStatus::InProgress.is_not_started());
        assert!(!TicketStatus::Complete.is_not_started());
        assert!(!TicketStatus::Cancelled.is_not_started());
    }

    #[test]
    fn test_ticket_status_from_str_case_insensitive() {
        // Lowercase
        assert_eq!("new".parse::<TicketStatus>().unwrap(), TicketStatus::New);
        assert_eq!(
            "in_progress".parse::<TicketStatus>().unwrap(),
            TicketStatus::InProgress
        );

        // Uppercase
        assert_eq!("NEW".parse::<TicketStatus>().unwrap(), TicketStatus::New);
        assert_eq!("NEXT".parse::<TicketStatus>().unwrap(), TicketStatus::Next);
        assert_eq!(
            "IN_PROGRESS".parse::<TicketStatus>().unwrap(),
            TicketStatus::InProgress
        );
        assert_eq!(
            "COMPLETE".parse::<TicketStatus>().unwrap(),
            TicketStatus::Complete
        );
        assert_eq!(
            "CANCELLED".parse::<TicketStatus>().unwrap(),
            TicketStatus::Cancelled
        );

        // Mixed case
        assert_eq!("New".parse::<TicketStatus>().unwrap(), TicketStatus::New);
        assert_eq!("Next".parse::<TicketStatus>().unwrap(), TicketStatus::Next);
        assert_eq!(
            "In_Progress".parse::<TicketStatus>().unwrap(),
            TicketStatus::InProgress
        );

        // Invalid
        assert!("invalid".parse::<TicketStatus>().is_err());
        assert!("".parse::<TicketStatus>().is_err());
    }

    #[test]
    fn test_ticket_type_from_str_case_insensitive() {
        // Lowercase
        assert_eq!("bug".parse::<TicketType>().unwrap(), TicketType::Bug);
        assert_eq!(
            "feature".parse::<TicketType>().unwrap(),
            TicketType::Feature
        );
        assert_eq!("task".parse::<TicketType>().unwrap(), TicketType::Task);
        assert_eq!("epic".parse::<TicketType>().unwrap(), TicketType::Epic);
        assert_eq!("chore".parse::<TicketType>().unwrap(), TicketType::Chore);

        // Uppercase
        assert_eq!("BUG".parse::<TicketType>().unwrap(), TicketType::Bug);
        assert_eq!(
            "FEATURE".parse::<TicketType>().unwrap(),
            TicketType::Feature
        );
        assert_eq!("TASK".parse::<TicketType>().unwrap(), TicketType::Task);
        assert_eq!("EPIC".parse::<TicketType>().unwrap(), TicketType::Epic);
        assert_eq!("CHORE".parse::<TicketType>().unwrap(), TicketType::Chore);

        // Mixed case
        assert_eq!("Bug".parse::<TicketType>().unwrap(), TicketType::Bug);
        assert_eq!(
            "Feature".parse::<TicketType>().unwrap(),
            TicketType::Feature
        );
        assert_eq!("Task".parse::<TicketType>().unwrap(), TicketType::Task);

        // Invalid
        assert!("invalid".parse::<TicketType>().is_err());
        assert!("".parse::<TicketType>().is_err());
    }

    #[test]
    fn test_ticket_priority_from_str() {
        // Valid numeric strings
        assert_eq!("0".parse::<TicketPriority>().unwrap(), TicketPriority::P0);
        assert_eq!("1".parse::<TicketPriority>().unwrap(), TicketPriority::P1);
        assert_eq!("2".parse::<TicketPriority>().unwrap(), TicketPriority::P2);
        assert_eq!("3".parse::<TicketPriority>().unwrap(), TicketPriority::P3);
        assert_eq!("4".parse::<TicketPriority>().unwrap(), TicketPriority::P4);

        // Invalid
        assert!("5".parse::<TicketPriority>().is_err());
        assert!("-1".parse::<TicketPriority>().is_err());
        assert!("p0".parse::<TicketPriority>().is_err());
        assert!("P0".parse::<TicketPriority>().is_err());
        assert!("".parse::<TicketPriority>().is_err());
    }

    #[test]
    fn test_spawning_metadata_fields_in_valid_fields() {
        // Verify spawning metadata fields are in VALID_TICKET_FIELDS
        assert!(VALID_TICKET_FIELDS.contains(&"spawned-from"));
        assert!(VALID_TICKET_FIELDS.contains(&"spawn-context"));
        assert!(VALID_TICKET_FIELDS.contains(&"depth"));
    }

    #[test]
    fn test_ticket_metadata_default_spawning_fields_none() {
        // Verify spawning fields default to None
        let metadata = TicketMetadata::default();
        assert!(metadata.spawned_from.is_none());
        assert!(metadata.spawn_context.is_none());
        assert!(metadata.depth.is_none());
    }

    #[test]
    fn test_ticket_metadata_spawning_fields_serialization() {
        use serde_yaml_ng as yaml;

        // Test that spawning fields serialize correctly when present
        let metadata = TicketMetadata {
            id: Some("j-test".to_string()),
            spawned_from: Some("j-parent".to_string()),
            spawn_context: Some("Test context".to_string()),
            depth: Some(2),
            ..Default::default()
        };

        let yaml_str = yaml::to_string(&metadata).unwrap();
        assert!(yaml_str.contains("spawned-from: j-parent"));
        assert!(yaml_str.contains("spawn-context: Test context"));
        assert!(yaml_str.contains("depth: 2"));
    }

    #[test]
    fn test_ticket_metadata_spawning_fields_skip_serialization_when_none() {
        use serde_yaml_ng as yaml;

        // Test that spawning fields are skipped when None
        let metadata = TicketMetadata {
            id: Some("j-test".to_string()),
            ..Default::default()
        };

        let yaml_str = yaml::to_string(&metadata).unwrap();
        assert!(!yaml_str.contains("spawned-from"));
        assert!(!yaml_str.contains("spawn-context"));
        assert!(!yaml_str.contains("depth"));
    }

    #[test]
    fn test_ticket_metadata_spawning_fields_deserialization() {
        use serde_yaml_ng as yaml;

        // Test that spawning fields deserialize correctly
        let yaml_str = r#"
id: j-test
spawned-from: j-parent
spawn-context: Auth implementation requires OAuth setup first
depth: 2
"#;
        let metadata: TicketMetadata = yaml::from_str(yaml_str).unwrap();
        assert_eq!(metadata.id, Some("j-test".to_string()));
        assert_eq!(metadata.spawned_from, Some("j-parent".to_string()));
        assert_eq!(
            metadata.spawn_context,
            Some("Auth implementation requires OAuth setup first".to_string())
        );
        assert_eq!(metadata.depth, Some(2));
    }

    #[test]
    fn test_ticket_metadata_spawning_fields_deserialization_missing() {
        use serde_yaml_ng as yaml;

        // Test that missing spawning fields deserialize as None
        let yaml_str = r#"
id: j-test
status: new
"#;
        let metadata: TicketMetadata = yaml::from_str(yaml_str).unwrap();
        assert_eq!(metadata.id, Some("j-test".to_string()));
        assert!(metadata.spawned_from.is_none());
        assert!(metadata.spawn_context.is_none());
        assert!(metadata.depth.is_none());
    }

    #[test]
    fn test_validate_field_name_valid() {
        assert!(validate_field_name("status", "update").is_ok());
        assert!(validate_field_name("priority", "update").is_ok());
        assert!(validate_field_name("type", "update").is_ok());
    }

    #[test]
    fn test_validate_field_name_invalid() {
        let result = validate_field_name("unknown_field", "update");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::InvalidField {
                field,
                valid_fields: _,
            } => {
                assert_eq!(field, "unknown_field");
            }
            _ => panic!("Expected InvalidField error"),
        }
    }

    #[test]
    fn test_validate_field_name_immutable_id() {
        let result = validate_field_name("id", "update");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::Other(msg) => {
                assert!(msg.contains("cannot update immutable field 'id'"));
            }
            _ => panic!("Expected Other error for immutable field"),
        }
    }

    #[test]
    fn test_validate_field_name_immutable_uuid() {
        let result = validate_field_name("uuid", "update");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::Other(msg) => {
                assert!(msg.contains("cannot update immutable field 'uuid'"));
            }
            _ => panic!("Expected Other error for immutable field"),
        }
    }

    #[test]
    fn test_validate_field_name_remove_immutable() {
        let result = validate_field_name("id", "remove");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::Other(msg) => {
                assert!(msg.contains("cannot remove immutable field 'id'"));
            }
            _ => panic!("Expected Other error for immutable field"),
        }
    }

    #[test]
    fn test_ticket_field_from_str_valid() {
        assert_eq!(TicketField::from_str("id").unwrap(), TicketField::Id);
        assert_eq!(TicketField::from_str("uuid").unwrap(), TicketField::Uuid);
        assert_eq!(
            TicketField::from_str("status").unwrap(),
            TicketField::Status
        );
        assert_eq!(TicketField::from_str("deps").unwrap(), TicketField::Deps);
        assert_eq!(TicketField::from_str("type").unwrap(), TicketField::Type);
        assert_eq!(
            TicketField::from_str("spawned-from").unwrap(),
            TicketField::SpawnedFrom
        );
        assert_eq!(
            TicketField::from_str("spawn-context").unwrap(),
            TicketField::SpawnContext
        );
    }

    #[test]
    fn test_ticket_field_from_str_invalid() {
        assert!(TicketField::from_str("invalid").is_err());
        assert!(TicketField::from_str("").is_err());
        assert!(TicketField::from_str("ID").is_err());
    }

    #[test]
    fn test_ticket_field_is_immutable() {
        assert!(TicketField::Id.is_immutable());
        assert!(TicketField::Uuid.is_immutable());
        assert!(!TicketField::Status.is_immutable());
        assert!(!TicketField::Priority.is_immutable());
        assert!(!TicketField::SpawnedFrom.is_immutable());
    }

    #[test]
    fn test_ticket_field_as_str() {
        assert_eq!(TicketField::Id.as_str(), "id");
        assert_eq!(TicketField::Uuid.as_str(), "uuid");
        assert_eq!(TicketField::Status.as_str(), "status");
        assert_eq!(TicketField::SpawnedFrom.as_str(), "spawned-from");
        assert_eq!(TicketField::SpawnContext.as_str(), "spawn-context");
    }

    #[test]
    fn test_ticket_field_display() {
        assert_eq!(format!("{}", TicketField::Id), "id");
        assert_eq!(format!("{}", TicketField::SpawnedFrom), "spawned-from");
    }

    #[test]
    fn test_validate_field_name_uses_strict_enum() {
        let valid_fields = TicketField::all();
        assert!(!valid_fields.is_empty());

        for field in valid_fields {
            if !field.is_immutable() {
                assert!(
                    validate_field_name(field.as_str(), "update").is_ok(),
                    "Valid mutable field '{}' should be accepted",
                    field
                );
            } else {
                let result = validate_field_name(field.as_str(), "update");
                assert!(
                    result.is_err(),
                    "Immutable field '{}' should be rejected for update",
                    field
                );
            }
        }
    }
}
