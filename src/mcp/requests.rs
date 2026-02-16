//! MCP tool request types and input validation.
//!
//! This module contains the request parameter structs for all MCP tools,
//! along with validation functions for input sanitization.

use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

use crate::utils::validation::{
    validate_description, validate_note, validate_optional_summary, validate_title_for_mcp,
};

// ============================================================================
// Tool Request Types
// ============================================================================

/// Request parameters for creating a new ticket
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CreateTicketRequest {
    /// Title of the ticket (required)
    #[schemars(description = "The title for the new ticket (max 200 chars, non-empty)")]
    pub title: String,

    /// Ticket type: bug, feature, task, epic, or chore (default: task)
    #[schemars(description = "Type of ticket: bug, feature, task, epic, or chore")]
    #[serde(rename = "type")]
    pub ticket_type: Option<String>,

    /// Priority from 0 (highest) to 4 (lowest), default 2
    #[schemars(description = "Priority level from 0 (highest) to 4 (lowest)")]
    pub priority: Option<u8>,

    /// Description/body content for the ticket
    #[schemars(description = "Optional description text for the ticket body (max 5000 chars)")]
    pub description: Option<String>,

    /// Size estimate: xsmall, small, medium, large, xlarge (or aliases: xs, s, m, l, xl)
    #[schemars(
        description = "Size estimate for the ticket. Valid values: xsmall/xs, small/s, medium/m, large/l, xlarge/xl"
    )]
    pub size: Option<String>,
}

impl CreateTicketRequest {
    /// Validate all fields in the request.
    /// Returns Ok if valid, Err with message if invalid.
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_title_for_mcp(&self.title)?;

        // Validate priority range (0-4)
        if let Some(p) = self.priority {
            if p > 4 {
                return Err(format!(
                    "Priority must be between 0 (highest) and 4 (lowest), got {p}"
                ));
            }
        }

        if let Some(ref desc) = self.description {
            validate_description(desc, "Description")?;
        }
        Ok(())
    }
}

/// Request parameters for spawning a subtask
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SpawnSubtaskRequest {
    /// ID of the parent ticket (can be partial)
    #[schemars(description = "ID of the parent ticket this subtask is spawned from")]
    pub parent_id: String,

    /// Title of the new subtask
    #[schemars(description = "Title for the new subtask (max 200 chars, non-empty)")]
    pub title: String,

    /// Description/body content for the subtask
    #[schemars(description = "Optional description text for the subtask (max 5000 chars)")]
    pub description: Option<String>,

    /// Context explaining why this subtask was created
    #[schemars(
        description = "Context explaining why this subtask was spawned from the parent (max 5000 chars)"
    )]
    pub spawn_context: Option<String>,
}

impl SpawnSubtaskRequest {
    /// Validate all fields in the request.
    /// Returns Ok if valid, Err with message if invalid.
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_title_for_mcp(&self.title)?;
        if let Some(ref desc) = self.description {
            validate_description(desc, "Description")?;
        }
        if let Some(ref context) = self.spawn_context {
            validate_description(context, "Spawn context")?;
        }
        Ok(())
    }
}

/// Request parameters for updating ticket status
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct UpdateStatusRequest {
    /// Ticket ID (can be partial)
    #[schemars(description = "ID of the ticket to update")]
    pub id: String,

    /// New status: new, next, in_progress, complete, or cancelled
    #[schemars(description = "New status: new, next, in_progress, complete, or cancelled")]
    pub status: String,

    /// Optional summary when closing (completing/cancelling) a ticket
    #[schemars(
        description = "Optional completion summary (max 5000 chars, recommended when closing tickets)"
    )]
    pub summary: Option<String>,
}

impl UpdateStatusRequest {
    /// Validate all fields in the request.
    /// Returns Ok if valid, Err with message if invalid.
    pub(crate) fn validate(&self) -> Result<(), String> {
        // Validate status string is a valid TicketStatus
        if self.status.parse::<crate::types::TicketStatus>().is_err() {
            return Err(format!(
                "Invalid status '{}'. Valid values: {}",
                self.status,
                crate::types::TicketStatus::ALL_STRINGS.join(", ")
            ));
        }

        validate_optional_summary(self.summary.as_deref())
    }
}

/// Request parameters for adding a note
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct AddNoteRequest {
    /// Ticket ID (can be partial)
    #[schemars(description = "ID of the ticket to add a note to")]
    pub id: String,

    /// Note content to add
    #[schemars(
        description = "The note text to add (will be timestamped, max 5000 chars, non-empty)"
    )]
    pub note: String,
}

impl AddNoteRequest {
    /// Validate all fields in the request.
    /// Returns Ok if valid, Err with message if invalid.
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_note(&self.note)
    }
}

/// Request parameters for listing tickets
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListTicketsRequest {
    /// Filter by status (e.g., "new", "in_progress")
    #[schemars(
        description = "Filter by ticket status. When not specified, only open tickets are returned (Complete and Cancelled are excluded). Set to 'complete' or 'cancelled' to see closed tickets."
    )]
    pub status: Option<String>,

    /// Filter by type (e.g., "bug", "feature")
    #[schemars(description = "Filter by ticket type")]
    #[serde(rename = "type")]
    pub ticket_type: Option<String>,

    /// Show only ready tickets (no incomplete dependencies)
    #[schemars(description = "If true, show only tickets with all dependencies complete")]
    pub ready: Option<bool>,

    /// Show only blocked tickets (has incomplete dependencies)
    #[schemars(description = "If true, show only tickets blocked by incomplete dependencies")]
    pub blocked: Option<bool>,

    /// Filter by spawned_from parent ID
    #[schemars(description = "Filter to show only tickets spawned from this parent ID")]
    pub spawned_from: Option<String>,

    /// Filter by exact decomposition depth
    #[schemars(description = "Filter by exact decomposition depth (0 = root tickets)")]
    pub depth: Option<u32>,

    /// Filter by size (comma-separated list of sizes: xsmall, small, medium, large, xlarge)
    #[schemars(
        description = "Filter by size. Comma-separated list of: xsmall/xs, small/s, medium/m, large/l, xlarge/xl"
    )]
    pub size: Option<String>,
}

/// Request parameters for showing a ticket
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ShowTicketRequest {
    /// Ticket ID (can be partial)
    #[schemars(description = "ID of the ticket to show")]
    pub id: String,
}

/// Request parameters for adding a dependency
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct AddDependencyRequest {
    /// ID of the ticket that will have the dependency
    #[schemars(description = "ID of the ticket that depends on another")]
    pub ticket_id: String,

    /// ID of the ticket to depend on
    #[schemars(description = "ID of the ticket that must be completed first")]
    pub depends_on_id: String,
}

/// Request parameters for removing a dependency
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RemoveDependencyRequest {
    /// ID of the ticket that has the dependency
    #[schemars(description = "ID of the ticket to remove the dependency from")]
    pub ticket_id: String,

    /// ID of the dependency to remove
    #[schemars(description = "ID of the dependency to remove")]
    pub depends_on_id: String,
}

/// Request parameters for adding a ticket to a plan
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct AddTicketToPlanRequest {
    /// Plan ID (can be partial)
    #[schemars(description = "ID of the plan to add the ticket to")]
    pub plan_id: String,

    /// Ticket ID (can be partial)
    #[schemars(description = "ID of the ticket to add to the plan")]
    pub ticket_id: String,

    /// Phase name/number (required for phased plans)
    #[schemars(description = "Phase name or number (required for phased plans)")]
    pub phase: Option<String>,
}

/// Request parameters for getting plan status
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GetPlanStatusRequest {
    /// Plan ID (can be partial)
    #[schemars(description = "ID of the plan to get status for")]
    pub plan_id: String,
}

/// Request parameters for getting children of a ticket
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GetChildrenRequest {
    /// Ticket ID (can be partial)
    #[schemars(description = "ID of the parent ticket")]
    pub ticket_id: String,
}

/// Request parameters for getting next available ticket(s)
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct GetNextAvailableTicketRequest {
    /// Maximum number of tickets to return (default: 5)
    #[schemars(description = "Maximum number of tickets to return")]
    pub limit: Option<usize>,
}

/// Request parameters for semantic search
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SemanticSearchRequest {
    /// Natural language search query
    #[schemars(description = "Natural language search query")]
    pub query: String,
    /// Maximum results to return (default: 10)
    #[schemars(description = "Maximum number of results to return")]
    pub limit: Option<usize>,
    /// Minimum similarity score 0.0-1.0 (default: 0.0)
    #[schemars(description = "Minimum similarity score (0.0-1.0)")]
    pub threshold: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::validation::{MAX_DESCRIPTION_LENGTH, MAX_TICKET_TITLE_LENGTH};

    // ============================================================================
    // Input Validation Tests
    // ============================================================================

    #[test]
    fn test_validate_title_for_mcp_empty() {
        let result = validate_title_for_mcp("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_validate_title_for_mcp_too_long() {
        let long_title = "a".repeat(MAX_TICKET_TITLE_LENGTH + 1);
        let result = validate_title_for_mcp(&long_title);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too long"));
    }

    #[test]
    fn test_validate_title_for_mcp_max_length() {
        let max_title = "a".repeat(MAX_TICKET_TITLE_LENGTH);
        let result = validate_title_for_mcp(&max_title);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_title_for_mcp_control_chars() {
        let result = validate_title_for_mcp("Title\x00with\x01nulls");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("control characters"));
    }

    #[test]
    fn test_validate_title_for_mcp_newline() {
        let result = validate_title_for_mcp("Title\nwith newline");
        assert!(result.is_err()); // Newlines not allowed in titles
    }

    #[test]
    fn test_validate_title_for_mcp_valid() {
        let result = validate_title_for_mcp("Valid Ticket Title");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_description_too_long() {
        let long_desc = "a".repeat(MAX_DESCRIPTION_LENGTH + 1);
        let result = validate_description(&long_desc, "Description");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too long"));
    }

    #[test]
    fn test_validate_description_max_length() {
        let max_desc = "a".repeat(MAX_DESCRIPTION_LENGTH);
        let result = validate_description(&max_desc, "Description");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_description_control_chars() {
        let result = validate_description("Desc\x00with\x01nulls", "Description");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("control characters"));
    }

    #[test]
    fn test_validate_description_newlines_allowed() {
        let result = validate_description("Line 1\nLine 2\r\nLine 3", "Description");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_note_empty() {
        let result = validate_note("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_validate_note_valid() {
        let result = validate_note("This is a valid note.");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_optional_summary_none() {
        let result = validate_optional_summary(None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_optional_summary_valid() {
        let result = validate_optional_summary(Some("Valid summary"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_optional_summary_too_long() {
        let long_summary = "a".repeat(MAX_DESCRIPTION_LENGTH + 1);
        let result = validate_optional_summary(Some(&long_summary));
        assert!(result.is_err());
    }

    // ============================================================================
    // Request Type Validation Tests
    // ============================================================================

    #[test]
    fn test_create_ticket_request_schema() {
        let schema = schemars::schema_for!(CreateTicketRequest);
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("title"));
        assert!(json.contains("description"));
    }

    #[test]
    fn test_spawn_subtask_request_schema() {
        let schema = schemars::schema_for!(SpawnSubtaskRequest);
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("parent_id"));
        assert!(json.contains("spawn_context"));
    }

    #[test]
    fn test_list_tickets_request_default() {
        let request = ListTicketsRequest::default();
        assert!(request.status.is_none());
        assert!(request.ready.is_none());
        assert!(request.blocked.is_none());
    }

    #[test]
    fn test_create_ticket_request_schema_includes_size() {
        let schema = schemars::schema_for!(CreateTicketRequest);
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("size"));
        assert!(json.contains("xsmall") || json.contains("xs"));
    }

    #[test]
    fn test_list_tickets_request_schema_includes_size() {
        let schema = schemars::schema_for!(ListTicketsRequest);
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("size"));
    }

    #[test]
    fn test_create_ticket_request_valid() {
        let request = CreateTicketRequest {
            title: "Valid Title".to_string(),
            ticket_type: None,
            priority: None,
            description: None,
            size: None,
        };
        assert!(request.validate().is_ok());
    }

    #[test]
    fn test_create_ticket_request_empty_title() {
        let request = CreateTicketRequest {
            title: "".to_string(),
            ticket_type: None,
            priority: None,
            description: None,
            size: None,
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn test_create_ticket_request_long_description() {
        let request = CreateTicketRequest {
            title: "Valid Title".to_string(),
            ticket_type: None,
            priority: None,
            description: Some("a".repeat(5001)),
            size: None,
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn test_spawn_subtask_request_valid() {
        let request = SpawnSubtaskRequest {
            parent_id: "j-a1b2".to_string(),
            title: "Valid Subtask".to_string(),
            description: Some("Valid description".to_string()),
            spawn_context: Some("Spawned for testing".to_string()),
        };
        assert!(request.validate().is_ok());
    }

    #[test]
    fn test_update_status_request_valid() {
        let request = UpdateStatusRequest {
            id: "j-a1b2".to_string(),
            status: "complete".to_string(),
            summary: Some("Completed successfully".to_string()),
        };
        assert!(request.validate().is_ok());
    }

    #[test]
    fn test_add_note_request_valid() {
        let request = AddNoteRequest {
            id: "j-a1b2".to_string(),
            note: "This is a valid note.".to_string(),
        };
        assert!(request.validate().is_ok());
    }

    #[test]
    fn test_add_note_request_empty() {
        let request = AddNoteRequest {
            id: "j-a1b2".to_string(),
            note: "".to_string(),
        };
        assert!(request.validate().is_err());
    }
}
