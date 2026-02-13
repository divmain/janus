//! MCP (Model Context Protocol) server for Janus.
//!
//! This module implements an MCP server that exposes Janus functionality
//! to AI agents. The server uses STDIO transport for local integration.
//!
//! # Architecture
//!
//! - `mod.rs` - Server setup and initialization
//! - `tools.rs` - Tool implementations (13 tools for ticket/plan operations)
//! - `resources.rs` - Resource implementations
//! - `types.rs` - MCP-specific types
//! - `format.rs` - Centralized ticket and plan formatting utilities
//!
//! # Usage
//!
//! Start the MCP server:
//! ```bash
//! janus mcp              # Start MCP server (STDIO transport)
//! janus mcp --version    # Show MCP protocol version
//! ```
//!
//! # Available Tools
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

pub mod format;
pub mod requests;
pub mod resources;
pub mod tools;
pub mod types;

use rmcp::{
    RoleServer, ServerHandler, ServiceExt,
    handler::server::tool::ToolCallContext,
    model::{
        CallToolRequestParam, CallToolResult, ErrorData, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParam, ReadResourceRequestParam,
        ReadResourceResult, ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    transport::stdio,
};

use crate::error::Result;
use resources::{ResourceError, list_all_resource_templates, list_all_resources, read_resource};
use tools::JanusTools;
use types::{MCP_PROTOCOL_VERSION, SERVER_NAME, SERVER_VERSION};

impl ServerHandler for JanusTools {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Janus MCP server provides access to plain-text issue tracking. \
                 Use tools to create, query, and manage tickets and plans. \
                 \n\nAvailable tools:\n\
                  - create_ticket: Create a new ticket\n\
                  - spawn_subtask: Create a child ticket with spawning metadata\n\
                  - update_status: Change ticket status (new/next/in_progress/complete/cancelled)\n\
                  - add_note: Add a timestamped note to a ticket\n\
                  - list_tickets: Query tickets with filters\n\
                  - show_ticket: Get full ticket content and relationships\n\
                  - add_dependency: Add a blocking dependency between tickets\n\
                  - remove_dependency: Remove a dependency\n\
                  - add_ticket_to_plan: Add a ticket to a plan\n\
                  - get_plan_status: Get plan progress and phase status\n\
                  - get_children: Get tickets spawned from a parent\n\
                  - semantic_search: Find tickets semantically similar to a query"
                    .to_string(),
            ),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
            server_info: rmcp::model::Implementation {
                name: SERVER_NAME.to_string(),
                version: SERVER_VERSION.to_string(),
                title: None,
                icons: None,
                website_url: None,
            },
            ..Default::default()
        }
    }

    /// List available tools.
    ///
    /// Returns all 13 Janus tools with their schemas and descriptions.
    async fn list_tools(
        &self,
        _pagination: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        let items = self.router().list_all();
        Ok(ListToolsResult::with_all_items(items))
    }

    /// Call a tool.
    ///
    /// Dispatches to the appropriate tool handler based on the tool name.
    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let tcc = ToolCallContext::new(self, request, context);
        self.router().call(tcc).await
    }

    /// List available resources.
    ///
    /// Returns all 9 Janus resources (5 static + 4 templates).
    async fn list_resources(
        &self,
        _pagination: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListResourcesResult, ErrorData> {
        Ok(list_all_resources())
    }

    /// List available resource templates.
    ///
    /// Returns resource templates that require parameters (e.g., `janus://ticket/{id}`).
    async fn list_resource_templates(
        &self,
        _pagination: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListResourceTemplatesResult, ErrorData> {
        Ok(ListResourceTemplatesResult {
            resource_templates: list_all_resource_templates(),
            next_cursor: None,
            meta: None,
        })
    }

    /// Read a resource by URI.
    ///
    /// Supports all 9 Janus resource URIs including template URIs with parameters.
    async fn read_resource(
        &self,
        request: ReadResourceRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ReadResourceResult, ErrorData> {
        match read_resource(&request.uri).await {
            Ok(result) => Ok(result),
            Err(ResourceError::NotFound(msg)) => Err(ErrorData {
                code: rmcp::model::ErrorCode::INVALID_REQUEST,
                message: std::borrow::Cow::Owned(msg),
                data: None,
            }),
            Err(ResourceError::Internal(msg)) => Err(ErrorData {
                code: rmcp::model::ErrorCode::INTERNAL_ERROR,
                message: std::borrow::Cow::Owned(msg),
                data: None,
            }),
        }
    }
}

/// Start the MCP server with STDIO transport.
///
/// This function starts the MCP server and blocks until the server
/// is shut down (via SIGINT/SIGTERM or client disconnect).
///
/// # Errors
///
/// Returns an error if the server fails to start or encounters
/// a fatal error during operation.
pub async fn cmd_mcp() -> Result<()> {
    // Log startup to stderr (stdout is the transport)
    eprintln!("Starting Janus MCP server...");

    // Initialize store and start filesystem watcher for live updates
    let store = crate::store::get_or_init_store().await?;
    if let Err(e) = crate::store::start_watching(store).await {
        eprintln!("Warning: Failed to start filesystem watcher: {e}");
    }

    let server = JanusTools::new();

    // Create STDIO transport and serve
    let service = server
        .serve(stdio())
        .await
        .map_err(|e| crate::error::JanusError::McpServerError(format!("Failed to start: {e}")))?;

    // Wait for the service to complete
    service
        .waiting()
        .await
        .map_err(|e| crate::error::JanusError::McpServerError(format!("{e}")))?;

    Ok(())
}

/// Print the MCP protocol version.
pub fn cmd_mcp_version() -> Result<()> {
    println!("MCP Protocol Version: {MCP_PROTOCOL_VERSION}");
    println!("Janus MCP Server: {SERVER_NAME} v{SERVER_VERSION}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_info() {
        let server = JanusTools::new();
        let info = server.get_info();

        assert!(info.instructions.is_some());
        let instructions = info.instructions.unwrap();
        assert!(instructions.contains("create_ticket"));
        assert!(instructions.contains("spawn_subtask"));
        assert!(instructions.contains("update_status"));
        assert_eq!(info.server_info.name, SERVER_NAME);
        assert_eq!(info.server_info.version, SERVER_VERSION);
    }

    #[test]
    fn test_default_server() {
        let server = JanusTools::default();
        let info = server.get_info();
        assert_eq!(info.server_info.name, SERVER_NAME);
    }

    #[test]
    #[allow(clippy::const_is_empty)]
    fn test_mcp_version_constants() {
        assert_eq!(MCP_PROTOCOL_VERSION, "2024-11-05");
        assert_eq!(SERVER_NAME, "janus");
        assert!(!SERVER_VERSION.is_empty());
    }

    #[test]
    fn test_tools_router_has_tools() {
        let server = JanusTools::new();
        let tools = server.router().list_all();
        // We should have 13 tools (including semantic_search)
        assert_eq!(tools.len(), 13);

        // Verify tool names
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert!(tool_names.contains(&"create_ticket"));
        assert!(tool_names.contains(&"spawn_subtask"));
        assert!(tool_names.contains(&"update_status"));
        assert!(tool_names.contains(&"add_note"));
        assert!(tool_names.contains(&"list_tickets"));
        assert!(tool_names.contains(&"show_ticket"));
        assert!(tool_names.contains(&"add_dependency"));
        assert!(tool_names.contains(&"remove_dependency"));
        assert!(tool_names.contains(&"add_ticket_to_plan"));
        assert!(tool_names.contains(&"get_plan_status"));
        assert!(tool_names.contains(&"get_children"));
        assert!(tool_names.contains(&"get_next_available_ticket"));
        assert!(tool_names.contains(&"semantic_search"));
    }

    #[test]
    fn test_resources_list() {
        let resources = list_all_resources();
        // We should have 5 static resources
        assert_eq!(resources.resources.len(), 5);

        let uris: Vec<&str> = resources
            .resources
            .iter()
            .map(|r| r.raw.uri.as_str())
            .collect();
        assert!(uris.contains(&"janus://tickets/ready"));
        assert!(uris.contains(&"janus://tickets/blocked"));
        assert!(uris.contains(&"janus://tickets/in-progress"));
        assert!(uris.contains(&"janus://graph/deps"));
        assert!(uris.contains(&"janus://graph/spawning"));
    }

    #[test]
    fn test_resource_templates_list() {
        let templates = list_all_resource_templates();
        // We should have 4 resource templates
        assert_eq!(templates.len(), 4);

        let uri_templates: Vec<&str> = templates
            .iter()
            .map(|t| t.raw.uri_template.as_str())
            .collect();
        assert!(uri_templates.contains(&"janus://ticket/{id}"));
        assert!(uri_templates.contains(&"janus://plan/{id}"));
        assert!(uri_templates.contains(&"janus://plan/{id}/next"));
        assert!(uri_templates.contains(&"janus://tickets/spawned-from/{id}"));
    }

    #[test]
    fn test_server_capabilities_include_resources() {
        let server = JanusTools::new();
        let info = server.get_info();
        // Server capabilities should include both tools and resources
        assert!(info.capabilities.tools.is_some());
        assert!(info.capabilities.resources.is_some());
    }
}
