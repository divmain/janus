//! MCP resource implementations for Janus.
//!
//! This module contains the resource implementations that are exposed
//! through the MCP server. Resources provide read-only access to Janus
//! data such as tickets, plans, and graphs.
//!
//! ## Available Resources
//!
//! | URI Pattern | Description | MIME Type |
//! |-------------|-------------|-----------|
//! | `janus://ticket/{id}` | Full ticket markdown content | text/markdown |
//! | `janus://tickets/ready` | List of ready tickets (JSON) | application/json |
//! | `janus://tickets/blocked` | List of blocked tickets (JSON) | application/json |
//! | `janus://tickets/in-progress` | List of in-progress tickets (JSON) | application/json |
//! | `janus://plan/{id}` | Plan with status (JSON) | application/json |
//! | `janus://plan/{id}/next` | Next actionable items (JSON) | application/json |
//! | `janus://tickets/spawned-from/{id}` | Children of ticket (JSON) | application/json |
//! | `janus://graph/deps` | Dependency graph (DOT) | text/vnd.graphviz |
//! | `janus://graph/spawning` | Spawning graph (DOT) | text/vnd.graphviz |

use std::collections::HashSet;

use rmcp::model::{
    ListResourcesResult, RawResource, RawResourceTemplate, ReadResourceResult, Resource,
    ResourceContents, ResourceTemplate,
};
use serde_json::json;

use crate::commands::graph::{RelationshipFilter, build_edges, generate_dot};
use crate::commands::{get_next_items_phased, get_next_items_simple};

use crate::plan::{Plan, compute_all_phase_statuses, compute_plan_status};
use crate::ticket::{Ticket, build_ticket_map, get_all_tickets_with_map};
use crate::types::{TicketMetadata, TicketStatus};

// ============================================================================
// Resource Definitions
// ============================================================================

/// Get all resource definitions for the MCP server.
///
/// This returns both static resources (fixed URIs) and resource templates
/// (URIs with parameters like `{id}`).
pub fn list_all_resources() -> ListResourcesResult {
    let resources = vec![
        // Static resources
        Resource {
            raw: RawResource {
                uri: "janus://tickets/ready".to_string(),
                name: "ready-tickets".to_string(),
                title: Some("Ready Tickets".to_string()),
                description: Some(
                    "List of ready tickets - tickets with status 'new' or 'next' that have all dependencies complete".to_string(),
                ),
                mime_type: Some("application/json".to_string()),
                size: None,
                icons: None,
                meta: None,
            },
            annotations: None,
        },
        Resource {
            raw: RawResource {
                uri: "janus://tickets/blocked".to_string(),
                name: "blocked-tickets".to_string(),
                title: Some("Blocked Tickets".to_string()),
                description: Some(
                    "List of blocked tickets - tickets with incomplete dependencies".to_string(),
                ),
                mime_type: Some("application/json".to_string()),
                size: None,
                icons: None,
                meta: None,
            },
            annotations: None,
        },
        Resource {
            raw: RawResource {
                uri: "janus://tickets/in-progress".to_string(),
                name: "in-progress-tickets".to_string(),
                title: Some("In-Progress Tickets".to_string()),
                description: Some(
                    "List of tickets currently being worked on (status: in_progress)".to_string(),
                ),
                mime_type: Some("application/json".to_string()),
                size: None,
                icons: None,
                meta: None,
            },
            annotations: None,
        },
        Resource {
            raw: RawResource {
                uri: "janus://graph/deps".to_string(),
                name: "dependency-graph".to_string(),
                title: Some("Dependency Graph".to_string()),
                description: Some(
                    "Ticket dependency relationships in DOT graph format".to_string(),
                ),
                mime_type: Some("text/vnd.graphviz".to_string()),
                size: None,
                icons: None,
                meta: None,
            },
            annotations: None,
        },
        Resource {
            raw: RawResource {
                uri: "janus://graph/spawning".to_string(),
                name: "spawning-graph".to_string(),
                title: Some("Spawning Graph".to_string()),
                description: Some(
                    "Ticket spawning relationships (parent/child) in DOT graph format".to_string(),
                ),
                mime_type: Some("text/vnd.graphviz".to_string()),
                size: None,
                icons: None,
                meta: None,
            },
            annotations: None,
        },
    ];

    ListResourcesResult {
        resources,
        next_cursor: None,
        meta: None,
    }
}

/// Get all resource templates (URIs with parameters).
pub fn list_all_resource_templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate {
            raw: RawResourceTemplate {
                uri_template: "janus://ticket/{id}".to_string(),
                name: "ticket".to_string(),
                title: Some("Ticket Content".to_string()),
                description: Some(
                    "Full markdown content of a specific ticket including frontmatter".to_string(),
                ),
                mime_type: Some("text/markdown".to_string()),
                icons: None,
            },
            annotations: None,
        },
        ResourceTemplate {
            raw: RawResourceTemplate {
                uri_template: "janus://plan/{id}".to_string(),
                name: "plan".to_string(),
                title: Some("Plan Status".to_string()),
                description: Some(
                    "Plan details with computed status and phase information".to_string(),
                ),
                mime_type: Some("application/json".to_string()),
                icons: None,
            },
            annotations: None,
        },
        ResourceTemplate {
            raw: RawResourceTemplate {
                uri_template: "janus://plan/{id}/next".to_string(),
                name: "plan-next".to_string(),
                title: Some("Plan Next Items".to_string()),
                description: Some(
                    "Next actionable items in a plan, similar to 'janus plan next'".to_string(),
                ),
                mime_type: Some("application/json".to_string()),
                icons: None,
            },
            annotations: None,
        },
        ResourceTemplate {
            raw: RawResourceTemplate {
                uri_template: "janus://tickets/spawned-from/{id}".to_string(),
                name: "spawned-tickets".to_string(),
                title: Some("Spawned Tickets".to_string()),
                description: Some(
                    "List of tickets spawned from a specific parent ticket".to_string(),
                ),
                mime_type: Some("application/json".to_string()),
                icons: None,
            },
            annotations: None,
        },
    ]
}

// ============================================================================
// Resource Handlers
// ============================================================================

/// Read a resource by its URI.
///
/// Returns the resource content or an error if the resource is not found.
pub async fn read_resource(uri: &str) -> Result<ReadResourceResult, ResourceError> {
    // Parse the URI and dispatch to the appropriate handler
    if let Some(id) = uri.strip_prefix("janus://ticket/") {
        read_ticket(id).await
    } else if uri == "janus://tickets/ready" {
        read_ready_tickets().await
    } else if uri == "janus://tickets/blocked" {
        read_blocked_tickets().await
    } else if uri == "janus://tickets/in-progress" {
        read_in_progress_tickets().await
    } else if let Some(rest) = uri.strip_prefix("janus://plan/") {
        // Check if it's plan/{id}/next or just plan/{id}
        if let Some(id) = rest.strip_suffix("/next") {
            read_plan_next(id).await
        } else {
            read_plan(rest).await
        }
    } else if let Some(id) = uri.strip_prefix("janus://tickets/spawned-from/") {
        read_spawned_from(id).await
    } else if uri == "janus://graph/deps" {
        read_graph_deps().await
    } else if uri == "janus://graph/spawning" {
        read_graph_spawning().await
    } else {
        Err(ResourceError::NotFound(uri.to_string()))
    }
}

/// Error type for resource operations
#[derive(Debug)]
pub enum ResourceError {
    /// Resource not found
    NotFound(String),
    /// Internal error
    Internal(String),
}

impl std::fmt::Display for ResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceError::NotFound(uri) => write!(f, "Resource not found: {uri}"),
            ResourceError::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

// ============================================================================
// Individual Resource Handlers
// ============================================================================

/// Read a ticket's full markdown content
async fn read_ticket(id: &str) -> Result<ReadResourceResult, ResourceError> {
    let ticket = Ticket::find(id)
        .await
        .map_err(|e| ResourceError::NotFound(format!("Ticket '{id}' not found: {e}")))?;

    let content = ticket
        .read_content()
        .map_err(|e| ResourceError::Internal(format!("Failed to read ticket: {e}")))?;

    Ok(ReadResourceResult {
        contents: vec![ResourceContents::TextResourceContents {
            uri: format!("janus://ticket/{}", ticket.id),
            mime_type: Some("text/markdown".to_string()),
            text: content,
            meta: None,
        }],
    })
}

/// Read list of ready tickets (new/next with all deps complete)
async fn read_ready_tickets() -> Result<ReadResourceResult, ResourceError> {
    let (tickets, ticket_map) = get_all_tickets_with_map()
        .await
        .map_err(|e| ResourceError::Internal(e.to_string()))?;

    let ready: Vec<serde_json::Value> = tickets
        .iter()
        .filter(|t| {
            // Must be new or next status
            if !matches!(t.status, Some(TicketStatus::New) | Some(TicketStatus::Next)) {
                return false;
            }
            // All deps must be terminal (complete or cancelled)
            t.deps.iter().all(|dep_id| {
                ticket_map
                    .get(dep_id)
                    .is_some_and(|dep| dep.status.is_some_and(|s| s.is_terminal()))
            })
        })
        .map(ticket_to_json)
        .collect();

    let json = json!({
        "count": ready.len(),
        "tickets": ready,
    });

    Ok(ReadResourceResult {
        contents: vec![ResourceContents::TextResourceContents {
            uri: "janus://tickets/ready".to_string(),
            mime_type: Some("application/json".to_string()),
            text: serde_json::to_string_pretty(&json).unwrap(),
            meta: None,
        }],
    })
}

/// Read list of blocked tickets (has incomplete deps)
async fn read_blocked_tickets() -> Result<ReadResourceResult, ResourceError> {
    let (tickets, ticket_map) = get_all_tickets_with_map()
        .await
        .map_err(|e| ResourceError::Internal(e.to_string()))?;

    let blocked: Vec<serde_json::Value> = tickets
        .iter()
        .filter(|t| {
            // Must be new or next status
            if !matches!(t.status, Some(TicketStatus::New) | Some(TicketStatus::Next)) {
                return false;
            }
            // Must have deps
            if t.deps.is_empty() {
                return false;
            }
            // At least one dep must be incomplete (not terminal)
            t.deps.iter().any(|dep_id| {
                ticket_map
                    .get(dep_id)
                    .is_none_or(|dep| !dep.status.is_some_and(|s| s.is_terminal()))
            })
        })
        .map(|t| {
            let mut json = ticket_to_json(t);
            // Add blocking deps info
            let blocking_deps: Vec<serde_json::Value> = t
                .deps
                .iter()
                .filter(|dep_id| {
                    ticket_map
                        .get(*dep_id)
                        .is_none_or(|dep| !dep.status.is_some_and(|s| s.is_terminal()))
                })
                .map(|dep_id| {
                    let dep = ticket_map.get(dep_id);
                    json!({
                        "id": dep_id,
                        "title": dep.and_then(|d| d.title.clone()),
                        "status": dep.and_then(|d| d.status).map(|s| s.to_string()),
                    })
                })
                .collect();
            json["blocking_deps"] = json!(blocking_deps);
            json
        })
        .collect();

    let json = json!({
        "count": blocked.len(),
        "tickets": blocked,
    });

    Ok(ReadResourceResult {
        contents: vec![ResourceContents::TextResourceContents {
            uri: "janus://tickets/blocked".to_string(),
            mime_type: Some("application/json".to_string()),
            text: serde_json::to_string_pretty(&json).unwrap(),
            meta: None,
        }],
    })
}

/// Read list of in-progress tickets
async fn read_in_progress_tickets() -> Result<ReadResourceResult, ResourceError> {
    let (tickets, _) = get_all_tickets_with_map()
        .await
        .map_err(|e| ResourceError::Internal(e.to_string()))?;

    let in_progress: Vec<serde_json::Value> = tickets
        .iter()
        .filter(|t| t.status == Some(TicketStatus::InProgress))
        .map(ticket_to_json)
        .collect();

    let json = json!({
        "count": in_progress.len(),
        "tickets": in_progress,
    });

    Ok(ReadResourceResult {
        contents: vec![ResourceContents::TextResourceContents {
            uri: "janus://tickets/in-progress".to_string(),
            mime_type: Some("application/json".to_string()),
            text: serde_json::to_string_pretty(&json).unwrap(),
            meta: None,
        }],
    })
}

/// Read plan with computed status
async fn read_plan(id: &str) -> Result<ReadResourceResult, ResourceError> {
    let plan = Plan::find(id)
        .await
        .map_err(|e| ResourceError::NotFound(format!("Plan '{id}' not found: {e}")))?;

    let metadata = plan
        .read()
        .map_err(|e| ResourceError::Internal(format!("Failed to read plan: {e}")))?;

    let ticket_map = build_ticket_map()
        .await
        .map_err(|e| ResourceError::Internal(format!("Failed to load tickets: {e}")))?;
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

    let all_tickets: Vec<&str> = metadata.all_tickets();
    let tickets_json: Vec<serde_json::Value> = all_tickets
        .iter()
        .map(|ticket_id| {
            let ticket = ticket_map.get(*ticket_id);
            json!({
                "id": ticket_id,
                "title": ticket.and_then(|t| t.title.clone()),
                "status": ticket.and_then(|t| t.status).map(|s| s.to_string()),
                "exists": ticket.is_some(),
            })
        })
        .collect();

    let json = json!({
        "plan_id": plan.id,
        "title": metadata.title,
        "description": metadata.description,
        "status": plan_status.status.to_string(),
        "completed_count": plan_status.completed_count,
        "total_count": plan_status.total_count,
        "progress_percent": plan_status.progress_percent(),
        "progress_string": plan_status.progress_string(),
        "is_phased": metadata.is_phased(),
        "phases": phases_json,
        "tickets": tickets_json,
    });

    Ok(ReadResourceResult {
        contents: vec![ResourceContents::TextResourceContents {
            uri: format!("janus://plan/{}", plan.id),
            mime_type: Some("application/json".to_string()),
            text: serde_json::to_string_pretty(&json).unwrap(),
            meta: None,
        }],
    })
}

/// Read next actionable items in a plan
async fn read_plan_next(id: &str) -> Result<ReadResourceResult, ResourceError> {
    let plan = Plan::find(id)
        .await
        .map_err(|e| ResourceError::NotFound(format!("Plan '{id}' not found: {e}")))?;

    let metadata = plan
        .read()
        .map_err(|e| ResourceError::Internal(format!("Failed to read plan: {e}")))?;

    let ticket_map = build_ticket_map()
        .await
        .map_err(|e| ResourceError::Internal(format!("Failed to load tickets: {e}")))?;

    // Get next items (using a reasonable default count)
    let next_items = if metadata.is_phased() {
        get_next_items_phased(&metadata, &ticket_map, false, true, 5)
    } else {
        get_next_items_simple(&metadata, &ticket_map, 5)
    };

    let next_items_json: Vec<serde_json::Value> = next_items
        .iter()
        .map(|item| {
            let tickets_json: Vec<serde_json::Value> = item
                .tickets
                .iter()
                .map(|(ticket_id, ticket_meta)| {
                    json!({
                        "id": ticket_id,
                        "title": ticket_meta.as_ref().and_then(|t| t.title.clone()),
                        "status": ticket_meta.as_ref().and_then(|t| t.status).map(|s| s.to_string()),
                        "priority": ticket_meta.as_ref().and_then(|t| t.priority).map(|p| p.as_num()),
                        "deps": ticket_meta.as_ref().map(|t| &t.deps).cloned().unwrap_or_default(),
                        "exists": ticket_meta.is_some(),
                    })
                })
                .collect();

            json!({
                "phase_number": item.phase_number,
                "phase_name": item.phase_name,
                "tickets": tickets_json,
            })
        })
        .collect();

    let json = json!({
        "plan_id": plan.id,
        "next_items": next_items_json,
    });

    Ok(ReadResourceResult {
        contents: vec![ResourceContents::TextResourceContents {
            uri: format!("janus://plan/{}/next", plan.id),
            mime_type: Some("application/json".to_string()),
            text: serde_json::to_string_pretty(&json).unwrap(),
            meta: None,
        }],
    })
}

/// Read tickets spawned from a parent ticket
async fn read_spawned_from(id: &str) -> Result<ReadResourceResult, ResourceError> {
    let parent = Ticket::find(id)
        .await
        .map_err(|e| ResourceError::NotFound(format!("Ticket '{id}' not found: {e}")))?;

    let (tickets, _) = get_all_tickets_with_map()
        .await
        .map_err(|e| ResourceError::Internal(format!("Failed to load tickets: {e}")))?;

    let children: Vec<serde_json::Value> = tickets
        .iter()
        .filter(|t| t.spawned_from.as_ref() == Some(&parent.id))
        .map(ticket_to_json)
        .collect();

    let json = json!({
        "parent_id": parent.id,
        "count": children.len(),
        "children": children,
    });

    Ok(ReadResourceResult {
        contents: vec![ResourceContents::TextResourceContents {
            uri: format!("janus://tickets/spawned-from/{}", parent.id),
            mime_type: Some("application/json".to_string()),
            text: serde_json::to_string_pretty(&json).unwrap(),
            meta: None,
        }],
    })
}

/// Read dependency graph in DOT format
async fn read_graph_deps() -> Result<ReadResourceResult, ResourceError> {
    let ticket_map = build_ticket_map()
        .await
        .map_err(|e| ResourceError::Internal(format!("Failed to load tickets: {e}")))?;
    let ticket_ids: HashSet<String> = ticket_map.keys().cloned().collect();
    let edges = build_edges(&ticket_ids, &ticket_map, RelationshipFilter::Deps);
    let dot = generate_dot(&ticket_ids, &edges, &ticket_map);

    Ok(ReadResourceResult {
        contents: vec![ResourceContents::TextResourceContents {
            uri: "janus://graph/deps".to_string(),
            mime_type: Some("text/vnd.graphviz".to_string()),
            text: dot,
            meta: None,
        }],
    })
}

/// Read spawning graph in DOT format
async fn read_graph_spawning() -> Result<ReadResourceResult, ResourceError> {
    let ticket_map = build_ticket_map()
        .await
        .map_err(|e| ResourceError::Internal(format!("Failed to load tickets: {e}")))?;
    let ticket_ids: HashSet<String> = ticket_map.keys().cloned().collect();
    let edges = build_edges(&ticket_ids, &ticket_map, RelationshipFilter::Spawn);
    let dot = generate_dot(&ticket_ids, &edges, &ticket_map);

    Ok(ReadResourceResult {
        contents: vec![ResourceContents::TextResourceContents {
            uri: "janus://graph/spawning".to_string(),
            mime_type: Some("text/vnd.graphviz".to_string()),
            text: dot,
            meta: None,
        }],
    })
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert ticket metadata to JSON value
fn ticket_to_json(ticket: &TicketMetadata) -> serde_json::Value {
    json!({
        "id": ticket.id,
        "title": ticket.title,
        "status": ticket.status.map(|s| s.to_string()),
        "type": ticket.ticket_type.map(|t| t.to_string()),
        "priority": ticket.priority.map(|p| p.as_num()),
        "deps": ticket.deps,
        "spawned_from": ticket.spawned_from,
        "depth": ticket.depth,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_all_resources() {
        let result = list_all_resources();
        assert_eq!(result.resources.len(), 5);

        // Check static resources
        let uris: Vec<&str> = result
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
    fn test_list_all_resource_templates() {
        let templates = list_all_resource_templates();
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
    fn test_ticket_to_json() {
        let ticket = TicketMetadata {
            id: Some("j-test".to_string()),
            title: Some("Test Ticket".to_string()),
            status: Some(TicketStatus::New),
            deps: vec!["j-dep1".to_string()],
            spawned_from: Some("j-parent".to_string()),
            depth: Some(1),
            ..Default::default()
        };

        let json = ticket_to_json(&ticket);
        assert_eq!(json["id"], "j-test");
        assert_eq!(json["title"], "Test Ticket");
        assert_eq!(json["status"], "new");
        assert_eq!(json["spawned_from"], "j-parent");
        assert_eq!(json["depth"], 1);
    }

    #[test]
    fn test_resource_error_display() {
        let not_found = ResourceError::NotFound("janus://ticket/xyz".to_string());
        assert!(not_found.to_string().contains("not found"));
        assert!(not_found.to_string().contains("janus://ticket/xyz"));

        let internal = ResourceError::Internal("something went wrong".to_string());
        assert!(internal.to_string().contains("Internal error"));
        assert!(internal.to_string().contains("something went wrong"));
    }
}
