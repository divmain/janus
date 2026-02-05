//! MCP tool implementations for Janus.
//!
//! This module contains the tool implementations that are exposed
//! through the MCP server. Tools allow AI agents to interact with
//! Janus tickets and plans.
//!
//! ## Available Tools
//!
//! | Tool | Description |
//! |------|-------------|
//! | `create_ticket` | Create a new ticket |
//! | `spawn_subtask` | Create a ticket as a child of another |
//! | `update_status` | Change a ticket's status |
//! | `add_note` | Add a timestamped note to a ticket |
//! | `list_tickets` | Query tickets with filters |
//! | `show_ticket` | Get full ticket content |
//! | `add_dependency` | Add a dependency between tickets |
//! | `remove_dependency` | Remove a dependency between tickets |
//! | `add_ticket_to_plan` | Add a ticket to a plan |
//! | `get_plan_status` | Get plan progress information |
//! | `get_children` | Get tickets spawned from a parent |
//! | `get_next_available_ticket` | Query the backlog for the next ticket(s) to work on |
//! | `semantic_search` | Find tickets semantically similar to a query (semantic-search feature) |

use rmcp::{
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    schemars::{self, JsonSchema},
    tool,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::str::FromStr;

use crate::cache::get_ticket_cache;
use crate::events::{Actor, EntityType, Event, EventType, log_event};
use crate::next::{InclusionReason, NextWorkFinder, WorkItem};
use crate::plan::parser::serialize_plan;
use crate::plan::types::{PlanMetadata, PlanStatus};
use crate::plan::{Plan, compute_all_phase_statuses, compute_plan_status};
use crate::remote::config::Config;
use crate::ticket::{Ticket, TicketBuilder, build_ticket_map, get_all_tickets_with_map};
use crate::types::{TicketMetadata, TicketSize, TicketStatus, TicketType};
use crate::utils::iso_date;

use super::format::{format_children_table_row, format_plan_ticket_entry, format_related_tickets_section, format_ticket_id, format_ticket_table_row, format_ticket_title, format_spawn_context_line};

// ============================================================================
// Tool Request Types
// ============================================================================

/// Request parameters for creating a new ticket
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CreateTicketRequest {
    /// Title of the ticket (required)
    #[schemars(description = "The title for the new ticket")]
    pub title: String,

    /// Ticket type: bug, feature, task, epic, or chore (default: task)
    #[schemars(description = "Type of ticket: bug, feature, task, epic, or chore")]
    #[serde(rename = "type")]
    pub ticket_type: Option<String>,

    /// Priority from 0 (highest) to 4 (lowest), default 2
    #[schemars(description = "Priority level from 0 (highest) to 4 (lowest)")]
    pub priority: Option<u8>,

    /// Description/body content for the ticket
    #[schemars(description = "Optional description text for the ticket body")]
    pub description: Option<String>,

    /// Size estimate: xsmall, small, medium, large, xlarge (or aliases: xs, s, m, l, xl)
    #[schemars(
        description = "Size estimate for the ticket. Valid values: xsmall/xs, small/s, medium/m, large/l, xlarge/xl"
    )]
    pub size: Option<String>,
}

/// Request parameters for spawning a subtask
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SpawnSubtaskRequest {
    /// ID of the parent ticket (can be partial)
    #[schemars(description = "ID of the parent ticket this subtask is spawned from")]
    pub parent_id: String,

    /// Title of the new subtask
    #[schemars(description = "Title for the new subtask")]
    pub title: String,

    /// Description/body content for the subtask
    #[schemars(description = "Optional description text for the subtask")]
    pub description: Option<String>,

    /// Context explaining why this subtask was created
    #[schemars(description = "Context explaining why this subtask was spawned from the parent")]
    pub spawn_context: Option<String>,
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
    #[schemars(description = "Optional completion summary (recommended when closing tickets)")]
    pub summary: Option<String>,
}

/// Request parameters for adding a note
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct AddNoteRequest {
    /// Ticket ID (can be partial)
    #[schemars(description = "ID of the ticket to add a note to")]
    pub id: String,

    /// Note content to add
    #[schemars(description = "The note text to add (will be timestamped)")]
    pub note: String,
}

/// Request parameters for listing tickets
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListTicketsRequest {
    /// Filter by status (e.g., "new", "in_progress")
    #[schemars(description = "Filter by ticket status")]
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

// ============================================================================
// Tool Router Implementation
// ============================================================================

/// The Janus MCP tool handler
#[derive(Clone, Debug)]
pub struct JanusTools {
    tool_router: ToolRouter<Self>,
}

impl Default for JanusTools {
    fn default() -> Self {
        Self::new()
    }
}

impl JanusTools {
    /// Create a new JanusTools instance with all tools registered
    pub fn new() -> Self {
        use rmcp::{
            handler::server::tool::ToolRoute,
            model::Tool,
        };
        use schemars::schema_for;
        use std::sync::Arc;

        let mut router = ToolRouter::new();

        // Helper to create Tool metadata from request type schema
        fn create_tool_meta<S: Serialize>(
            name: &str,
            description: &str,
            schema: S,
        ) -> Tool {
            let schema_value = serde_json::to_value(schema).unwrap();
            let schema_obj = match schema_value {
                serde_json::Value::Object(obj) => obj,
                _ => panic!("Schema must be an object"),
            };
            Tool::new(
                name.to_string(),
                description.to_string(),
                Arc::new(schema_obj),
            )
        }

        // create_ticket
        let tool = create_tool_meta(
            "create_ticket",
            "Create a new ticket. Returns the ticket ID and file path.",
            schema_for!(CreateTicketRequest),
        );
        let route = ToolRoute::new_dyn(
            tool,
            |ctx: rmcp::handler::server::tool::ToolCallContext<'_, Self>| {
                Box::pin(async move {
                    let this = ctx.service;
                    let args = ctx.arguments.ok_or(rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Borrowed("Missing arguments"),
                        data: None,
                    })?;
                    let request: CreateTicketRequest = serde_json::from_value(
                        serde_json::Value::Object(args),
                    )
                    .map_err(|e| rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Owned(format!("Invalid parameters: {e}")),
                        data: None,
                    })?;
                    match this.create_ticket_impl(Parameters(request)).await {
                        Ok(result) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(result)],
                            structured_content: None,
                            is_error: None,
                            meta: None,
                        }),
                        Err(e) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(e)],
                            structured_content: None,
                            is_error: Some(true),
                            meta: None,
                        }),
                    }
                })
            },
        );
        router.add_route(route);

        // spawn_subtask
        let tool = create_tool_meta(
            "spawn_subtask",
            "Create a new ticket as a child of an existing ticket. Sets spawning metadata for decomposition tracking.",
            schema_for!(SpawnSubtaskRequest),
        );
        let route = ToolRoute::new_dyn(
            tool,
            |ctx: rmcp::handler::server::tool::ToolCallContext<'_, Self>| {
                Box::pin(async move {
                    let this = ctx.service;
                    let args = ctx.arguments.ok_or(rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Borrowed("Missing arguments"),
                        data: None,
                    })?;
                    let request: SpawnSubtaskRequest = serde_json::from_value(
                        serde_json::Value::Object(args),
                    )
                    .map_err(|e| rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Owned(format!("Invalid parameters: {e}")),
                        data: None,
                    })?;
                    match this.spawn_subtask_impl(Parameters(request)).await {
                        Ok(result) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(result)],
                            structured_content: None,
                            is_error: None,
                            meta: None,
                        }),
                        Err(e) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(e)],
                            structured_content: None,
                            is_error: Some(true),
                            meta: None,
                        }),
                    }
                })
            },
        );
        router.add_route(route);

        // update_status
        let tool = create_tool_meta(
            "update_status",
            "Change a ticket's status. Valid statuses: new, next, in_progress, complete, cancelled.",
            schema_for!(UpdateStatusRequest),
        );
        let route = ToolRoute::new_dyn(
            tool,
            |ctx: rmcp::handler::server::tool::ToolCallContext<'_, Self>| {
                Box::pin(async move {
                    let this = ctx.service;
                    let args = ctx.arguments.ok_or(rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Borrowed("Missing arguments"),
                        data: None,
                    })?;
                    let request: UpdateStatusRequest = serde_json::from_value(
                        serde_json::Value::Object(args),
                    )
                    .map_err(|e| rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Owned(format!("Invalid parameters: {e}")),
                        data: None,
                    })?;
                    match this.update_status_impl(Parameters(request)).await {
                        Ok(result) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(result)],
                            structured_content: None,
                            is_error: None,
                            meta: None,
                        }),
                        Err(e) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(e)],
                            structured_content: None,
                            is_error: Some(true),
                            meta: None,
                        }),
                    }
                })
            },
        );
        router.add_route(route);

        // add_note
        let tool = create_tool_meta(
            "add_note",
            "Add a timestamped note to a ticket. Notes are appended under a '## Notes' section.",
            schema_for!(AddNoteRequest),
        );
        let route = ToolRoute::new_dyn(
            tool,
            |ctx: rmcp::handler::server::tool::ToolCallContext<'_, Self>| {
                Box::pin(async move {
                    let this = ctx.service;
                    let args = ctx.arguments.ok_or(rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Borrowed("Missing arguments"),
                        data: None,
                    })?;
                    let request: AddNoteRequest = serde_json::from_value(
                        serde_json::Value::Object(args),
                    )
                    .map_err(|e| rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Owned(format!("Invalid parameters: {e}")),
                        data: None,
                    })?;
                    match this.add_note_impl(Parameters(request)).await {
                        Ok(result) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(result)],
                            structured_content: None,
                            is_error: None,
                            meta: None,
                        }),
                        Err(e) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(e)],
                            structured_content: None,
                            is_error: Some(true),
                            meta: None,
                        }),
                    }
                })
            },
        );
        router.add_route(route);

        // list_tickets
        let tool = create_tool_meta(
            "list_tickets",
            "Query tickets with optional filters. Returns a list of matching tickets with their metadata.",
            schema_for!(ListTicketsRequest),
        );
        let route = ToolRoute::new_dyn(
            tool,
            |ctx: rmcp::handler::server::tool::ToolCallContext<'_, Self>| {
                Box::pin(async move {
                    let this = ctx.service;
                    let args = ctx.arguments.unwrap_or_default();
                    let request: ListTicketsRequest = serde_json::from_value(
                        serde_json::Value::Object(args),
                    )
                    .map_err(|e| rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Owned(format!("Invalid parameters: {e}")),
                        data: None,
                    })?;
                    match this.list_tickets_impl(Parameters(request)).await {
                        Ok(result) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(result)],
                            structured_content: None,
                            is_error: None,
                            meta: None,
                        }),
                        Err(e) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(e)],
                            structured_content: None,
                            is_error: Some(true),
                            meta: None,
                        }),
                    }
                })
            },
        );
        router.add_route(route);

        // show_ticket
        let tool = create_tool_meta(
            "show_ticket",
            "Get full ticket content including metadata, body, dependencies, and relationships. Returns markdown optimized for LLM consumption.",
            schema_for!(ShowTicketRequest),
        );
        let route = ToolRoute::new_dyn(
            tool,
            |ctx: rmcp::handler::server::tool::ToolCallContext<'_, Self>| {
                Box::pin(async move {
                    let this = ctx.service;
                    let args = ctx.arguments.ok_or(rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Borrowed("Missing arguments"),
                        data: None,
                    })?;
                    let request: ShowTicketRequest = serde_json::from_value(
                        serde_json::Value::Object(args),
                    )
                    .map_err(|e| rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Owned(format!("Invalid parameters: {e}")),
                        data: None,
                    })?;
                    match this.show_ticket_impl(Parameters(request)).await {
                        Ok(result) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(result)],
                            structured_content: None,
                            is_error: None,
                            meta: None,
                        }),
                        Err(e) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(e)],
                            structured_content: None,
                            is_error: Some(true),
                            meta: None,
                        }),
                    }
                })
            },
        );
        router.add_route(route);

        // add_dependency
        let tool = create_tool_meta(
            "add_dependency",
            "Add a dependency. The first ticket will depend on the second (blocking relationship).",
            schema_for!(AddDependencyRequest),
        );
        let route = ToolRoute::new_dyn(
            tool,
            |ctx: rmcp::handler::server::tool::ToolCallContext<'_, Self>| {
                Box::pin(async move {
                    let this = ctx.service;
                    let args = ctx.arguments.ok_or(rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Borrowed("Missing arguments"),
                        data: None,
                    })?;
                    let request: AddDependencyRequest = serde_json::from_value(
                        serde_json::Value::Object(args),
                    )
                    .map_err(|e| rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Owned(format!("Invalid parameters: {e}")),
                        data: None,
                    })?;
                    match this.add_dependency_impl(Parameters(request)).await {
                        Ok(result) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(result)],
                            structured_content: None,
                            is_error: None,
                            meta: None,
                        }),
                        Err(e) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(e)],
                            structured_content: None,
                            is_error: Some(true),
                            meta: None,
                        }),
                    }
                })
            },
        );
        router.add_route(route);

        // remove_dependency
        let tool = create_tool_meta(
            "remove_dependency",
            "Remove a dependency from a ticket.",
            schema_for!(RemoveDependencyRequest),
        );
        let route = ToolRoute::new_dyn(
            tool,
            |ctx: rmcp::handler::server::tool::ToolCallContext<'_, Self>| {
                Box::pin(async move {
                    let this = ctx.service;
                    let args = ctx.arguments.ok_or(rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Borrowed("Missing arguments"),
                        data: None,
                    })?;
                    let request: RemoveDependencyRequest = serde_json::from_value(
                        serde_json::Value::Object(args),
                    )
                    .map_err(|e| rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Owned(format!("Invalid parameters: {e}")),
                        data: None,
                    })?;
                    match this.remove_dependency_impl(Parameters(request)).await {
                        Ok(result) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(result)],
                            structured_content: None,
                            is_error: None,
                            meta: None,
                        }),
                        Err(e) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(e)],
                            structured_content: None,
                            is_error: Some(true),
                            meta: None,
                        }),
                    }
                })
            },
        );
        router.add_route(route);

        // add_ticket_to_plan
        let tool = create_tool_meta(
            "add_ticket_to_plan",
            "Add a ticket to a plan. For phased plans, specify the phase.",
            schema_for!(AddTicketToPlanRequest),
        );
        let route = ToolRoute::new_dyn(
            tool,
            |ctx: rmcp::handler::server::tool::ToolCallContext<'_, Self>| {
                Box::pin(async move {
                    let this = ctx.service;
                    let args = ctx.arguments.ok_or(rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Borrowed("Missing arguments"),
                        data: None,
                    })?;
                    let request: AddTicketToPlanRequest = serde_json::from_value(
                        serde_json::Value::Object(args),
                    )
                    .map_err(|e| rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Owned(format!("Invalid parameters: {e}")),
                        data: None,
                    })?;
                    match this.add_ticket_to_plan_impl(Parameters(request)).await {
                        Ok(result) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(result)],
                            structured_content: None,
                            is_error: None,
                            meta: None,
                        }),
                        Err(e) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(e)],
                            structured_content: None,
                            is_error: Some(true),
                            meta: None,
                        }),
                    }
                })
            },
        );
        router.add_route(route);

        // get_plan_status
        let tool = create_tool_meta(
            "get_plan_status",
            "Get plan status including progress percentage and phase breakdown. Returns markdown optimized for LLM consumption.",
            schema_for!(GetPlanStatusRequest),
        );
        let route = ToolRoute::new_dyn(
            tool,
            |ctx: rmcp::handler::server::tool::ToolCallContext<'_, Self>| {
                Box::pin(async move {
                    let this = ctx.service;
                    let args = ctx.arguments.ok_or(rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Borrowed("Missing arguments"),
                        data: None,
                    })?;
                    let request: GetPlanStatusRequest = serde_json::from_value(
                        serde_json::Value::Object(args),
                    )
                    .map_err(|e| rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Owned(format!("Invalid parameters: {e}")),
                        data: None,
                    })?;
                    match this.get_plan_status_impl(Parameters(request)).await {
                        Ok(result) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(result)],
                            structured_content: None,
                            is_error: None,
                            meta: None,
                        }),
                        Err(e) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(e)],
                            structured_content: None,
                            is_error: Some(true),
                            meta: None,
                        }),
                    }
                })
            },
        );
        router.add_route(route);

        // get_children
        let tool = create_tool_meta(
            "get_children",
            "Get all tickets that were spawned from a given parent ticket. Returns markdown optimized for LLM consumption.",
            schema_for!(GetChildrenRequest),
        );
        let route = ToolRoute::new_dyn(
            tool,
            |ctx: rmcp::handler::server::tool::ToolCallContext<'_, Self>| {
                Box::pin(async move {
                    let this = ctx.service;
                    let args = ctx.arguments.ok_or(rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Borrowed("Missing arguments"),
                        data: None,
                    })?;
                    let request: GetChildrenRequest = serde_json::from_value(
                        serde_json::Value::Object(args),
                    )
                    .map_err(|e| rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Owned(format!("Invalid parameters: {e}")),
                        data: None,
                    })?;
                    match this.get_children_impl(Parameters(request)).await {
                        Ok(result) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(result)],
                            structured_content: None,
                            is_error: None,
                            meta: None,
                        }),
                        Err(e) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(e)],
                            structured_content: None,
                            is_error: Some(true),
                            meta: None,
                        }),
                    }
                })
            },
        );
        router.add_route(route);

        // get_next_available_ticket
        let tool = create_tool_meta(
            "get_next_available_ticket",
            "Query the Janus ticket backlog for the next ticket(s) to work on, based on priority and dependency resolution. Returns tickets in optimal order (dependencies before dependents). Use this if you've been instructed to work on tickets on the backlog. Do NOT use this for guidance on your current task.",
            schema_for!(GetNextAvailableTicketRequest),
        );
        let route = ToolRoute::new_dyn(
            tool,
            |ctx: rmcp::handler::server::tool::ToolCallContext<'_, Self>| {
                Box::pin(async move {
                    let this = ctx.service;
                    let args = ctx.arguments.unwrap_or_default();
                    let request: GetNextAvailableTicketRequest = serde_json::from_value(
                        serde_json::Value::Object(args),
                    )
                    .map_err(|e| rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Owned(format!("Invalid parameters: {e}")),
                        data: None,
                    })?;
                    match this.get_next_available_ticket_impl(Parameters(request)).await {
                        Ok(result) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(result)],
                            structured_content: None,
                            is_error: None,
                            meta: None,
                        }),
                        Err(e) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(e)],
                            structured_content: None,
                            is_error: Some(true),
                            meta: None,
                        }),
                    }
                })
            },
        );
        router.add_route(route);

        // semantic_search
        let tool = create_tool_meta(
            "semantic_search",
            "Find tickets semantically similar to a natural language query. Uses vector embeddings for fuzzy matching by intent rather than exact keywords.",
            schema_for!(SemanticSearchRequest),
        );
        let route = ToolRoute::new_dyn(
            tool,
            |ctx: rmcp::handler::server::tool::ToolCallContext<'_, Self>| {
                Box::pin(async move {
                    let this = ctx.service;
                    let args = ctx.arguments.ok_or(rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Borrowed("Missing arguments"),
                        data: None,
                    })?;
                    let request: SemanticSearchRequest = serde_json::from_value(
                        serde_json::Value::Object(args),
                    )
                    .map_err(|e| rmcp::model::ErrorData {
                        code: rmcp::model::ErrorCode::INVALID_PARAMS,
                        message: std::borrow::Cow::Owned(format!("Invalid parameters: {e}")),
                        data: None,
                    })?;
                    match this.semantic_search_impl(Parameters(request)).await {
                        Ok(result) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(result)],
                            structured_content: None,
                            is_error: None,
                            meta: None,
                        }),
                        Err(e) => Ok(rmcp::model::CallToolResult {
                            content: vec![rmcp::model::Content::text(e)],
                            structured_content: None,
                            is_error: Some(true),
                            meta: None,
                        }),
                    }
                })
            },
        );
        router.add_route(route);

        Self { tool_router: router }
    }

    /// Get the tool router for use with ServerHandler
    pub fn router(&self) -> &ToolRouter<Self> {
        &self.tool_router
    }

    // ========================================================================
    // Tool Implementations
    // ========================================================================

    /// Create a new ticket with the given title and optional metadata.
    /// Returns the created ticket ID and file path.
    /// Implementation for create_ticket tool
    async fn create_ticket_impl(
        &self,
        Parameters(request): Parameters<CreateTicketRequest>,
    ) -> Result<String, String> {
        let mut builder = TicketBuilder::new(&request.title)
            .description(request.description.as_deref())
            .run_hooks(true);

        if let Some(ref t) = request.ticket_type {
            // Validate ticket type
            TicketType::from_str(t).map_err(|_| format!("Invalid ticket type: {t}"))?;
            builder = builder.ticket_type(t);
        }

        if let Some(p) = request.priority {
            if p > 4 {
                return Err(format!("Priority must be 0-4, got {p}"));
            }
            builder = builder.priority(p.to_string());
        }

        // Parse and set size if provided
        let size = if let Some(ref s) = request.size {
            Some(TicketSize::from_str(s).map_err(|_| {
                format!(
                    "Invalid size: {s}. Valid values: xsmall/xs, small/s, medium/m, large/l, xlarge/xl"
                )
            })?)
        } else {
            None
        };
        builder = builder.size(size);

        let (id, _file_path) = builder.build().map_err(|e| e.to_string())?;

        // Log the event with MCP actor
        let ticket_type = request.ticket_type.as_deref().unwrap_or("task");
        let priority = request.priority.unwrap_or(2);
        let size_str = size.map(|s| s.to_string());
        log_event(
            Event::new(
                EventType::TicketCreated,
                EntityType::Ticket,
                &id,
                json!({
                    "title": request.title,
                    "type": ticket_type,
                    "priority": priority,
                    "size": size_str,
                }),
            )
            .with_actor(Actor::Mcp),
        );

        Ok(format!("Created ticket **{}**: \"{}\"", id, request.title))
    }

    /// Spawn a subtask from a parent ticket.
    /// Sets spawned_from, spawn_context, and depth fields.
    #[tool(
        name = "spawn_subtask",
        description = "Create a new ticket as a child of an existing ticket. Sets spawning metadata for decomposition tracking."
    )]
    /// Implementation for spawn_subtask tool
    async fn spawn_subtask_impl(
        &self,
        Parameters(request): Parameters<SpawnSubtaskRequest>,
    ) -> Result<String, String> {
        // Find the parent ticket to get its depth
        let parent = Ticket::find(&request.parent_id)
            .await
            .map_err(|e| format!("Parent ticket not found: {e}"))?;
        let parent_metadata = parent.read().map_err(|e| e.to_string())?;

        // Calculate new depth
        let parent_depth = parent_metadata.depth.unwrap_or(0);
        let new_depth = parent_depth + 1;

        let (id, _file_path) = TicketBuilder::new(&request.title)
            .description(request.description.as_deref())
            .spawned_from(Some(&parent.id))
            .spawn_context(request.spawn_context.as_deref())
            .depth(Some(new_depth))
            .run_hooks(true)
            .build()
            .map_err(|e| e.to_string())?;

        // Log with MCP actor
        log_event(
            Event::new(
                EventType::TicketCreated,
                EntityType::Ticket,
                &id,
                json!({
                    "title": request.title,
                    "type": "task",
                    "priority": 2,
                    "spawned_from": parent.id,
                    "depth": new_depth,
                }),
            )
            .with_actor(Actor::Mcp),
        );

        Ok(format!(
            "Spawned subtask **{}**: \"{}\" from parent {} (depth: {})",
            id, request.title, parent.id, new_depth
        ))
    }

    /// Update a ticket's status.
    /// When closing (complete/cancelled), optionally include a summary.
    #[tool(
        name = "update_status",
        description = "Change a ticket's status. Valid statuses: new, next, in_progress, complete, cancelled."
    )]
    /// Implementation for update_status tool
    async fn update_status_impl(
        &self,
        Parameters(request): Parameters<UpdateStatusRequest>,
    ) -> Result<String, String> {
        let ticket = Ticket::find(&request.id)
            .await
            .map_err(|e| format!("Ticket not found: {e}"))?;
        let metadata = ticket.read().map_err(|e| e.to_string())?;

        // Validate and parse status
        let new_status = TicketStatus::from_str(&request.status).map_err(|_| {
            format!(
                "Invalid status '{}'. Must be one of: new, next, in_progress, complete, cancelled",
                request.status
            )
        })?;

        let previous_status = metadata.status.unwrap_or_default();

        // Update the status field
        ticket
            .update_field("status", &new_status.to_string())
            .map_err(|e| e.to_string())?;

        // Write completion summary if provided and ticket is being closed
        if new_status.is_terminal()
            && let Some(ref summary) = request.summary
        {
            write_completion_summary(&ticket, summary).map_err(|e| e.to_string())?;
        }

        // Log with MCP actor
        log_event(
            Event::new(
                EventType::StatusChanged,
                EntityType::Ticket,
                &ticket.id,
                json!({
                    "from": previous_status.to_string(),
                    "to": new_status.to_string(),
                    "summary": request.summary,
                }),
            )
            .with_actor(Actor::Mcp),
        );

        Ok(format!(
            "Updated **{}** status: {} → {}",
            ticket.id, previous_status, new_status
        ))
    }

    /// Add a timestamped note to a ticket.
    #[tool(
        name = "add_note",
        description = "Add a timestamped note to a ticket. Notes are appended under a '## Notes' section."
    )]
    /// Implementation for add_note tool
    async fn add_note_impl(
        &self,
        Parameters(request): Parameters<AddNoteRequest>,
    ) -> Result<String, String> {
        if request.note.trim().is_empty() {
            return Err("Note content cannot be empty".to_string());
        }

        let ticket = Ticket::find(&request.id)
            .await
            .map_err(|e| format!("Ticket not found: {e}"))?;

        let mut content = fs::read_to_string(&ticket.file_path).map_err(|e| e.to_string())?;

        // Add Notes section if it doesn't exist
        if !content.contains("## Notes") {
            content.push_str("\n## Notes");
        }

        // Add the note with timestamp
        let timestamp = iso_date();
        content.push_str(&format!("\n\n**{}**\n\n{}", timestamp, request.note));

        fs::write(&ticket.file_path, &content).map_err(|e| e.to_string())?;

        // Log with MCP actor
        log_event(
            Event::new(
                EventType::NoteAdded,
                EntityType::Ticket,
                &ticket.id,
                json!({
                    "content_preview": if request.note.len() > 100 {
                        format!("{}...", &request.note[..97])
                    } else {
                        request.note.clone()
                    },
                }),
            )
            .with_actor(Actor::Mcp),
        );

        Ok(format!("Added note to **{}** at {}", ticket.id, timestamp))
    }

    /// List tickets with optional filters.
    #[tool(
        name = "list_tickets",
        description = "Query tickets with optional filters. Returns a list of matching tickets with their metadata."
    )]
    /// Implementation for list_tickets tool
    async fn list_tickets_impl(
        &self,
        Parameters(request): Parameters<ListTicketsRequest>,
    ) -> Result<String, String> {
        let (tickets, ticket_map) = get_all_tickets_with_map()
            .await
            .map_err(|e| format!("failed to load tickets: {e}"))?;

        // Resolve spawned_from partial ID if provided
        let resolved_spawned_from = if let Some(ref partial_id) = request.spawned_from {
            let ticket = Ticket::find(partial_id)
                .await
                .map_err(|e| format!("spawned_from ticket not found: {e}"))?;
            Some(ticket.id)
        } else {
            None
        };

        // Parse size filter if provided
        let size_filter: Option<Vec<TicketSize>> = if let Some(ref s) = request.size {
            let sizes: Result<Vec<TicketSize>, String> = s
                .split(',')
                .map(|size_str| {
                    TicketSize::from_str(size_str.trim()).map_err(|_| {
                        format!(
                            "Invalid size: {}. Valid values: xsmall/xs, small/s, medium/m, large/l, xlarge/xl",
                            size_str.trim()
                        )
                    })
                })
                .collect();
            Some(sizes?)
        } else {
            None
        };

        let filtered: Vec<&TicketMetadata> = tickets
            .iter()
            .filter(|t| {
                // Filter by spawned_from
                if let Some(ref parent_id) = resolved_spawned_from {
                    match &t.spawned_from {
                        Some(sf) if sf == parent_id => {}
                        _ => return false,
                    }
                }

                // Filter by depth
                if let Some(target_depth) = request.depth {
                    let ticket_depth = t
                        .depth
                        .unwrap_or_else(|| if t.spawned_from.is_none() { 0 } else { 1 });
                    if ticket_depth != target_depth {
                        return false;
                    }
                }

                // Filter by status
                if let Some(ref status_filter) = request.status {
                    let ticket_status = t
                        .status
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "new".to_string());
                    if ticket_status != *status_filter {
                        return false;
                    }
                }

                // Filter by type
                if let Some(ref type_filter) = request.ticket_type {
                    let ticket_type = t
                        .ticket_type
                        .map(|tt| tt.to_string())
                        .unwrap_or_else(|| "task".to_string());
                    if ticket_type != *type_filter {
                        return false;
                    }
                }

                // Filter by size
                if let Some(ref sizes) = size_filter {
                    if let Some(ticket_size) = t.size {
                        if !sizes.contains(&ticket_size) {
                            return false;
                        }
                    } else {
                        // Ticket has no size, exclude it when filtering by size
                        return false;
                    }
                }

                // Filter by ready (no incomplete dependencies)
                if request.ready == Some(true) {
                    if !matches!(t.status, Some(TicketStatus::New) | Some(TicketStatus::Next)) {
                        return false;
                    }
                    let all_deps_complete = t.deps.iter().all(|dep_id| {
                        ticket_map
                            .get(dep_id)
                            .map(|dep| dep.status == Some(TicketStatus::Complete))
                            .unwrap_or(false)
                    });
                    if !all_deps_complete {
                        return false;
                    }
                }

                // Filter by blocked (has incomplete dependencies)
                if request.blocked == Some(true) {
                    if !matches!(t.status, Some(TicketStatus::New) | Some(TicketStatus::Next)) {
                        return false;
                    }
                    if t.deps.is_empty() {
                        return false;
                    }
                    let has_incomplete_dep = t.deps.iter().any(|dep_id| {
                        ticket_map
                            .get(dep_id)
                            .map(|dep| dep.status != Some(TicketStatus::Complete))
                            .unwrap_or(true)
                    });
                    if !has_incomplete_dep {
                        return false;
                    }
                }

                // Exclude closed tickets by default (unless filtering by status)
                if request.status.is_none() {
                    let is_closed = matches!(
                        t.status,
                        Some(TicketStatus::Complete) | Some(TicketStatus::Cancelled)
                    );
                    if is_closed {
                        return false;
                    }
                }

                true
            })
            .collect();

        // Build filter summary
        let filter_summary = build_filter_summary(&request);

        // Format as markdown
        Ok(format_ticket_list_as_markdown(&filtered, &filter_summary))
    }

    /// Show full ticket content and metadata.
    #[tool(
        name = "show_ticket",
        description = "Get full ticket content including metadata, body, dependencies, and relationships. Returns markdown optimized for LLM consumption."
    )]
    /// Implementation for show_ticket tool
    async fn show_ticket_impl(
        &self,
        Parameters(request): Parameters<ShowTicketRequest>,
    ) -> Result<String, String> {
        let ticket = Ticket::find(&request.id)
            .await
            .map_err(|e| format!("Ticket not found: {e}"))?;
        let content = ticket.read_content().map_err(|e| e.to_string())?;
        let metadata = ticket.read().map_err(|e| e.to_string())?;
        let ticket_map = build_ticket_map()
            .await
            .map_err(|e| format!("failed to load tickets: {e}"))?;

        // Find blockers and blocking tickets
        let mut blockers: Vec<&TicketMetadata> = Vec::new();
        let mut blocking: Vec<&TicketMetadata> = Vec::new();
        let mut children: Vec<&TicketMetadata> = Vec::new();

        for (other_id, other) in &ticket_map {
            if other_id == &ticket.id {
                continue;
            }

            // Check if this is a child (spawned from current ticket)
            if other.spawned_from.as_ref() == Some(&ticket.id) {
                children.push(other);
            }

            // Check if other ticket is blocked by current ticket
            if other.deps.contains(&ticket.id) && other.status != Some(TicketStatus::Complete) {
                blocking.push(other);
            }
        }

        // Find blockers (deps that are not complete)
        for dep_id in &metadata.deps {
            if let Some(dep) = ticket_map.get(dep_id)
                && dep.status != Some(TicketStatus::Complete)
            {
                blockers.push(dep);
            }
        }

        Ok(format_ticket_as_markdown(
            &metadata, &content, &blockers, &blocking, &children,
        ))
    }

    /// Add a dependency between tickets.
    #[tool(
        name = "add_dependency",
        description = "Add a dependency. The first ticket will depend on the second (blocking relationship)."
    )]
    /// Implementation for add_dependency tool
    async fn add_dependency_impl(
        &self,
        Parameters(request): Parameters<AddDependencyRequest>,
    ) -> Result<String, String> {
        let ticket = Ticket::find(&request.ticket_id)
            .await
            .map_err(|e| format!("Ticket not found: {e}"))?;
        let dep_ticket = Ticket::find(&request.depends_on_id)
            .await
            .map_err(|e| format!("Dependency ticket not found: {e}"))?;

        // Check for circular dependencies
        let ticket_map = build_ticket_map()
            .await
            .map_err(|e| format!("failed to load tickets: {e}"))?;
        check_circular_dependency(&ticket.id, &dep_ticket.id, &ticket_map)?;

        let added = ticket
            .add_to_array_field("deps", &dep_ticket.id)
            .map_err(|e| e.to_string())?;

        if added {
            // Log with MCP actor
            log_event(
                Event::new(
                    EventType::DependencyAdded,
                    EntityType::Ticket,
                    &ticket.id,
                    json!({ "dependency_id": dep_ticket.id }),
                )
                .with_actor(Actor::Mcp),
            );
        }

        if added {
            Ok(format!(
                "Added dependency: **{}** now depends on **{}**",
                ticket.id, dep_ticket.id
            ))
        } else {
            Ok(format!(
                "Dependency already exists: **{}** already depends on **{}**",
                ticket.id, dep_ticket.id
            ))
        }
    }

    /// Remove a dependency between tickets.
    #[tool(
        name = "remove_dependency",
        description = "Remove a dependency from a ticket."
    )]
    /// Implementation for remove_dependency tool
    async fn remove_dependency_impl(
        &self,
        Parameters(request): Parameters<RemoveDependencyRequest>,
    ) -> Result<String, String> {
        let ticket = Ticket::find(&request.ticket_id)
            .await
            .map_err(|e| format!("Ticket not found: {e}"))?;

        let removed = ticket
            .remove_from_array_field("deps", &request.depends_on_id)
            .map_err(|e| e.to_string())?;

        if !removed {
            return Err(format!(
                "Dependency '{}' not found in ticket",
                request.depends_on_id
            ));
        }

        // Log with MCP actor
        log_event(
            Event::new(
                EventType::DependencyRemoved,
                EntityType::Ticket,
                &ticket.id,
                json!({ "dependency_id": request.depends_on_id }),
            )
            .with_actor(Actor::Mcp),
        );

        Ok(format!(
            "Removed dependency: **{}** no longer depends on **{}**",
            ticket.id, request.depends_on_id
        ))
    }

    /// Add a ticket to a plan.
    #[tool(
        name = "add_ticket_to_plan",
        description = "Add a ticket to a plan. For phased plans, specify the phase."
    )]
    /// Implementation for add_ticket_to_plan tool
    async fn add_ticket_to_plan_impl(
        &self,
        Parameters(request): Parameters<AddTicketToPlanRequest>,
    ) -> Result<String, String> {
        // Validate ticket exists
        let ticket = Ticket::find(&request.ticket_id)
            .await
            .map_err(|e| format!("Ticket not found: {e}"))?;

        let plan = Plan::find(&request.plan_id)
            .await
            .map_err(|e| format!("Plan not found: {e}"))?;
        let mut metadata = plan.read().map_err(|e| e.to_string())?;

        // Check if ticket is already in plan
        let existing_tickets = metadata.all_tickets();
        if existing_tickets.contains(&ticket.id.as_str()) {
            return Err(format!("Ticket '{}' is already in this plan", ticket.id));
        }

        let mut added_to_phase: Option<String> = None;

        if metadata.is_phased() {
            // Phased plan requires --phase
            let phase_identifier = request
                .phase
                .as_deref()
                .ok_or("Phased plan requires 'phase' parameter")?;

            let phase_obj = metadata
                .find_phase_mut(phase_identifier)
                .ok_or_else(|| format!("Phase '{phase_identifier}' not found"))?;

            added_to_phase = Some(phase_obj.name.clone());
            phase_obj.add_ticket(&ticket.id);
        } else if metadata.is_simple() {
            if request.phase.is_some() {
                return Err("Cannot use 'phase' parameter with simple plans".to_string());
            }

            let tickets = metadata
                .tickets_section_mut()
                .ok_or("Plan has no tickets section")?;
            tickets.push(ticket.id.clone());
        } else {
            return Err("Plan has no tickets section or phases".to_string());
        }

        // Write updated plan
        let content = serialize_plan(&metadata);
        plan.write(&content).map_err(|e| e.to_string())?;

        // Log with MCP actor
        log_event(
            Event::new(
                EventType::TicketAddedToPlan,
                EntityType::Plan,
                &plan.id,
                json!({
                    "ticket_id": ticket.id,
                    "phase": added_to_phase,
                }),
            )
            .with_actor(Actor::Mcp),
        );

        if let Some(phase_name) = added_to_phase {
            Ok(format!(
                "Added **{}** to plan **{}** ({})",
                ticket.id, plan.id, phase_name
            ))
        } else {
            Ok(format!("Added **{}** to plan **{}**", ticket.id, plan.id))
        }
    }

    /// Get plan status and progress.
    #[tool(
        name = "get_plan_status",
        description = "Get plan status including progress percentage and phase breakdown. Returns markdown optimized for LLM consumption."
    )]
    /// Implementation for get_plan_status tool
    async fn get_plan_status_impl(
        &self,
        Parameters(request): Parameters<GetPlanStatusRequest>,
    ) -> Result<String, String> {
        let plan = Plan::find(&request.plan_id)
            .await
            .map_err(|e| format!("Plan not found: {e}"))?;
        let metadata = plan.read().map_err(|e| e.to_string())?;
        let ticket_map = build_ticket_map()
            .await
            .map_err(|e| format!("failed to load tickets: {e}"))?;

        let plan_status = compute_plan_status(&metadata, &ticket_map);

        Ok(format_plan_status_as_markdown(
            &plan.id,
            &metadata,
            &plan_status,
            &ticket_map,
        ))
    }

    /// Get tickets spawned from a parent ticket.
    #[tool(
        name = "get_children",
        description = "Get all tickets that were spawned from a given parent ticket. Returns markdown optimized for LLM consumption."
    )]
    /// Implementation for get_children tool
    async fn get_children_impl(
        &self,
        Parameters(request): Parameters<GetChildrenRequest>,
    ) -> Result<String, String> {
        let parent = Ticket::find(&request.ticket_id)
            .await
            .map_err(|e| format!("Ticket not found: {e}"))?;
        let parent_metadata = parent.read().map_err(|e| e.to_string())?;

        let (tickets, _) = get_all_tickets_with_map()
            .await
            .map_err(|e| format!("failed to load tickets: {e}"))?;

        let children: Vec<&TicketMetadata> = tickets
            .iter()
            .filter(|t| t.spawned_from.as_ref() == Some(&parent.id))
            .collect();

        let parent_title = parent_metadata.title.as_deref().unwrap_or("Untitled");
        Ok(format_children_as_markdown(
            &parent.id,
            parent_title,
            &children,
        ))
    }

    /// Query the Janus ticket backlog for the next ticket(s) to work on.
    #[tool(
        name = "get_next_available_ticket",
        description = "Query the Janus ticket backlog for the next ticket(s) to work on, based on priority and dependency resolution. Returns tickets in optimal order (dependencies before dependents). Use this if you've been instructed to work on tickets on the backlog. Do NOT use this for guidance on your current task."
    )]
    /// Implementation for get_next_available_ticket tool
    async fn get_next_available_ticket_impl(
        &self,
        Parameters(request): Parameters<GetNextAvailableTicketRequest>,
    ) -> Result<String, String> {
        let limit = request.limit.unwrap_or(5);

        let ticket_map = build_ticket_map()
            .await
            .map_err(|e| format!("failed to load tickets: {e}"))?;

        if ticket_map.is_empty() {
            return Ok("No tickets found in the repository.".to_string());
        }

        // Check if all tickets are complete or cancelled
        let all_complete = ticket_map.values().all(|t| {
            matches!(
                t.status,
                Some(TicketStatus::Complete) | Some(TicketStatus::Cancelled)
            )
        });

        if all_complete {
            return Ok("All tickets are complete. Nothing to work on.".to_string());
        }

        let finder = NextWorkFinder::new(&ticket_map);
        let work_items = finder.get_next_work(limit).map_err(|e| e.to_string())?;

        if work_items.is_empty() {
            return Ok("No tickets ready to work on.".to_string());
        }

        Ok(format_next_work_as_markdown(&work_items, &ticket_map))
    }

    /// Find tickets semantically similar to a natural language query.
    #[tool(
        name = "semantic_search",
        description = "Find tickets semantically similar to a natural language query. Uses vector embeddings for fuzzy matching by intent rather than exact keywords."
    )]
    /// Implementation for semantic_search tool
    async fn semantic_search_impl(
        &self,
        Parameters(request): Parameters<SemanticSearchRequest>,
    ) -> Result<String, String> {
        // Validate query
        if request.query.trim().is_empty() {
            return Err("Search query cannot be empty".to_string());
        }

        // Check if semantic search is enabled
        match Config::load() {
            Ok(config) => {
                if !config.semantic_search_enabled() {
                    return Ok("Semantic search is disabled. Enable with: janus config set semantic_search.enabled true".to_string());
                }
            }
            Err(e) => {
                eprintln!(
                    "Warning: failed to load config: {e}. Proceeding with semantic search."
                );
            }
        }

        // Get cache
        let cache = get_ticket_cache()
            .await
            .map_err(|e| format!("Failed to access ticket cache: {e}. Ensure the cache is initialized with 'janus cache'."))?;

        // Check if embeddings available
        let (with_embedding, total) = cache
            .embedding_coverage()
            .await
            .map_err(|e| format!("Failed to check embedding coverage: {e}"))?;

        if total == 0 {
            return Err("No tickets found in the cache.".to_string());
        }

        if with_embedding == 0 {
            return Err("No ticket embeddings available. Run 'janus cache rebuild' to generate embeddings for all tickets.".to_string());
        }

        // Check for model version mismatch
        let needs_reembed = cache.needs_reembedding().await.unwrap_or(false);

        // Set defaults
        let limit = request.limit.unwrap_or(10);
        let threshold = request.threshold.unwrap_or(0.0);

        // Perform search
        let results = cache
            .semantic_search(&request.query, limit)
            .await
            .map_err(|e| format!("Search failed: {e}"))?;

        // Filter by threshold
        let results = results
            .into_iter()
            .filter(|r| r.similarity >= threshold)
            .collect::<Vec<_>>();

        // Format as table for LLM consumption using tabled
        if results.is_empty() {
            return Ok("No tickets found matching the query.".to_string());
        }

        use tabled::settings::Style;
        use tabled::{Table, Tabled};

        #[derive(Tabled)]
        struct SearchRow {
            #[tabled(rename = "ID")]
            id: String,
            #[tabled(rename = "Similarity")]
            similarity: String,
            #[tabled(rename = "Title")]
            title: String,
            #[tabled(rename = "Status")]
            status: String,
        }

        let rows: Vec<SearchRow> = results
            .iter()
            .map(|r| SearchRow {
                id: r.ticket.id.as_deref().unwrap_or("unknown").to_string(),
                similarity: format!("{:.2}", r.similarity),
                title: r.ticket.title.as_deref().unwrap_or("Untitled").to_string(),
                status: r
                    .ticket
                    .status
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "new".to_string()),
            })
            .collect();

        let mut table = Table::new(rows);
        table.with(Style::modern());

        let mut output = format!(
            "Found {} ticket(s) semantically similar to: {}\n\n",
            results.len(),
            request.query
        );
        output.push_str(&table.to_string());

        if with_embedding < total {
            let percentage = (with_embedding * 100) / total;
            output.push_str(&format!(
                "\n\n*Note: Only {with_embedding}/{total} tickets have embeddings ({percentage}%). Results may be incomplete. Run 'janus cache rebuild' to generate embeddings for all tickets.*"
            ));
        }

        if needs_reembed {
            output.push_str("\n\n*Warning: Embedding model version mismatch detected. Run 'janus cache rebuild' to update embeddings to the current model.*");
        }

        Ok(output)
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Write a completion summary section to a ticket file
fn write_completion_summary(ticket: &Ticket, summary: &str) -> crate::error::Result<()> {
    let content = ticket.read_content()?;

    // Check if there's already a Completion Summary section
    let section_pattern =
        regex::Regex::new(r"(?mi)^## completion summary\s*$").expect("regex should compile");
    let section_start = section_pattern.find(&content).map(|m| m.start());

    let new_content = if let Some(start_idx) = section_start {
        // Replace existing section
        let after_header = &content[start_idx..];
        let header_end = after_header
            .find('\n')
            .map(|i| i + 1)
            .unwrap_or(after_header.len());
        let section_content_start = start_idx + header_end;

        let section_content = &content[section_content_start..];
        let next_h2_re = regex::Regex::new(r"(?m)^## ").expect("regex should compile");
        let section_end = next_h2_re
            .find(section_content)
            .map(|m| section_content_start + m.start())
            .unwrap_or(content.len());

        let before = &content[..start_idx];
        let after = &content[section_end..];

        format!(
            "{}## Completion Summary\n\n{}\n{}",
            before,
            summary,
            if after.is_empty() { "" } else { "\n" }.to_owned() + after.trim_start_matches('\n')
        )
    } else {
        // Add new section at end
        let trimmed = content.trim_end();
        format!("{trimmed}\n\n## Completion Summary\n\n{summary}\n")
    };

    ticket.write(&new_content)
}

/// Format a ticket as markdown for LLM consumption
fn format_ticket_as_markdown(
    metadata: &TicketMetadata,
    content: &str,
    blockers: &[&TicketMetadata],
    blocking: &[&TicketMetadata],
    children: &[&TicketMetadata],
) -> String {
    let mut output = String::new();

    // Title with ID
    let id = format_ticket_id(metadata);
    let title = format_ticket_title(metadata);
    output.push_str(&format!("# {id}: {title}\n\n"));

    // Metadata table
    output.push_str("| Field | Value |\n");
    output.push_str("|-------|-------|\n");

    if let Some(status) = metadata.status {
        output.push_str(&format!("| Status | {status} |\n"));
    }
    if let Some(ticket_type) = metadata.ticket_type {
        output.push_str(&format!("| Type | {ticket_type} |\n"));
    }
    if let Some(priority) = metadata.priority {
        output.push_str(&format!("| Priority | P{} |\n", priority.as_num()));
    }
    if let Some(size) = metadata.size {
        output.push_str(&format!("| Size | {size} |\n"));
    }
    if let Some(ref created) = metadata.created {
        // Extract just the date portion (YYYY-MM-DD) from the ISO timestamp
        let date = created.split('T').next().unwrap_or(created);
        output.push_str(&format!("| Created | {date} |\n"));
    }
    if !metadata.deps.is_empty() {
        output.push_str(&format!(
            "| Dependencies | {} |\n",
            metadata.deps.join(", ")
        ));
    }
    if !metadata.links.is_empty() {
        output.push_str(&format!("| Links | {} |\n", metadata.links.join(", ")));
    }
    if let Some(ref parent) = metadata.parent {
        output.push_str(&format!("| Parent | {parent} |\n"));
    }
    if let Some(ref spawned_from) = metadata.spawned_from {
        output.push_str(&format!("| Spawned From | {spawned_from} |\n"));
    }
    if let Some(ref spawn_context) = metadata.spawn_context {
        output.push_str(&format!("| Spawn Context | {spawn_context} |\n"));
    }
    if let Some(depth) = metadata.depth {
        output.push_str(&format!("| Depth | {depth} |\n"));
    }
    if let Some(ref external_ref) = metadata.external_ref {
        output.push_str(&format!("| External Ref | {external_ref} |\n"));
    }
    if let Some(ref remote) = metadata.remote {
        output.push_str(&format!("| Remote | {remote} |\n"));
    }

    // Description section (the ticket body content)
    output.push_str("\n## Description\n\n");
    output.push_str(content.trim());
    output.push('\n');

    // Completion summary section (if present)
    if let Some(ref summary) = metadata.completion_summary {
        output.push_str("\n## Completion Summary\n\n");
        output.push_str(summary.trim());
        output.push('\n');
    }

    // Blockers section
    if let Some(section) = format_related_tickets_section("Blockers", blockers) {
        output.push_str(&section);
    }

    // Blocking section
    if let Some(section) = format_related_tickets_section("Blocking", blocking) {
        output.push_str(&section);
    }

    // Children section
    if let Some(section) = format_related_tickets_section("Children", children) {
        output.push_str(&section);
    }

    output
}

/// Build a human-readable filter summary from a ListTicketsRequest
fn build_filter_summary(request: &ListTicketsRequest) -> String {
    let mut filters = Vec::new();

    if request.ready == Some(true) {
        filters.push("ready tickets".to_string());
    }
    if request.blocked == Some(true) {
        filters.push("blocked tickets".to_string());
    }
    if let Some(ref status) = request.status {
        filters.push(format!("status={status}"));
    }
    if let Some(ref ticket_type) = request.ticket_type {
        filters.push(format!("type={ticket_type}"));
    }
    if let Some(ref spawned_from) = request.spawned_from {
        filters.push(format!("spawned_from={spawned_from}"));
    }
    if let Some(depth) = request.depth {
        filters.push(format!("depth={depth}"));
    }
    if let Some(ref size) = request.size {
        filters.push(format!("size={size}"));
    }

    if filters.is_empty() {
        String::new()
    } else {
        format!("**Showing:** {}\n\n", filters.join(", "))
    }
}

/// Format a list of tickets as markdown for LLM consumption
fn format_ticket_list_as_markdown(tickets: &[&TicketMetadata], filter_summary: &str) -> String {
    let mut output = String::new();

    // Header
    output.push_str("# Tickets\n\n");

    // Filter summary if any filters were applied
    if !filter_summary.is_empty() {
        output.push_str(filter_summary);
    }

    // Handle empty results
    if tickets.is_empty() {
        output.push_str("No tickets found matching criteria.\n");
        return output;
    }

    // Table header
    output.push_str("| ID | Title | Status | Type | Priority | Size |\n");
    output.push_str("|----|-------|--------|------|----------|------|\n");

    // Table rows using centralized formatting
    for ticket in tickets {
        output.push_str(&format_ticket_table_row(ticket));
    }

    // Total count
    output.push_str(&format!("\n**Total:** {} tickets\n", tickets.len()));

    output
}

/// Check for circular dependencies
fn check_circular_dependency(
    from_id: &str,
    to_id: &str,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> Result<(), String> {
    use std::collections::HashSet;

    // Direct circular check
    if let Some(dep_ticket) = ticket_map.get(to_id)
        && dep_ticket.deps.contains(&from_id.to_string())
    {
        return Err(format!(
            "Circular dependency: {to_id} already depends on {from_id}"
        ));
    }

    // Transitive circular check via DFS
    fn has_path_to(
        current: &str,
        target: &str,
        ticket_map: &HashMap<String, TicketMetadata>,
        visited: &mut HashSet<String>,
    ) -> bool {
        if current == target {
            return true;
        }
        if visited.contains(current) {
            return false;
        }
        visited.insert(current.to_string());

        if let Some(ticket) = ticket_map.get(current) {
            for dep in &ticket.deps {
                if has_path_to(dep, target, ticket_map, visited) {
                    return true;
                }
            }
        }
        false
    }

    let mut visited = HashSet::new();
    if has_path_to(to_id, from_id, ticket_map, &mut visited) {
        return Err(format!(
            "Circular dependency: adding {from_id} -> {to_id} would create a cycle"
        ));
    }

    Ok(())
}

/// Format plan status as markdown for LLM consumption
fn format_plan_status_as_markdown(
    plan_id: &str,
    metadata: &PlanMetadata,
    plan_status: &PlanStatus,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> String {
    let mut output = String::new();

    // Title and ID
    let title = metadata.title.as_deref().unwrap_or("Untitled");
    output.push_str(&format!("# Plan: {plan_id} - {title}\n\n"));

    // Overall status and progress
    output.push_str(&format!("**Status:** {}  \n", plan_status.status));
    output.push_str(&format!(
        "**Progress:** {}/{} tickets complete ({}%)\n",
        plan_status.completed_count,
        plan_status.total_count,
        plan_status.progress_percent() as u32
    ));

    if metadata.is_phased() {
        // Phased plan: show phases with tickets
        let phase_statuses = compute_all_phase_statuses(metadata, ticket_map);

        for (phase, phase_status) in metadata.phases().iter().zip(phase_statuses.iter()) {
            output.push_str(&format!(
                "\n## Phase {}: {} ({})",
                phase.number, phase.name, phase_status.status
            ));

            for ticket_id in &phase.tickets {
                output.push_str(&format_plan_ticket_entry(ticket_id, ticket_map));
            }
        }
    } else {
        // Simple plan: show tickets in a single list
        let tickets = metadata.all_tickets();
        if !tickets.is_empty() {
            output.push_str("\n## Tickets\n");
            for ticket_id in tickets {
                output.push_str(&format_plan_ticket_entry(ticket_id, ticket_map));
            }
        }
    }

    output
}

/// Format children of a ticket as markdown for LLM consumption
fn format_children_as_markdown(
    parent_id: &str,
    parent_title: &str,
    children: &[&TicketMetadata],
) -> String {
    let mut output = String::new();

    // Header with parent info
    output.push_str(&format!(
        "# Children of {parent_id}: {parent_title}\n\n"
    ));

    // Handle empty results
    if children.is_empty() {
        output.push_str("No children found for this ticket.\n");
        return output;
    }

    // Spawned count
    output.push_str(&format!("**Spawned tickets:** {}\n\n", children.len()));

    // Table header
    output.push_str("| ID | Title | Status | Depth |\n");
    output.push_str("|----|-------|--------|-------|\n");

    // Table rows using centralized formatting
    for child in children {
        output.push_str(&format_children_table_row(child));
    }

    // Spawn contexts section (only if any children have spawn_context)
    let children_with_context: Vec<_> = children
        .iter()
        .filter(|c| c.spawn_context.is_some())
        .collect();

    if !children_with_context.is_empty() {
        output.push_str("\n**Spawn contexts:**\n");
        for child in children_with_context {
            if let Some(line) = format_spawn_context_line(child) {
                output.push_str(&line);
            }
        }
    }

    output
}

/// Format next work items as markdown for LLM consumption
fn format_next_work_as_markdown(
    work_items: &[WorkItem],
    ticket_map: &HashMap<String, TicketMetadata>,
) -> String {
    let mut output = String::new();

    // Header
    output.push_str("## Next Work Items\n\n");

    // Numbered list of work items
    for (idx, item) in work_items.iter().enumerate() {
        let ticket_id = &item.ticket_id;
        let priority = item.metadata.priority_num();
        let title = format_ticket_title(&item.metadata);
        let priority_badge = format!("[P{priority}]");

        // Format the main line with context
        let context = match &item.reason {
            InclusionReason::Blocking(target_id) => {
                format!(" *(blocks {target_id})*")
            }
            InclusionReason::TargetBlocked => " *(currently blocked)*".to_string(),
            InclusionReason::Ready => String::new(),
        };

        output.push_str(&format!(
            "{}. **{}** {} {}{}\n",
            idx + 1,
            ticket_id,
            priority_badge,
            title,
            context
        ));

        // Status line
        let status = match &item.reason {
            InclusionReason::Ready | InclusionReason::Blocking(_) => "ready",
            InclusionReason::TargetBlocked => "blocked",
        };
        output.push_str(&format!("   - Status: {status}\n"));

        // Additional context for blocked tickets
        if matches!(item.reason, InclusionReason::TargetBlocked) {
            let incomplete_deps: Vec<&String> = item
                .metadata
                .deps
                .iter()
                .filter(|dep_id| {
                    ticket_map
                        .get(*dep_id)
                        .map(|dep| dep.status != Some(TicketStatus::Complete))
                        .unwrap_or(false)
                })
                .collect();

            if !incomplete_deps.is_empty() {
                let dep_list: Vec<&str> = incomplete_deps.iter().map(|s| s.as_str()).collect();
                output.push_str(&format!("   - Waiting on: {}\n", dep_list.join(", ")));
            }
        }

        // Context about what this ticket blocks
        if let Some(blocks) = &item.blocks {
            output.push_str(&format!(
                "   - This ticket must be completed before {blocks} can be worked on\n"
            ));
        }

        output.push('\n');
    }

    // Find the first ready ticket for recommended action
    let first_ready = work_items.iter().find(|item| {
        matches!(
            item.reason,
            InclusionReason::Ready | InclusionReason::Blocking(_)
        )
    });

    if let Some(ready_item) = first_ready {
        let ready_title = format_ticket_title(&ready_item.metadata);
        output.push_str("### Recommended Action\n\n");
        output.push_str(&format!(
            "Start with **{}**: {}\n",
            ready_item.ticket_id, ready_title
        ));
    } else {
        // All items are blocked
        output.push_str("### Note\n\n");
        output.push_str("All listed tickets are currently blocked by dependencies. Consider working on the dependencies first or reviewing the dependency chain.\n");
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_ticket_request_schema() {
        // Verify the request type implements JsonSchema
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
    fn test_circular_dependency_direct() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "a".to_string(),
            TicketMetadata {
                id: Some("a".to_string()),
                deps: vec!["b".to_string()],
                ..Default::default()
            },
        );
        ticket_map.insert(
            "b".to_string(),
            TicketMetadata {
                id: Some("b".to_string()),
                deps: vec![],
                ..Default::default()
            },
        );

        // b -> a should fail because a already depends on b
        let result = check_circular_dependency("b", "a", &ticket_map);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Circular dependency"));
    }

    #[test]
    fn test_circular_dependency_transitive() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "a".to_string(),
            TicketMetadata {
                id: Some("a".to_string()),
                deps: vec!["b".to_string()],
                ..Default::default()
            },
        );
        ticket_map.insert(
            "b".to_string(),
            TicketMetadata {
                id: Some("b".to_string()),
                deps: vec!["c".to_string()],
                ..Default::default()
            },
        );
        ticket_map.insert(
            "c".to_string(),
            TicketMetadata {
                id: Some("c".to_string()),
                deps: vec![],
                ..Default::default()
            },
        );

        // c -> a should fail because a -> b -> c
        let result = check_circular_dependency("c", "a", &ticket_map);
        assert!(result.is_err());
    }

    #[test]
    fn test_no_circular_dependency() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "a".to_string(),
            TicketMetadata {
                id: Some("a".to_string()),
                deps: vec![],
                ..Default::default()
            },
        );
        ticket_map.insert(
            "b".to_string(),
            TicketMetadata {
                id: Some("b".to_string()),
                deps: vec![],
                ..Default::default()
            },
        );

        // a -> b should succeed
        let result = check_circular_dependency("a", "b", &ticket_map);
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_filter_summary_empty() {
        let request = ListTicketsRequest::default();
        let summary = build_filter_summary(&request);
        assert!(summary.is_empty());
    }

    #[test]
    fn test_build_filter_summary_ready() {
        let request = ListTicketsRequest {
            ready: Some(true),
            ..Default::default()
        };
        let summary = build_filter_summary(&request);
        assert_eq!(summary, "**Showing:** ready tickets\n\n");
    }

    #[test]
    fn test_build_filter_summary_multiple() {
        let request = ListTicketsRequest {
            status: Some("new".to_string()),
            ticket_type: Some("bug".to_string()),
            ..Default::default()
        };
        let summary = build_filter_summary(&request);
        assert_eq!(summary, "**Showing:** status=new, type=bug\n\n");
    }

    #[test]
    fn test_format_ticket_list_empty() {
        let tickets: Vec<&TicketMetadata> = vec![];
        let output = format_ticket_list_as_markdown(&tickets, "");
        assert!(output.contains("# Tickets"));
        assert!(output.contains("No tickets found matching criteria."));
        assert!(!output.contains("| ID |"));
    }

    #[test]
    fn test_format_ticket_list_with_tickets() {
        use crate::types::{TicketPriority, TicketStatus, TicketType};

        let ticket1 = TicketMetadata {
            id: Some("j-a1b2".to_string()),
            title: Some("Add authentication".to_string()),
            status: Some(TicketStatus::New),
            ticket_type: Some(TicketType::Feature),
            priority: Some(TicketPriority::P1),
            ..Default::default()
        };
        let ticket2 = TicketMetadata {
            id: Some("j-c3d4".to_string()),
            title: Some("Fix login bug".to_string()),
            status: Some(TicketStatus::InProgress),
            ticket_type: Some(TicketType::Bug),
            priority: Some(TicketPriority::P2),
            ..Default::default()
        };
        let tickets = vec![&ticket1, &ticket2];
        let output = format_ticket_list_as_markdown(&tickets, "");

        assert!(output.contains("# Tickets"));
        assert!(output.contains("| ID | Title | Status | Type | Priority |"));
        assert!(output.contains("| j-a1b2 | Add authentication | new | feature | P1 |"));
        assert!(output.contains("| j-c3d4 | Fix login bug | in_progress | bug | P2 |"));
        assert!(output.contains("**Total:** 2 tickets"));
    }

    #[test]
    fn test_format_ticket_list_with_filter_summary() {
        let tickets: Vec<&TicketMetadata> = vec![];
        let output = format_ticket_list_as_markdown(&tickets, "**Showing:** ready tickets\n\n");
        assert!(output.contains("**Showing:** ready tickets"));
    }

    #[test]
    fn test_format_plan_status_simple_plan() {
        use crate::plan::types::{PlanMetadata, PlanSection, PlanStatus};
        use crate::types::{TicketPriority, TicketStatus, TicketType};

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a1b2".to_string(),
            TicketMetadata {
                id: Some("j-a1b2".to_string()),
                title: Some("Configure OAuth provider".to_string()),
                status: Some(TicketStatus::Complete),
                ticket_type: Some(TicketType::Task),
                priority: Some(TicketPriority::P2),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-c3d4".to_string(),
            TicketMetadata {
                id: Some("j-c3d4".to_string()),
                title: Some("Add auth dependencies".to_string()),
                status: Some(TicketStatus::InProgress),
                ticket_type: Some(TicketType::Task),
                priority: Some(TicketPriority::P2),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-e5f6".to_string(),
            TicketMetadata {
                id: Some("j-e5f6".to_string()),
                title: Some("Implement logout".to_string()),
                status: Some(TicketStatus::New),
                ticket_type: Some(TicketType::Task),
                priority: Some(TicketPriority::P2),
                ..Default::default()
            },
        );

        let metadata = PlanMetadata {
            id: Some("plan-a1b2".to_string()),
            title: Some("Implement Authentication".to_string()),
            sections: vec![PlanSection::Tickets(vec![
                "j-a1b2".to_string(),
                "j-c3d4".to_string(),
                "j-e5f6".to_string(),
            ])],
            ..Default::default()
        };

        let plan_status = PlanStatus {
            status: TicketStatus::InProgress,
            completed_count: 1,
            total_count: 3,
        };

        let output =
            format_plan_status_as_markdown("plan-a1b2", &metadata, &plan_status, &ticket_map);

        // Check header
        assert!(output.contains("# Plan: plan-a1b2 - Implement Authentication"));
        // Check status and progress
        assert!(output.contains("**Status:** in_progress"));
        assert!(output.contains("**Progress:** 1/3 tickets complete (33%)"));
        // Check tickets
        assert!(output.contains("## Tickets"));
        assert!(output.contains("- [x] j-a1b2: Configure OAuth provider"));
        assert!(output.contains("- [ ] j-c3d4: Add auth dependencies (in_progress)"));
        assert!(output.contains("- [ ] j-e5f6: Implement logout"));
    }

    #[test]
    fn test_format_plan_status_phased_plan() {
        use crate::plan::types::{Phase, PlanMetadata, PlanSection, PlanStatus};
        use crate::types::{TicketPriority, TicketStatus, TicketType};

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a1b2".to_string(),
            TicketMetadata {
                id: Some("j-a1b2".to_string()),
                title: Some("Configure OAuth provider".to_string()),
                status: Some(TicketStatus::Complete),
                ticket_type: Some(TicketType::Task),
                priority: Some(TicketPriority::P2),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-c3d4".to_string(),
            TicketMetadata {
                id: Some("j-c3d4".to_string()),
                title: Some("Add auth dependencies".to_string()),
                status: Some(TicketStatus::Complete),
                ticket_type: Some(TicketType::Task),
                priority: Some(TicketPriority::P2),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-e5f6".to_string(),
            TicketMetadata {
                id: Some("j-e5f6".to_string()),
                title: Some("Create login endpoint".to_string()),
                status: Some(TicketStatus::Complete),
                ticket_type: Some(TicketType::Task),
                priority: Some(TicketPriority::P2),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-g7h8".to_string(),
            TicketMetadata {
                id: Some("j-g7h8".to_string()),
                title: Some("Add session management".to_string()),
                status: Some(TicketStatus::InProgress),
                ticket_type: Some(TicketType::Task),
                priority: Some(TicketPriority::P2),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-i9j0".to_string(),
            TicketMetadata {
                id: Some("j-i9j0".to_string()),
                title: Some("Implement logout".to_string()),
                status: Some(TicketStatus::New),
                ticket_type: Some(TicketType::Task),
                priority: Some(TicketPriority::P2),
                ..Default::default()
            },
        );

        let mut phase1 = Phase::new("1", "Setup");
        phase1.tickets = vec!["j-a1b2".to_string(), "j-c3d4".to_string()];

        let mut phase2 = Phase::new("2", "Implementation");
        phase2.tickets = vec![
            "j-e5f6".to_string(),
            "j-g7h8".to_string(),
            "j-i9j0".to_string(),
        ];

        let metadata = PlanMetadata {
            id: Some("plan-a1b2".to_string()),
            title: Some("Implement Authentication".to_string()),
            sections: vec![PlanSection::Phase(phase1), PlanSection::Phase(phase2)],
            ..Default::default()
        };

        let plan_status = PlanStatus {
            status: TicketStatus::InProgress,
            completed_count: 3,
            total_count: 5,
        };

        let output =
            format_plan_status_as_markdown("plan-a1b2", &metadata, &plan_status, &ticket_map);

        // Check header
        assert!(output.contains("# Plan: plan-a1b2 - Implement Authentication"));
        // Check status and progress
        assert!(output.contains("**Status:** in_progress"));
        assert!(output.contains("**Progress:** 3/5 tickets complete (60%)"));
        // Check phases
        assert!(output.contains("## Phase 1: Setup (complete)"));
        assert!(output.contains("- [x] j-a1b2: Configure OAuth provider"));
        assert!(output.contains("- [x] j-c3d4: Add auth dependencies"));
        assert!(output.contains("## Phase 2: Implementation (in_progress)"));
        assert!(output.contains("- [x] j-e5f6: Create login endpoint"));
        assert!(output.contains("- [ ] j-g7h8: Add session management (in_progress)"));
        assert!(output.contains("- [ ] j-i9j0: Implement logout"));
    }

    #[test]
    fn test_format_plan_ticket_line_complete() {
        use super::super::format::format_plan_ticket_line;

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a1b2".to_string(),
            TicketMetadata {
                id: Some("j-a1b2".to_string()),
                title: Some("Test ticket".to_string()),
                status: Some(TicketStatus::Complete),
                ..Default::default()
            },
        );

        let (checkbox, title, suffix) = format_plan_ticket_line("j-a1b2", &ticket_map);
        assert_eq!(checkbox, 'x');
        assert_eq!(title, "Test ticket");
        assert_eq!(suffix, "\n");
    }

    #[test]
    fn test_format_plan_ticket_line_in_progress() {
        use super::super::format::format_plan_ticket_line;

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a1b2".to_string(),
            TicketMetadata {
                id: Some("j-a1b2".to_string()),
                title: Some("Test ticket".to_string()),
                status: Some(TicketStatus::InProgress),
                ..Default::default()
            },
        );

        let (checkbox, title, suffix) = format_plan_ticket_line("j-a1b2", &ticket_map);
        assert_eq!(checkbox, ' ');
        assert_eq!(title, "Test ticket");
        assert_eq!(suffix, " (in_progress)\n");
    }

    #[test]
    fn test_format_plan_ticket_line_not_found() {
        use super::super::format::format_plan_ticket_line;

        let ticket_map = HashMap::new();

        let (checkbox, title, suffix) = format_plan_ticket_line("j-unknown", &ticket_map);
        assert_eq!(checkbox, ' ');
        assert_eq!(title, "Unknown ticket");
        assert_eq!(suffix, "\n");
    }

    #[test]
    fn test_format_children_as_markdown_empty() {
        let children: Vec<&TicketMetadata> = vec![];
        let output = format_children_as_markdown("j-a1b2", "Add authentication", &children);

        assert!(output.contains("# Children of j-a1b2: Add authentication"));
        assert!(output.contains("No children found for this ticket."));
        assert!(!output.contains("| ID |"));
    }

    #[test]
    fn test_format_children_as_markdown_with_children() {
        use crate::types::TicketStatus;

        let child1 = TicketMetadata {
            id: Some("j-c3d4".to_string()),
            title: Some("Setup OAuth".to_string()),
            status: Some(TicketStatus::Complete),
            depth: Some(1),
            ..Default::default()
        };
        let child2 = TicketMetadata {
            id: Some("j-e5f6".to_string()),
            title: Some("Add login flow".to_string()),
            status: Some(TicketStatus::InProgress),
            depth: Some(1),
            ..Default::default()
        };
        let child3 = TicketMetadata {
            id: Some("j-g7h8".to_string()),
            title: Some("Add logout flow".to_string()),
            status: Some(TicketStatus::New),
            depth: Some(1),
            ..Default::default()
        };
        let children = vec![&child1, &child2, &child3];
        let output = format_children_as_markdown("j-a1b2", "Add authentication", &children);

        // Check header
        assert!(output.contains("# Children of j-a1b2: Add authentication"));
        // Check count
        assert!(output.contains("**Spawned tickets:** 3"));
        // Check table header
        assert!(output.contains("| ID | Title | Status | Depth |"));
        // Check table rows
        assert!(output.contains("| j-c3d4 | Setup OAuth | complete | 1 |"));
        assert!(output.contains("| j-e5f6 | Add login flow | in_progress | 1 |"));
        assert!(output.contains("| j-g7h8 | Add logout flow | new | 1 |"));
    }

    #[test]
    fn test_format_children_as_markdown_with_spawn_context() {
        use crate::types::TicketStatus;

        let child1 = TicketMetadata {
            id: Some("j-c3d4".to_string()),
            title: Some("Setup OAuth".to_string()),
            status: Some(TicketStatus::Complete),
            depth: Some(1),
            spawn_context: Some("Setting up OAuth as the first step".to_string()),
            ..Default::default()
        };
        let child2 = TicketMetadata {
            id: Some("j-e5f6".to_string()),
            title: Some("Add login flow".to_string()),
            status: Some(TicketStatus::InProgress),
            depth: Some(1),
            spawn_context: Some("Login flow implementation".to_string()),
            ..Default::default()
        };
        let child3 = TicketMetadata {
            id: Some("j-g7h8".to_string()),
            title: Some("Add logout flow".to_string()),
            status: Some(TicketStatus::New),
            depth: Some(1),
            // No spawn_context for this one
            ..Default::default()
        };
        let children = vec![&child1, &child2, &child3];
        let output = format_children_as_markdown("j-a1b2", "Add authentication", &children);

        // Check spawn contexts section
        assert!(output.contains("**Spawn contexts:**"));
        assert!(output.contains("- **j-c3d4**: \"Setting up OAuth as the first step\""));
        assert!(output.contains("- **j-e5f6**: \"Login flow implementation\""));
        // Should NOT include j-g7h8 since it has no spawn_context
        assert!(!output.contains("- **j-g7h8**:"));
    }

    #[test]
    fn test_format_children_as_markdown_no_spawn_context() {
        use crate::types::TicketStatus;

        let child1 = TicketMetadata {
            id: Some("j-c3d4".to_string()),
            title: Some("Setup OAuth".to_string()),
            status: Some(TicketStatus::Complete),
            depth: Some(1),
            // No spawn_context
            ..Default::default()
        };
        let children = vec![&child1];
        let output = format_children_as_markdown("j-a1b2", "Add authentication", &children);

        // Should NOT contain spawn contexts section
        assert!(!output.contains("**Spawn contexts:**"));
    }

    #[test]
    fn test_create_ticket_request_schema_includes_size() {
        // Verify the request type schema includes size field
        let schema = schemars::schema_for!(CreateTicketRequest);
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("size"));
        assert!(json.contains("xsmall") || json.contains("xs"));
    }

    #[test]
    fn test_list_tickets_request_schema_includes_size() {
        // Verify the request type schema includes size field
        let schema = schemars::schema_for!(ListTicketsRequest);
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("size"));
    }

    #[test]
    fn test_format_ticket_list_includes_size() {
        use crate::types::{TicketPriority, TicketSize, TicketStatus, TicketType};

        let ticket1 = TicketMetadata {
            id: Some("j-a1b2".to_string()),
            title: Some("Add authentication".to_string()),
            status: Some(TicketStatus::New),
            ticket_type: Some(TicketType::Feature),
            priority: Some(TicketPriority::P1),
            size: Some(TicketSize::Medium),
            ..Default::default()
        };
        let ticket2 = TicketMetadata {
            id: Some("j-c3d4".to_string()),
            title: Some("Fix login bug".to_string()),
            status: Some(TicketStatus::InProgress),
            ticket_type: Some(TicketType::Bug),
            priority: Some(TicketPriority::P2),
            size: Some(TicketSize::Small),
            ..Default::default()
        };
        let ticket3 = TicketMetadata {
            id: Some("j-e5f6".to_string()),
            title: Some("Update docs".to_string()),
            status: Some(TicketStatus::New),
            ticket_type: Some(TicketType::Task),
            priority: Some(TicketPriority::P3),
            // No size set
            ..Default::default()
        };
        let tickets = vec![&ticket1, &ticket2, &ticket3];
        let output = format_ticket_list_as_markdown(&tickets, "");

        // Check table header includes Size column
        assert!(output.contains("| ID | Title | Status | Type | Priority | Size |"));
        // Check rows include size values
        assert!(output.contains("| j-a1b2 | Add authentication | new | feature | P1 | medium |"));
        assert!(output.contains("| j-c3d4 | Fix login bug | in_progress | bug | P2 | small |"));
        assert!(output.contains("| j-e5f6 | Update docs | new | task | P3 | - |"));
    }

    #[test]
    fn test_format_ticket_as_markdown_includes_size() {
        use crate::types::{TicketSize, TicketStatus};

        let metadata = TicketMetadata {
            id: Some("j-test".to_string()),
            title: Some("Test ticket".to_string()),
            status: Some(TicketStatus::New),
            size: Some(TicketSize::Large),
            ..Default::default()
        };

        let output = format_ticket_as_markdown(&metadata, "Test content", &[], &[], &[]);

        // Check size is in the metadata table
        assert!(output.contains("| Size | large |"));
    }

    #[test]
    fn test_build_filter_summary_includes_size() {
        let request = ListTicketsRequest {
            size: Some("medium,large".to_string()),
            ..Default::default()
        };
        let summary = build_filter_summary(&request);
        assert_eq!(summary, "**Showing:** size=medium,large\n\n");
    }

    #[test]
    fn test_ticket_size_parsing_for_mcp() {
        // Test full names
        assert_eq!("xsmall".parse::<TicketSize>().unwrap(), TicketSize::XSmall);
        assert_eq!("small".parse::<TicketSize>().unwrap(), TicketSize::Small);
        assert_eq!("medium".parse::<TicketSize>().unwrap(), TicketSize::Medium);
        assert_eq!("large".parse::<TicketSize>().unwrap(), TicketSize::Large);
        assert_eq!("xlarge".parse::<TicketSize>().unwrap(), TicketSize::XLarge);

        // Test aliases
        assert_eq!("xs".parse::<TicketSize>().unwrap(), TicketSize::XSmall);
        assert_eq!("s".parse::<TicketSize>().unwrap(), TicketSize::Small);
        assert_eq!("m".parse::<TicketSize>().unwrap(), TicketSize::Medium);
        assert_eq!("l".parse::<TicketSize>().unwrap(), TicketSize::Large);
        assert_eq!("xl".parse::<TicketSize>().unwrap(), TicketSize::XLarge);

        // Test case insensitivity
        assert_eq!("MEDIUM".parse::<TicketSize>().unwrap(), TicketSize::Medium);
        assert_eq!("M".parse::<TicketSize>().unwrap(), TicketSize::Medium);
        assert_eq!("XLarge".parse::<TicketSize>().unwrap(), TicketSize::XLarge);

        // Test invalid size
        assert!("invalid".parse::<TicketSize>().is_err());
        assert!("tiny".parse::<TicketSize>().is_err());
    }
}
