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
//! | `semantic_search` | Find tickets semantically similar to a query (requires semantic-search config) |
//! | `doc_list` | List all project knowledge documents |
//! | `doc_show` | Show a document's content (optionally a line range) |
//! | `doc_set` | Create a new project knowledge document |
//! | `doc_search` | Search documents semantically for relevant content |

use rmcp::handler::server::{tool::ToolRouter, wrapper::Parameters};
use rmcp::model::ToolAnnotations;
use tracing::warn;

use std::str::FromStr;
use tokio::time::timeout;

use crate::config::Config;
use crate::doc::{Doc, DocMetadata, get_all_docs_from_disk};
use crate::embedding::model::EMBEDDING_TIMEOUT;
use crate::events::Actor;
use crate::graph::check_circular_dependency;
use crate::next::NextWorkFinder;
use crate::plan::parser::serialize_plan;
use crate::plan::{Plan, compute_plan_status};
use crate::status::is_dependency_satisfied;
use crate::store::get_or_init_store;
use crate::ticket::{
    ArrayField, Ticket, TicketBuilder, build_ticket_map, get_all_tickets_with_map,
};
use crate::types::{TicketMetadata, TicketPriority, TicketSize, TicketStatus, TicketType};
use crate::utils::iso_date;

use super::format::{
    build_filter_summary, format_children_as_markdown, format_next_work_as_markdown,
    format_plan_status_as_markdown, format_ticket_as_markdown, format_ticket_list_as_markdown,
};
use super::requests::{
    AddDependencyRequest, AddNoteRequest, AddTicketToPlanRequest, CreateTicketRequest,
    DocListRequest, DocSearchRequest, DocSetRequest, DocShowRequest, GetChildrenRequest,
    GetNextAvailableTicketRequest, GetPlanStatusRequest, ListTicketsRequest,
    RemoveDependencyRequest, SemanticSearchRequest, ShowTicketRequest, SpawnSubtaskRequest,
    UpdateStatusRequest,
};

/// Helper to create ToolAnnotations with all fields set
fn tool_annotations(
    read_only: bool,
    destructive: bool,
    idempotent: bool,
    open_world: bool,
) -> ToolAnnotations {
    ToolAnnotations::new()
        .read_only(read_only)
        .destructive(destructive)
        .idempotent(idempotent)
        .open_world(open_world)
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

/// Macro to register a tool with MCP.
/// Generates the ToolRoute boilerplate: extract args, deserialize, call impl, wrap result.
///
/// # Parameters
/// - `$router`: The ToolRouter to add the route to
/// - `$name`: Tool name string
/// - `$desc`: Tool description string
/// - `$req_type`: The request type for deserialization
/// - `$method`: The method to call on `self` that implements the tool logic
/// - `$optional`: `true` if arguments are optional (uses `unwrap_or_default`),
///   `false` if required (errors on missing args)
/// - `$annotations`: ToolAnnotations for the tool (optional, for backward compatibility)
macro_rules! register_tool {
    // Full signature with annotations
    ($router:expr, $name:expr, $desc:expr, $req_type:ty, $method:ident, $optional:expr, $annotations:expr) => {{
        use rmcp::handler::server::tool::ToolRoute;
        use rmcp::model::Tool;
        use rmcp::schemars::schema_for;
        use std::sync::Arc;

        let schema_value = serde_json::to_value(schema_for!($req_type))
            .unwrap_or_else(|e| panic!("Failed to serialize schema for tool '{}': {e}", $name));
        let schema_obj = match schema_value {
            serde_json::Value::Object(obj) => obj,
            _ => panic!(
                "Schema for tool '{}' is not an object (this is a bug)",
                $name
            ),
        };
        let tool = Tool::new($name.to_string(), $desc.to_string(), Arc::new(schema_obj))
            .annotate($annotations);
        let route =
            ToolRoute::new_dyn(
                tool,
                |ctx: rmcp::handler::server::tool::ToolCallContext<'_, JanusTools>| {
                    Box::pin(async move {
                        let this = ctx.service;
                        let args = if $optional {
                            ctx.arguments.unwrap_or_default()
                        } else {
                            ctx.arguments.ok_or(rmcp::model::ErrorData {
                                code: rmcp::model::ErrorCode::INVALID_PARAMS,
                                message: std::borrow::Cow::Borrowed("Missing arguments"),
                                data: None,
                            })?
                        };
                        let request: $req_type = serde_json::from_value(serde_json::Value::Object(
                            args,
                        ))
                        .map_err(|e| rmcp::model::ErrorData {
                            code: rmcp::model::ErrorCode::INVALID_PARAMS,
                            message: std::borrow::Cow::Owned(format!("Invalid parameters: {e}")),
                            data: None,
                        })?;
                        match this.$method(Parameters(request)).await {
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
        $router.add_route(route);
    }};
    // Legacy signature without annotations (backward compatibility)
    ($router:expr, $name:expr, $desc:expr, $req_type:ty, $method:ident, $optional:expr) => {{
        use rmcp::handler::server::tool::ToolRoute;
        use rmcp::model::Tool;
        use rmcp::schemars::schema_for;
        use std::sync::Arc;

        let schema_value = serde_json::to_value(schema_for!($req_type))
            .unwrap_or_else(|e| panic!("Failed to serialize schema for tool '{}': {e}", $name));
        let schema_obj = match schema_value {
            serde_json::Value::Object(obj) => obj,
            _ => panic!(
                "Schema for tool '{}' is not an object (this is a bug)",
                $name
            ),
        };
        let tool = Tool::new($name.to_string(), $desc.to_string(), Arc::new(schema_obj));
        let route =
            ToolRoute::new_dyn(
                tool,
                |ctx: rmcp::handler::server::tool::ToolCallContext<'_, JanusTools>| {
                    Box::pin(async move {
                        let this = ctx.service;
                        let args = if $optional {
                            ctx.arguments.unwrap_or_default()
                        } else {
                            ctx.arguments.ok_or(rmcp::model::ErrorData {
                                code: rmcp::model::ErrorCode::INVALID_PARAMS,
                                message: std::borrow::Cow::Borrowed("Missing arguments"),
                                data: None,
                            })?
                        };
                        let request: $req_type = serde_json::from_value(serde_json::Value::Object(
                            args,
                        ))
                        .map_err(|e| rmcp::model::ErrorData {
                            code: rmcp::model::ErrorCode::INVALID_PARAMS,
                            message: std::borrow::Cow::Owned(format!("Invalid parameters: {e}")),
                            data: None,
                        })?;
                        match this.$method(Parameters(request)).await {
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
        $router.add_route(route);
    }};
}

impl JanusTools {
    /// Create a new JanusTools instance with all tools registered
    pub fn new() -> Self {
        let mut router = ToolRouter::new();

        register_tool!(
            router,
            "create_ticket",
            "Create a new ticket. Returns the ticket ID and file path.",
            CreateTicketRequest,
            create_ticket_impl,
            false,
            tool_annotations(false, false, false, false)
        );

        register_tool!(
            router,
            "spawn_subtask",
            "Create a new ticket as a child of an existing ticket. Sets spawning metadata for decomposition tracking.",
            SpawnSubtaskRequest,
            spawn_subtask_impl,
            false,
            tool_annotations(false, false, false, false)
        );

        register_tool!(
            router,
            "update_status",
            "Change a ticket's status. Valid statuses: new, next, in_progress, complete, cancelled.",
            UpdateStatusRequest,
            update_status_impl,
            false,
            tool_annotations(false, false, true, false)
        );

        register_tool!(
            router,
            "add_note",
            "Add a timestamped note to a ticket. Notes are appended under a '## Notes' section.",
            AddNoteRequest,
            add_note_impl,
            false,
            tool_annotations(false, false, false, false)
        );

        register_tool!(
            router,
            "list_tickets",
            "Query tickets with optional filters. Returns a list of matching tickets with their metadata. By default, only open tickets are returned (Complete and Cancelled tickets are excluded). To include closed tickets, specify an explicit status filter.",
            ListTicketsRequest,
            list_tickets_impl,
            true,
            tool_annotations(true, false, true, false)
        );

        register_tool!(
            router,
            "show_ticket",
            "Get full ticket content including metadata, body, dependencies, and relationships. Returns markdown optimized for LLM consumption.",
            ShowTicketRequest,
            show_ticket_impl,
            false,
            tool_annotations(true, false, true, false)
        );

        register_tool!(
            router,
            "add_dependency",
            "Add a dependency. The first ticket will depend on the second (blocking relationship).",
            AddDependencyRequest,
            add_dependency_impl,
            false,
            tool_annotations(false, false, true, false)
        );

        register_tool!(
            router,
            "remove_dependency",
            "Remove a dependency from a ticket.",
            RemoveDependencyRequest,
            remove_dependency_impl,
            false,
            tool_annotations(false, true, true, false)
        );

        register_tool!(
            router,
            "add_ticket_to_plan",
            "Add a ticket to a plan. For phased plans, specify the phase.",
            AddTicketToPlanRequest,
            add_ticket_to_plan_impl,
            false,
            tool_annotations(false, false, true, false)
        );

        register_tool!(
            router,
            "get_plan_status",
            "Get plan status including progress percentage and phase breakdown. Returns markdown optimized for LLM consumption.",
            GetPlanStatusRequest,
            get_plan_status_impl,
            false,
            tool_annotations(true, false, true, false)
        );

        register_tool!(
            router,
            "get_children",
            "Get all tickets that were spawned from a given parent ticket. Returns markdown optimized for LLM consumption.",
            GetChildrenRequest,
            get_children_impl,
            false,
            tool_annotations(true, false, true, false)
        );

        register_tool!(
            router,
            "get_next_available_ticket",
            "Query the Janus ticket backlog for the next ticket(s) to work on, based on priority and dependency resolution. Returns tickets in optimal order (dependencies before dependents). Use this if you've been instructed to work on tickets on the backlog. Do NOT use this for guidance on your current task.",
            GetNextAvailableTicketRequest,
            get_next_available_ticket_impl,
            true,
            tool_annotations(true, false, true, false)
        );

        register_tool!(
            router,
            "semantic_search",
            "Find tickets semantically similar to a natural language query. Uses vector embeddings for fuzzy matching by intent rather than exact keywords.",
            SemanticSearchRequest,
            semantic_search_impl,
            false,
            tool_annotations(true, false, true, false)
        );

        register_tool!(
            router,
            "doc_list",
            "List all project knowledge documents with their metadata (label, title, description, tags).",
            DocListRequest,
            doc_list_impl,
            true,
            tool_annotations(true, false, true, false)
        );

        register_tool!(
            router,
            "doc_show",
            "Show a document's full content or a specific line range. Returns markdown with heading context.",
            DocShowRequest,
            doc_show_impl,
            false,
            tool_annotations(true, false, true, false)
        );

        register_tool!(
            router,
            "doc_set",
            "Create a new project knowledge document. Returns error if document already exists (no overwrite).",
            DocSetRequest,
            doc_set_impl,
            false,
            tool_annotations(false, false, false, false)
        );

        register_tool!(
            router,
            "doc_search",
            "Search project knowledge documents semantically. Returns chunks with heading paths and line numbers.",
            DocSearchRequest,
            doc_search_impl,
            false,
            tool_annotations(true, false, true, false)
        );

        Self {
            tool_router: router,
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
    /// Implementation for create_ticket tool
    async fn create_ticket_impl(
        &self,
        Parameters(request): Parameters<CreateTicketRequest>,
    ) -> Result<String, String> {
        // Validate input
        request.validate()?;

        let mut builder = TicketBuilder::new(&request.title)
            .description(request.description.as_deref())
            .run_hooks(true);

        if let Some(ref t) = request.ticket_type {
            let tt = TicketType::from_str(t).map_err(|_| format!("Invalid ticket type: {t}"))?;
            builder = builder.ticket_type(tt);
        }

        if let Some(p) = request.priority {
            let pp = TicketPriority::from_str(&p.to_string())
                .map_err(|_| format!("Priority must be 0-4, got {p}"))?;
            builder = builder.priority(pp);
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

        // Refresh the in-memory store immediately
        if let Ok(store) = get_or_init_store().await {
            store.refresh_ticket_in_store(&id).await;
        } else {
            warn!(
                "Failed to refresh ticket {} in store - store initialization failed",
                &id
            );
        }

        // Log the event with MCP actor
        let ticket_type = request.ticket_type.as_deref().unwrap_or("task");
        let priority = request.priority.unwrap_or(2);
        let _size_str = size.map(|s| s.to_string());
        crate::events::log_ticket_created(
            &id,
            &request.title,
            ticket_type,
            priority,
            None,
            Some(Actor::Mcp),
        );

        Ok(format!("Created ticket **{}**: \"{}\"", id, request.title))
    }

    /// Spawn a subtask from a parent ticket.
    /// Sets spawned_from, spawn_context, and depth fields.
    async fn spawn_subtask_impl(
        &self,
        Parameters(request): Parameters<SpawnSubtaskRequest>,
    ) -> Result<String, String> {
        // Validate input
        request.validate()?;

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

        // Refresh the in-memory store immediately (child and parent)
        if let Ok(store) = get_or_init_store().await {
            store.refresh_ticket_in_store(&id).await;
            store.refresh_ticket_in_store(&parent.id).await;
        } else {
            warn!(
                "Failed to refresh tickets {} and {} in store - store initialization failed",
                &id, &parent.id
            );
        }

        // Log with MCP actor
        crate::events::log_ticket_created(
            &id,
            &request.title,
            "task",
            2,
            Some(&parent.id),
            Some(Actor::Mcp),
        );

        Ok(format!(
            "Spawned subtask **{}**: \"{}\" from parent {} (depth: {})",
            id, request.title, parent.id, new_depth
        ))
    }

    /// Update a ticket's status.
    /// When closing (complete/cancelled), optionally include a summary.
    async fn update_status_impl(
        &self,
        Parameters(request): Parameters<UpdateStatusRequest>,
    ) -> Result<String, String> {
        // Validate input
        request.validate()?;

        let ticket = Ticket::find(&request.id)
            .await
            .map_err(|e| format!("Ticket not found: {e}"))?;

        // Validate and parse status
        let new_status = TicketStatus::from_str(&request.status).map_err(|_| {
            format!(
                "Invalid status '{}'. Must be one of: new, next, in_progress, complete, cancelled",
                request.status
            )
        })?;

        // Use the domain method with Actor::Mcp to log the event correctly
        ticket
            .update_status_with_actor(new_status, request.summary.as_deref(), Some(Actor::Mcp))
            .map_err(|e| e.to_string())?;

        // Refresh the in-memory store immediately
        if let Ok(store) = get_or_init_store().await {
            store.refresh_ticket_in_store(&ticket.id).await;
        } else {
            warn!(
                "Failed to refresh ticket {} in store - store initialization failed",
                &ticket.id
            );
        }

        Ok(format!(
            "Updated **{}** status to {}",
            ticket.id, new_status
        ))
    }

    /// Add a timestamped note to a ticket.
    async fn add_note_impl(
        &self,
        Parameters(request): Parameters<AddNoteRequest>,
    ) -> Result<String, String> {
        // Validate input
        request.validate()?;

        let ticket = Ticket::find(&request.id)
            .await
            .map_err(|e| format!("Ticket not found: {e}"))?;

        // Use the shared add_note method on Ticket with Actor::Mcp
        ticket
            .add_note_with_actor(&request.note, Some(Actor::Mcp))
            .map_err(|e| e.to_string())?;

        // Refresh the in-memory store immediately
        if let Ok(store) = get_or_init_store().await {
            store.refresh_ticket_in_store(&ticket.id).await;
        } else {
            warn!(
                "Failed to refresh ticket {} in store - store initialization failed",
                &ticket.id
            );
        }

        let timestamp = iso_date();
        Ok(format!("Added note to **{}** at {}", ticket.id, timestamp))
    }

    /// List tickets with optional filters.
    async fn list_tickets_impl(
        &self,
        Parameters(request): Parameters<ListTicketsRequest>,
    ) -> Result<String, String> {
        use crate::query::{
            BlockedFilter, ReadyFilter, SizeFilter, SpawningFilter, StatusFilter,
            TicketQueryBuilder, TypeFilter,
        };

        request.validate()?;

        let (tickets, _ticket_map) = get_all_tickets_with_map()
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

        // Build the query using TicketQueryBuilder
        let mut query_builder = TicketQueryBuilder::new();

        // Add spawned_from filter
        if let Some(ref parent_id) = resolved_spawned_from {
            query_builder = query_builder.with_filter(Box::new(SpawningFilter::new(
                Some(parent_id),
                None,
                None,
            )));
        }

        // Add depth filter
        if let Some(target_depth) = request.depth {
            query_builder = query_builder.with_filter(Box::new(SpawningFilter::new(
                None,
                Some(target_depth),
                None,
            )));
        }

        // Add status filter
        if let Some(ref status_filter) = request.status {
            let parsed_status = TicketStatus::from_str(status_filter).map_err(|_| {
                format!(
                    "Invalid status '{}'. Must be one of: {}",
                    status_filter,
                    crate::types::TicketStatus::ALL_STRINGS.join(", ")
                )
            })?;
            query_builder = query_builder.with_filter(Box::new(StatusFilter::new(parsed_status)));
        }

        // Add type filter
        if let Some(ref type_filter) = request.ticket_type {
            let parsed_type = TicketType::from_str(type_filter).map_err(|_| {
                format!(
                    "Invalid type '{}'. Must be one of: {}",
                    type_filter,
                    TicketType::ALL_STRINGS.join(", ")
                )
            })?;
            query_builder = query_builder.with_filter(Box::new(TypeFilter::new(parsed_type)));
        }

        // Add size filter
        if let Some(ref sizes) = size_filter {
            query_builder = query_builder.with_filter(Box::new(SizeFilter::new(sizes.clone())));
        }

        // Add ready filter
        if request.ready == Some(true) {
            query_builder = query_builder.with_filter(Box::new(ReadyFilter));
        }

        // Add blocked filter
        if request.blocked == Some(true) {
            query_builder = query_builder.with_filter(Box::new(BlockedFilter));
        }

        // Execute the query
        let mut filtered_tickets = query_builder
            .execute(tickets)
            .await
            .map_err(|e| format!("query execution failed: {e}"))?;

        // Exclude closed tickets by default (unless filtering by status)
        if request.status.is_none() {
            filtered_tickets.retain(|t| {
                !matches!(
                    t.status,
                    Some(TicketStatus::Complete) | Some(TicketStatus::Cancelled)
                )
            });
        }

        // Convert to references for the formatter
        let filtered_refs: Vec<&TicketMetadata> = filtered_tickets.iter().collect();

        // Build filter summary
        let filter_summary = build_filter_summary(
            request.ready,
            request.blocked,
            request.status.as_deref(),
            request.ticket_type.as_deref(),
            request.spawned_from.as_deref(),
            request.depth,
            request.size.as_deref(),
        );

        // Format as markdown
        Ok(format_ticket_list_as_markdown(
            &filtered_refs,
            &filter_summary,
        ))
    }

    /// Show full ticket content and metadata.
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
            if other.spawned_from.as_deref() == Some(ticket.id.as_str()) {
                children.push(other);
            }

            // Check if other ticket is blocked by current ticket
            // (other depends on us, and we are not yet terminal)
            let ticket_id = crate::types::TicketId::new_unchecked(&ticket.id);
            if other.deps.contains(&ticket_id) && !metadata.status.is_some_and(|s| s.is_terminal())
            {
                blocking.push(other);
            }
        }

        // Find blockers (deps that are not satisfied per canonical definition)
        for dep_id in &metadata.deps {
            if !is_dependency_satisfied(dep_id.as_ref(), &ticket_map) {
                if let Some(dep) = ticket_map.get(dep_id.as_ref()) {
                    blockers.push(dep);
                }
            }
        }

        Ok(format_ticket_as_markdown(
            &metadata, &content, &blockers, &blocking, &children,
        ))
    }

    /// Add a dependency between tickets.
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
        check_circular_dependency(&ticket.id, &dep_ticket.id, &ticket_map)
            .map_err(|e| e.to_string())?;

        // Use the method with Actor::Mcp to log the event correctly
        let added = ticket
            .add_to_array_field_with_actor(ArrayField::Deps, &dep_ticket.id, Some(Actor::Mcp))
            .map_err(|e| e.to_string())?;

        if added {
            // Refresh the in-memory store immediately
            if let Ok(store) = get_or_init_store().await {
                store.refresh_ticket_in_store(&ticket.id).await;
            } else {
                warn!(
                    "Failed to refresh ticket {} in store - store initialization failed",
                    &ticket.id
                );
            }
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
    async fn remove_dependency_impl(
        &self,
        Parameters(request): Parameters<RemoveDependencyRequest>,
    ) -> Result<String, String> {
        let ticket = Ticket::find(&request.ticket_id)
            .await
            .map_err(|e| format!("Ticket not found: {e}"))?;

        // Use the method with Actor::Mcp to log the event correctly
        let removed = ticket
            .remove_from_array_field_with_actor(
                ArrayField::Deps,
                &request.depends_on_id,
                Some(Actor::Mcp),
            )
            .map_err(|e| e.to_string())?;

        if !removed {
            return Err(format!(
                "Dependency '{}' not found in ticket",
                request.depends_on_id
            ));
        }

        // Refresh the in-memory store immediately
        if let Ok(store) = get_or_init_store().await {
            store.refresh_ticket_in_store(&ticket.id).await;
        } else {
            warn!(
                "Failed to refresh ticket {} in store - store initialization failed",
                &ticket.id
            );
        }

        Ok(format!(
            "Removed dependency: **{}** no longer depends on **{}**",
            ticket.id, request.depends_on_id
        ))
    }

    /// Add a ticket to a plan.
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

            let ts = metadata
                .tickets_section_mut()
                .ok_or("Plan has no tickets section")?;
            ts.add_ticket(ticket.id.clone());
        } else {
            return Err("Plan has no tickets section or phases".to_string());
        }

        // Write updated plan
        let content = serialize_plan(&metadata).map_err(|e| e.to_string())?;
        plan.write(&content).map_err(|e| e.to_string())?;

        // Refresh the in-memory store immediately
        if let Ok(store) = get_or_init_store().await {
            store.refresh_plan_in_store(&plan.id).await;
        } else {
            warn!(
                "Failed to refresh plan {} in store - store initialization failed",
                &plan.id
            );
        }

        // Log with MCP actor using the helper function
        crate::events::log_ticket_added_to_plan(
            &plan.id,
            &ticket.id,
            added_to_phase.as_deref(),
            Some(Actor::Mcp),
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
            .filter(|t| t.spawned_from.as_deref() == Some(parent.id.as_str()))
            .collect();

        let parent_title = parent_metadata.title.as_deref().unwrap_or("Untitled");
        Ok(format_children_as_markdown(
            &parent.id,
            parent_title,
            &children,
        ))
    }

    /// Query the Janus ticket backlog for the next ticket(s) to work on.
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
        let work_items = finder.get_next_work(limit);

        if work_items.is_empty() {
            return Ok("No tickets ready to work on.".to_string());
        }

        Ok(format_next_work_as_markdown(&work_items, &ticket_map))
    }

    /// Find tickets semantically similar to a natural language query.
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
                    return Err("Semantic search is disabled. Enable with: janus config set semantic_search.enabled true".to_string());
                }
            }
            Err(e) => {
                eprintln!("Warning: failed to load config: {e}. Proceeding with semantic search.");
            }
        }

        // Get store
        let store = get_or_init_store()
            .await
            .map_err(|e| format!("Failed to initialize store: {e}"))?;

        // Check if embeddings available
        let (with_embedding, total) = store.embedding_coverage();

        if total == 0 {
            return Err("No tickets found.".to_string());
        }

        if with_embedding == 0 {
            return Err("No ticket embeddings available. Run 'janus cache rebuild' to generate embeddings for all tickets.".to_string());
        }

        // Set defaults
        let limit = request.limit.unwrap_or(10);
        let threshold = request.threshold.unwrap_or(0.0);

        // Generate query embedding with timeout and perform search
        let query_embedding = match timeout(
            EMBEDDING_TIMEOUT,
            crate::embedding::model::generate_embedding(&request.query),
        )
        .await
        {
            Ok(Ok(embedding)) => embedding,
            Ok(Err(e)) => return Err(format!("Failed to generate query embedding: {e}")),
            Err(_) => {
                return Err(format!(
                    "Embedding generation timed out after {} seconds. The embedding service may be unresponsive.",
                    EMBEDDING_TIMEOUT.as_secs()
                ));
            }
        };
        let results = store.semantic_search(&query_embedding, limit);

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

        Ok(output)
    }

    /// List all project knowledge documents.
    async fn doc_list_impl(&self, _request: Parameters<DocListRequest>) -> Result<String, String> {
        let docs = get_all_docs_from_disk();
        let doc_list = docs.into_docs();

        if doc_list.is_empty() {
            return Ok("No documents found.".to_string());
        }

        let mut output = String::from("# Project Knowledge Documents\n\n");
        output.push_str(&format!("**Total:** {} document(s)\n\n", doc_list.len()));
        output.push_str("| Label | Title | Description | Tags |\n");
        output.push_str("|-------|-------|-------------|------|\n");

        for doc in doc_list {
            let label = doc.label().unwrap_or("(no label)");
            let title = doc.title().unwrap_or("(no title)");
            let description = doc.description.as_deref().unwrap_or("");
            let tags = if doc.tags.is_empty() {
                "-".to_string()
            } else {
                doc.tags.join(", ")
            };

            let desc_display = if description.len() > 40 {
                format!("{}...", &description[..40])
            } else {
                description.to_string()
            };

            output.push_str(&format!(
                "| {label} | {title} | {desc_display} | {tags} |\n"
            ));
        }

        Ok(output)
    }

    /// Show a document's content, optionally with line range.
    async fn doc_show_impl(
        &self,
        Parameters(request): Parameters<DocShowRequest>,
    ) -> Result<String, String> {
        let doc = Doc::find(&request.label)
            .await
            .map_err(|e| format!("Document not found: {e}"))?;
        let content = doc.read_content().map_err(|e| e.to_string())?;
        let metadata = doc.read().map_err(|e| e.to_string())?;

        let start_line = request.start_line.unwrap_or(1);
        let end_line = request.end_line;

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        if start_line == 0 || start_line > total_lines {
            return Err(format!(
                "Invalid start_line: {start_line}. Document has {total_lines} lines."
            ));
        }

        let actual_end_line = end_line.map(|e| e.min(total_lines)).unwrap_or(total_lines);

        if actual_end_line < start_line {
            return Err(format!(
                "Invalid line range: end_line ({actual_end_line}) must be >= start_line ({start_line})"
            ));
        }

        let mut output = String::new();
        let title = metadata.title().unwrap_or(&doc.label);
        output.push_str(&format!("# Document: {title}\n\n"));
        output.push_str(&format!("**Label:** {}\n", doc.label));
        if let Some(ref desc) = metadata.description {
            output.push_str(&format!("**Description:** {desc}\n"));
        }
        if !metadata.tags.is_empty() {
            output.push_str(&format!("**Tags:** {}\n", metadata.tags.join(", ")));
        }
        output.push_str(&format!(
            "**Lines:** {start_line}-{actual_end_line}/{total_lines}\n\n"
        ));

        let selected_lines = &lines[start_line - 1..actual_end_line];
        output.push_str("```markdown\n");
        for line in selected_lines {
            output.push_str(line);
            output.push('\n');
        }
        output.push_str("```");

        Ok(output)
    }

    /// Create a new project knowledge document.
    async fn doc_set_impl(
        &self,
        Parameters(request): Parameters<DocSetRequest>,
    ) -> Result<String, String> {
        request.validate()?;

        let doc = Doc::with_label(&request.label).map_err(|e| e.to_string())?;
        if doc.exists() {
            return Err(format!(
                "Document '{}' already exists. Use doc_edit to modify existing documents.",
                request.label
            ));
        }

        let mut metadata = DocMetadata {
            label: Some(crate::doc::DocLabel::new(&request.label).map_err(|e| e.to_string())?),
            description: request.description,
            tags: request.tags.unwrap_or_default(),
            ..Default::default()
        };

        let lines: Vec<&str> = request.content.lines().collect();
        for line in &lines {
            let trimmed = line.trim();
            if let Some(title_text) = trimmed.strip_prefix("# ") {
                metadata.title = Some(title_text.to_string());
                break;
            }
        }

        let content = crate::doc::serialize_doc(&metadata).map_err(|e| e.to_string())?;
        let full_content = format!("{}\n{}", content, request.content);
        doc.write(&full_content).map_err(|e| e.to_string())?;

        crate::events::log_doc_created(
            &request.label,
            metadata.title.as_deref().unwrap_or("Untitled"),
            Some(Actor::Mcp),
        );

        Ok(format!("Created document **{}**", request.label))
    }

    /// Search documents semantically.
    async fn doc_search_impl(
        &self,
        Parameters(request): Parameters<DocSearchRequest>,
    ) -> Result<String, String> {
        request.validate()?;

        match Config::load() {
            Ok(config) => {
                if !config.semantic_search_enabled() {
                    return Err("Semantic search is disabled. Enable with: janus config set semantic_search.enabled true".to_string());
                }
            }
            Err(e) => {
                eprintln!("Warning: failed to load config: {e}. Proceeding with semantic search.");
            }
        }

        let store = get_or_init_store()
            .await
            .map_err(|e| format!("Failed to initialize store: {e}"))?;

        // Resolve document label if specified
        let resolved_label = if let Some(ref doc_label) = request.document {
            let docs: Vec<_> = store
                .docs()
                .iter()
                .filter(|entry| {
                    let label = entry.key();
                    label == doc_label || label.starts_with(doc_label)
                })
                .map(|entry| entry.key().clone())
                .collect();

            match docs.as_slice() {
                [] => {
                    return Err(format!("No document found matching '{doc_label}'"));
                }
                [single] => Some(single.clone()),
                multiple => {
                    return Err(format!(
                        "Ambiguous document label '{}'. Matches: {}",
                        doc_label,
                        multiple.join(", ")
                    ));
                }
            }
        } else {
            None
        };

        let doc_embeddings: Vec<_> = store
            .embeddings()
            .iter()
            .filter(|entry| entry.key().starts_with("doc:"))
            .collect();

        if doc_embeddings.is_empty() {
            return Err("No document embeddings available. Run 'janus cache rebuild' to generate embeddings for all documents.".to_string());
        }

        let query_embedding = match timeout(
            EMBEDDING_TIMEOUT,
            crate::embedding::model::generate_embedding(&request.query),
        )
        .await
        {
            Ok(Ok(embedding)) => embedding,
            Ok(Err(e)) => return Err(format!("Failed to generate query embedding: {e}")),
            Err(_) => {
                return Err(format!(
                    "Embedding generation timed out after {} seconds.",
                    EMBEDDING_TIMEOUT.as_secs()
                ));
            }
        };

        let limit = request.limit.unwrap_or(10);
        let threshold = request.threshold.unwrap_or(0.0);

        let results = match &resolved_label {
            Some(label) => store.doc_search_by_document(&query_embedding, label, limit),
            None => store.doc_search(&query_embedding, limit),
        };

        let results: Vec<_> = results
            .into_iter()
            .filter(|r| r.similarity >= threshold)
            .collect();

        if results.is_empty() {
            let target_info = if let Some(ref label) = resolved_label {
                format!(" in document '{label}'")
            } else {
                String::new()
            };
            return Ok(format!(
                "No documents found matching the query{target_info}."
            ));
        }

        let target_info = if let Some(ref label) = resolved_label {
            format!(" in document '{label}'")
        } else {
            String::new()
        };
        let mut output = format!(
            "Found {} document chunk(s) semantically similar to: {}{target_info}\n\n",
            results.len(),
            request.query,
        );

        for (i, result) in results.iter().enumerate() {
            let title = result.doc.title().unwrap_or("(no title)");
            let heading_path = if result.heading_path.is_empty() {
                "(document)".to_string()
            } else {
                result.heading_path.join(" > ")
            };

            output.push_str(&format!(
                "## {}. {} (similarity: {:.2})\n\n",
                i + 1,
                title,
                result.similarity
            ));
            output.push_str(&format!("**Label:** `{}`\n", result.label));
            output.push_str(&format!(
                "**Location:** {} (lines {}-{})\n\n",
                heading_path, result.line_range.0, result.line_range.1
            ));

            let snippet = if result.content_snippet.len() > 500 {
                format!("{}...", &result.content_snippet[..500])
            } else {
                result.content_snippet.clone()
            };
            output.push_str("```\n");
            output.push_str(&snippet);
            output.push_str("\n```\n\n");
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::types::{PlanId, TicketId};

    #[test]
    fn test_circular_dependency_direct() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "a".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("a")),
                deps: vec![TicketId::new_unchecked("b")],
                ..Default::default()
            },
        );
        ticket_map.insert(
            "b".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("b")),
                deps: vec![],
                ..Default::default()
            },
        );

        // b -> a should fail because a already depends on b
        let result = check_circular_dependency("b", "a", &ticket_map);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("circular dependency")
        );
    }

    #[test]
    fn test_circular_dependency_transitive() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "a".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("a")),
                deps: vec![TicketId::new_unchecked("b")],
                ..Default::default()
            },
        );
        ticket_map.insert(
            "b".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("b")),
                deps: vec![TicketId::new_unchecked("c")],
                ..Default::default()
            },
        );
        ticket_map.insert(
            "c".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("c")),
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
                id: Some(TicketId::new_unchecked("a")),
                deps: vec![],
                ..Default::default()
            },
        );
        ticket_map.insert(
            "b".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("b")),
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
        let summary = build_filter_summary(None, None, None, None, None, None, None);
        assert!(summary.is_empty());
    }

    #[test]
    fn test_build_filter_summary_ready() {
        let summary = build_filter_summary(Some(true), None, None, None, None, None, None);
        assert_eq!(summary, "**Showing:** ready tickets\n\n");
    }

    #[test]
    fn test_build_filter_summary_multiple() {
        let summary = build_filter_summary(None, None, Some("new"), Some("bug"), None, None, None);
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
            id: Some(TicketId::new_unchecked("j-a1b2")),
            title: Some("Add authentication".to_string()),
            status: Some(TicketStatus::New),
            ticket_type: Some(TicketType::Feature),
            priority: Some(TicketPriority::P1),
            ..Default::default()
        };
        let ticket2 = TicketMetadata {
            id: Some(TicketId::new_unchecked("j-c3d4")),
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
                id: Some(TicketId::new_unchecked("j-a1b2")),
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
                id: Some(TicketId::new_unchecked("j-c3d4")),
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
                id: Some(TicketId::new_unchecked("j-e5f6")),
                title: Some("Implement logout".to_string()),
                status: Some(TicketStatus::New),
                ticket_type: Some(TicketType::Task),
                priority: Some(TicketPriority::P2),
                ..Default::default()
            },
        );

        let metadata = PlanMetadata {
            id: Some(PlanId::new_unchecked("plan-a1b2")),
            title: Some("Implement Authentication".to_string()),
            sections: vec![PlanSection::Tickets(
                crate::plan::types::TicketsSection::new(vec![
                    "j-a1b2".to_string(),
                    "j-c3d4".to_string(),
                    "j-e5f6".to_string(),
                ]),
            )],
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
                id: Some(TicketId::new_unchecked("j-a1b2")),
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
                id: Some(TicketId::new_unchecked("j-c3d4")),
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
                id: Some(TicketId::new_unchecked("j-e5f6")),
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
                id: Some(TicketId::new_unchecked("j-g7h8")),
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
                id: Some(TicketId::new_unchecked("j-i9j0")),
                title: Some("Implement logout".to_string()),
                status: Some(TicketStatus::New),
                ticket_type: Some(TicketType::Task),
                priority: Some(TicketPriority::P2),
                ..Default::default()
            },
        );

        let mut phase1 = Phase::new("1", "Setup");
        phase1.ticket_list.tickets = vec!["j-a1b2".to_string(), "j-c3d4".to_string()];

        let mut phase2 = Phase::new("2", "Implementation");
        phase2.ticket_list.tickets = vec![
            "j-e5f6".to_string(),
            "j-g7h8".to_string(),
            "j-i9j0".to_string(),
        ];

        let metadata = PlanMetadata {
            id: Some(PlanId::new_unchecked("plan-a1b2")),
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
                id: Some(TicketId::new_unchecked("j-a1b2")),
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
                id: Some(TicketId::new_unchecked("j-a1b2")),
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
            id: Some(TicketId::new_unchecked("j-c3d4")),
            title: Some("Setup OAuth".to_string()),
            status: Some(TicketStatus::Complete),
            depth: Some(1),
            ..Default::default()
        };
        let child2 = TicketMetadata {
            id: Some(TicketId::new_unchecked("j-e5f6")),
            title: Some("Add login flow".to_string()),
            status: Some(TicketStatus::InProgress),
            depth: Some(1),
            ..Default::default()
        };
        let child3 = TicketMetadata {
            id: Some(TicketId::new_unchecked("j-g7h8")),
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
            id: Some(TicketId::new_unchecked("j-c3d4")),
            title: Some("Setup OAuth".to_string()),
            status: Some(TicketStatus::Complete),
            depth: Some(1),
            spawn_context: Some("Setting up OAuth as the first step".to_string()),
            ..Default::default()
        };
        let child2 = TicketMetadata {
            id: Some(TicketId::new_unchecked("j-e5f6")),
            title: Some("Add login flow".to_string()),
            status: Some(TicketStatus::InProgress),
            depth: Some(1),
            spawn_context: Some("Login flow implementation".to_string()),
            ..Default::default()
        };
        let child3 = TicketMetadata {
            id: Some(TicketId::new_unchecked("j-g7h8")),
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
            id: Some(TicketId::new_unchecked("j-c3d4")),
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
    fn test_format_ticket_list_includes_size() {
        use crate::types::{TicketPriority, TicketSize, TicketStatus, TicketType};

        let ticket1 = TicketMetadata {
            id: Some(TicketId::new_unchecked("j-a1b2")),
            title: Some("Add authentication".to_string()),
            status: Some(TicketStatus::New),
            ticket_type: Some(TicketType::Feature),
            priority: Some(TicketPriority::P1),
            size: Some(TicketSize::Medium),
            ..Default::default()
        };
        let ticket2 = TicketMetadata {
            id: Some(TicketId::new_unchecked("j-c3d4")),
            title: Some("Fix login bug".to_string()),
            status: Some(TicketStatus::InProgress),
            ticket_type: Some(TicketType::Bug),
            priority: Some(TicketPriority::P2),
            size: Some(TicketSize::Small),
            ..Default::default()
        };
        let ticket3 = TicketMetadata {
            id: Some(TicketId::new_unchecked("j-e5f6")),
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
            id: Some(TicketId::new_unchecked("j-test")),
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
        let summary =
            build_filter_summary(None, None, None, None, None, None, Some("medium,large"));
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

#[cfg(test)]
mod annotation_tests {
    use super::*;

    #[test]
    fn test_all_tools_have_annotations() {
        let server = JanusTools::new();
        let tools = server.router().list_all();

        assert!(!tools.is_empty(), "Should have registered tools");

        for tool in &tools {
            assert!(
                tool.annotations.is_some(),
                "Tool '{}' should have annotations set",
                tool.name
            );
        }
    }

    #[test]
    fn test_read_only_tools_annotated_correctly() {
        let server = JanusTools::new();
        let tools = server.router().list_all();

        let read_only_tools = [
            "list_tickets",
            "show_ticket",
            "get_children",
            "get_plan_status",
            "get_next_available_ticket",
            "semantic_search",
            "doc_list",
            "doc_show",
            "doc_search",
        ];

        for tool in &tools {
            if read_only_tools.contains(&tool.name.as_ref()) {
                let ann = tool
                    .annotations
                    .as_ref()
                    .expect(&format!("Tool {} missing annotations", tool.name));
                assert!(
                    ann.read_only_hint.unwrap_or(false),
                    "Tool '{}' should be marked read-only",
                    tool.name
                );
                assert!(
                    !ann.destructive_hint.unwrap_or(true),
                    "Read-only tool '{}' should not be destructive",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn test_remove_dependency_is_destructive() {
        let server = JanusTools::new();
        let tools = server.router().list_all();

        let remove_dep_tool = tools
            .iter()
            .find(|t| t.name == "remove_dependency")
            .expect("remove_dependency tool should exist");

        let ann = remove_dep_tool
            .annotations
            .as_ref()
            .expect("remove_dependency should have annotations");

        assert!(
            ann.destructive_hint.unwrap_or(false),
            "remove_dependency should be marked destructive"
        );
        assert!(
            ann.idempotent_hint.unwrap_or(false),
            "remove_dependency should be idempotent"
        );
    }

    #[test]
    fn test_write_tools_not_read_only() {
        let server = JanusTools::new();
        let tools = server.router().list_all();

        let write_tools = [
            "create_ticket",
            "spawn_subtask",
            "update_status",
            "add_note",
            "add_dependency",
            "remove_dependency",
            "add_ticket_to_plan",
            "doc_set",
        ];

        for tool in &tools {
            if write_tools.contains(&tool.name.as_ref()) {
                let ann = tool
                    .annotations
                    .as_ref()
                    .expect(&format!("Tool {} missing annotations", tool.name));
                assert!(
                    !ann.read_only_hint.unwrap_or(true),
                    "Write tool '{}' should not be marked read-only",
                    tool.name
                );
            }
        }
    }
}
