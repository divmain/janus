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

use rmcp::{
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    schemars::{self, JsonSchema},
    tool, tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::str::FromStr;

use crate::events::{Actor, EntityType, Event, EventType, log_event};
use crate::plan::parser::serialize_plan;
use crate::plan::{Plan, compute_all_phase_statuses, compute_plan_status};
use crate::ticket::{Ticket, TicketBuilder, build_ticket_map, get_all_tickets_with_map};
use crate::types::{TicketMetadata, TicketStatus, TicketType};
use crate::utils::iso_date;

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

#[tool_router]
impl JanusTools {
    /// Create a new JanusTools instance with all tools registered
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
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
    #[tool(
        name = "create_ticket",
        description = "Create a new ticket. Returns the ticket ID and file path."
    )]
    async fn create_ticket(
        &self,
        Parameters(request): Parameters<CreateTicketRequest>,
    ) -> Result<String, String> {
        let mut builder = TicketBuilder::new(&request.title)
            .description(request.description.as_deref())
            .run_hooks(true);

        if let Some(ref t) = request.ticket_type {
            // Validate ticket type
            TicketType::from_str(t).map_err(|_| format!("Invalid ticket type: {}", t))?;
            builder = builder.ticket_type(t);
        }

        if let Some(p) = request.priority {
            if p > 4 {
                return Err(format!("Priority must be 0-4, got {}", p));
            }
            builder = builder.priority(p.to_string());
        }

        let (id, file_path) = builder.build().map_err(|e| e.to_string())?;

        // Log the event with MCP actor
        let ticket_type = request.ticket_type.as_deref().unwrap_or("task");
        let priority = request.priority.unwrap_or(2);
        log_event(
            Event::new(
                EventType::TicketCreated,
                EntityType::Ticket,
                &id,
                json!({
                    "title": request.title,
                    "type": ticket_type,
                    "priority": priority,
                }),
            )
            .with_actor(Actor::Mcp),
        );

        Ok(serde_json::to_string_pretty(&json!({
            "success": true,
            "id": id,
            "file_path": file_path.to_string_lossy(),
            "title": request.title,
            "type": ticket_type,
            "priority": priority,
        }))
        .unwrap())
    }

    /// Spawn a subtask from a parent ticket.
    /// Sets spawned_from, spawn_context, and depth fields.
    #[tool(
        name = "spawn_subtask",
        description = "Create a new ticket as a child of an existing ticket. Sets spawning metadata for decomposition tracking."
    )]
    async fn spawn_subtask(
        &self,
        Parameters(request): Parameters<SpawnSubtaskRequest>,
    ) -> Result<String, String> {
        // Find the parent ticket to get its depth
        let parent = Ticket::find(&request.parent_id)
            .await
            .map_err(|e| format!("Parent ticket not found: {}", e))?;
        let parent_metadata = parent.read().map_err(|e| e.to_string())?;

        // Calculate new depth
        let parent_depth = parent_metadata.depth.unwrap_or(0);
        let new_depth = parent_depth + 1;

        let (id, file_path) = TicketBuilder::new(&request.title)
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

        Ok(serde_json::to_string_pretty(&json!({
            "success": true,
            "id": id,
            "file_path": file_path.to_string_lossy(),
            "title": request.title,
            "spawned_from": parent.id,
            "depth": new_depth,
        }))
        .unwrap())
    }

    /// Update a ticket's status.
    /// When closing (complete/cancelled), optionally include a summary.
    #[tool(
        name = "update_status",
        description = "Change a ticket's status. Valid statuses: new, next, in_progress, complete, cancelled."
    )]
    async fn update_status(
        &self,
        Parameters(request): Parameters<UpdateStatusRequest>,
    ) -> Result<String, String> {
        let ticket = Ticket::find(&request.id)
            .await
            .map_err(|e| format!("Ticket not found: {}", e))?;
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

        Ok(serde_json::to_string_pretty(&json!({
            "success": true,
            "id": ticket.id,
            "previous_status": previous_status.to_string(),
            "new_status": new_status.to_string(),
        }))
        .unwrap())
    }

    /// Add a timestamped note to a ticket.
    #[tool(
        name = "add_note",
        description = "Add a timestamped note to a ticket. Notes are appended under a '## Notes' section."
    )]
    async fn add_note(
        &self,
        Parameters(request): Parameters<AddNoteRequest>,
    ) -> Result<String, String> {
        if request.note.trim().is_empty() {
            return Err("Note content cannot be empty".to_string());
        }

        let ticket = Ticket::find(&request.id)
            .await
            .map_err(|e| format!("Ticket not found: {}", e))?;

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

        Ok(serde_json::to_string_pretty(&json!({
            "success": true,
            "id": ticket.id,
            "timestamp": timestamp,
        }))
        .unwrap())
    }

    /// List tickets with optional filters.
    #[tool(
        name = "list_tickets",
        description = "Query tickets with optional filters. Returns a list of matching tickets with their metadata."
    )]
    async fn list_tickets(
        &self,
        Parameters(request): Parameters<ListTicketsRequest>,
    ) -> Result<String, String> {
        let (tickets, ticket_map) = get_all_tickets_with_map().await;

        // Resolve spawned_from partial ID if provided
        let resolved_spawned_from = if let Some(ref partial_id) = request.spawned_from {
            let ticket = Ticket::find(partial_id)
                .await
                .map_err(|e| format!("spawned_from ticket not found: {}", e))?;
            Some(ticket.id)
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

        let result: Vec<serde_json::Value> = filtered
            .iter()
            .map(|t| {
                json!({
                    "id": t.id,
                    "title": t.title,
                    "status": t.status.map(|s| s.to_string()),
                    "type": t.ticket_type.map(|tt| tt.to_string()),
                    "priority": t.priority.map(|p| p.as_num()),
                    "deps": t.deps,
                    "spawned_from": t.spawned_from,
                    "depth": t.depth,
                })
            })
            .collect();

        Ok(serde_json::to_string_pretty(&json!({
            "count": result.len(),
            "tickets": result,
        }))
        .unwrap())
    }

    /// Show full ticket content and metadata.
    #[tool(
        name = "show_ticket",
        description = "Get full ticket content including metadata, body, dependencies, and relationships."
    )]
    async fn show_ticket(
        &self,
        Parameters(request): Parameters<ShowTicketRequest>,
    ) -> Result<String, String> {
        let ticket = Ticket::find(&request.id)
            .await
            .map_err(|e| format!("Ticket not found: {}", e))?;
        let content = ticket.read_content().map_err(|e| e.to_string())?;
        let metadata = ticket.read().map_err(|e| e.to_string())?;
        let ticket_map = build_ticket_map().await;

        // Find blockers and blocking tickets
        let mut blockers: Vec<serde_json::Value> = Vec::new();
        let mut blocking: Vec<serde_json::Value> = Vec::new();
        let mut children: Vec<serde_json::Value> = Vec::new();

        for (other_id, other) in &ticket_map {
            if other_id == &ticket.id {
                continue;
            }

            // Check if this is a child (spawned from current ticket)
            if other.spawned_from.as_ref() == Some(&ticket.id) {
                children.push(json!({
                    "id": other.id,
                    "title": other.title,
                    "status": other.status.map(|s| s.to_string()),
                }));
            }

            // Check if other ticket is blocked by current ticket
            if other.deps.contains(&ticket.id) && other.status != Some(TicketStatus::Complete) {
                blocking.push(json!({
                    "id": other.id,
                    "title": other.title,
                    "status": other.status.map(|s| s.to_string()),
                }));
            }
        }

        // Find blockers (deps that are not complete)
        for dep_id in &metadata.deps {
            if let Some(dep) = ticket_map.get(dep_id)
                && dep.status != Some(TicketStatus::Complete)
            {
                blockers.push(json!({
                    "id": dep.id,
                    "title": dep.title,
                    "status": dep.status.map(|s| s.to_string()),
                }));
            }
        }

        Ok(serde_json::to_string_pretty(&json!({
            "id": metadata.id,
            "uuid": metadata.uuid,
            "title": metadata.title,
            "status": metadata.status.map(|s| s.to_string()),
            "type": metadata.ticket_type.map(|t| t.to_string()),
            "priority": metadata.priority.map(|p| p.as_num()),
            "created": metadata.created,
            "deps": metadata.deps,
            "links": metadata.links,
            "parent": metadata.parent,
            "spawned_from": metadata.spawned_from,
            "spawn_context": metadata.spawn_context,
            "depth": metadata.depth,
            "completion_summary": metadata.completion_summary,
            "blockers": blockers,
            "blocking": blocking,
            "children": children,
            "content": content,
        }))
        .unwrap())
    }

    /// Add a dependency between tickets.
    #[tool(
        name = "add_dependency",
        description = "Add a dependency. The first ticket will depend on the second (blocking relationship)."
    )]
    async fn add_dependency(
        &self,
        Parameters(request): Parameters<AddDependencyRequest>,
    ) -> Result<String, String> {
        let ticket = Ticket::find(&request.ticket_id)
            .await
            .map_err(|e| format!("Ticket not found: {}", e))?;
        let dep_ticket = Ticket::find(&request.depends_on_id)
            .await
            .map_err(|e| format!("Dependency ticket not found: {}", e))?;

        // Check for circular dependencies
        let ticket_map = build_ticket_map().await;
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

        Ok(serde_json::to_string_pretty(&json!({
            "success": true,
            "ticket_id": ticket.id,
            "depends_on_id": dep_ticket.id,
            "action": if added { "dependency_added" } else { "dependency_already_exists" },
        }))
        .unwrap())
    }

    /// Remove a dependency between tickets.
    #[tool(
        name = "remove_dependency",
        description = "Remove a dependency from a ticket."
    )]
    async fn remove_dependency(
        &self,
        Parameters(request): Parameters<RemoveDependencyRequest>,
    ) -> Result<String, String> {
        let ticket = Ticket::find(&request.ticket_id)
            .await
            .map_err(|e| format!("Ticket not found: {}", e))?;

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

        Ok(serde_json::to_string_pretty(&json!({
            "success": true,
            "ticket_id": ticket.id,
            "removed_dependency": request.depends_on_id,
        }))
        .unwrap())
    }

    /// Add a ticket to a plan.
    #[tool(
        name = "add_ticket_to_plan",
        description = "Add a ticket to a plan. For phased plans, specify the phase."
    )]
    async fn add_ticket_to_plan(
        &self,
        Parameters(request): Parameters<AddTicketToPlanRequest>,
    ) -> Result<String, String> {
        // Validate ticket exists
        let ticket = Ticket::find(&request.ticket_id)
            .await
            .map_err(|e| format!("Ticket not found: {}", e))?;

        let plan = Plan::find(&request.plan_id)
            .await
            .map_err(|e| format!("Plan not found: {}", e))?;
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
                .ok_or_else(|| format!("Phase '{}' not found", phase_identifier))?;

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

        Ok(serde_json::to_string_pretty(&json!({
            "success": true,
            "plan_id": plan.id,
            "ticket_id": ticket.id,
            "phase": added_to_phase,
        }))
        .unwrap())
    }

    /// Get plan status and progress.
    #[tool(
        name = "get_plan_status",
        description = "Get plan status including progress percentage and phase breakdown."
    )]
    async fn get_plan_status(
        &self,
        Parameters(request): Parameters<GetPlanStatusRequest>,
    ) -> Result<String, String> {
        let plan = Plan::find(&request.plan_id)
            .await
            .map_err(|e| format!("Plan not found: {}", e))?;
        let metadata = plan.read().map_err(|e| e.to_string())?;
        let ticket_map = build_ticket_map().await;

        let plan_status = compute_plan_status(&metadata, &ticket_map);

        let phases_json: Vec<serde_json::Value> = if metadata.is_phased() {
            compute_all_phase_statuses(&metadata, &ticket_map)
                .iter()
                .map(|ps| {
                    json!({
                        "number": ps.phase_number,
                        "name": ps.phase_name,
                        "status": ps.status.to_string(),
                        "completed_count": ps.completed_count,
                        "total_count": ps.total_count,
                    })
                })
                .collect()
        } else {
            vec![]
        };

        Ok(serde_json::to_string_pretty(&json!({
            "plan_id": plan.id,
            "title": metadata.title,
            "status": plan_status.status.to_string(),
            "completed_count": plan_status.completed_count,
            "total_count": plan_status.total_count,
            "progress_percent": plan_status.progress_percent(),
            "progress_string": plan_status.progress_string(),
            "phases": phases_json,
        }))
        .unwrap())
    }

    /// Get tickets spawned from a parent ticket.
    #[tool(
        name = "get_children",
        description = "Get all tickets that were spawned from a given parent ticket."
    )]
    async fn get_children(
        &self,
        Parameters(request): Parameters<GetChildrenRequest>,
    ) -> Result<String, String> {
        let parent = Ticket::find(&request.ticket_id)
            .await
            .map_err(|e| format!("Ticket not found: {}", e))?;

        let (tickets, _) = get_all_tickets_with_map().await;

        let children: Vec<serde_json::Value> = tickets
            .iter()
            .filter(|t| t.spawned_from.as_ref() == Some(&parent.id))
            .map(|t| {
                json!({
                    "id": t.id,
                    "title": t.title,
                    "status": t.status.map(|s| s.to_string()),
                    "type": t.ticket_type.map(|tt| tt.to_string()),
                    "priority": t.priority.map(|p| p.as_num()),
                    "depth": t.depth,
                    "spawn_context": t.spawn_context,
                })
            })
            .collect();

        Ok(serde_json::to_string_pretty(&json!({
            "parent_id": parent.id,
            "count": children.len(),
            "children": children,
        }))
        .unwrap())
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
        format!("{}\n\n## Completion Summary\n\n{}\n", trimmed, summary)
    };

    ticket.write(&new_content)
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
            "Circular dependency: {} already depends on {}",
            to_id, from_id
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
            "Circular dependency: adding {} -> {} would create a cycle",
            from_id, to_id
        ));
    }

    Ok(())
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
}
