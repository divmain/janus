//! Graph command - output ticket relationship graphs in DOT and Mermaid formats

mod builder;
mod filter;
mod formatter;
mod types;

pub use builder::build_edges;
pub use formatter::generate_dot;
pub use types::{Edge, EdgeType, GraphFormat, RelationshipFilter};

use std::collections::HashSet;

use serde_json::json;

use super::CommandOutput;
use crate::error::Result;
use crate::ticket::build_ticket_map;

use filter::{get_plan_tickets, get_reachable_tickets};
use formatter::generate_mermaid;

/// Build the graph command output
pub async fn cmd_graph(
    deps_only: bool,
    spawn_only: bool,
    all: bool,
    format: &str,
    root: Option<&str>,
    plan: Option<&str>,
    output_json: bool,
) -> Result<()> {
    // Note: The `all` parameter is accepted for explicitness but is the default behavior
    let _ = all; // Explicitly mark as used
    use types::RelationshipFilter;

    // Validate format early to fail fast
    let graph_format: GraphFormat = format.parse()?;

    let filter = if deps_only && spawn_only {
        RelationshipFilter::All
    } else if deps_only {
        RelationshipFilter::Deps
    } else if spawn_only {
        RelationshipFilter::Spawn
    } else {
        RelationshipFilter::All
    };

    let ticket_map = build_ticket_map().await?;

    let ticket_ids: HashSet<String> = if let Some(root_id) = root {
        get_reachable_tickets(root_id, &ticket_map, filter)?
    } else if let Some(plan_id) = plan {
        get_plan_tickets(plan_id).await?
    } else {
        ticket_map.keys().cloned().collect()
    };

    let edges = build_edges(&ticket_ids, &ticket_map, filter);

    let graph_output = match graph_format {
        GraphFormat::Dot => generate_dot(&ticket_ids, &edges, &ticket_map),
        GraphFormat::Mermaid => generate_mermaid(&ticket_ids, &edges, &ticket_map),
    };

    let nodes_json: Vec<serde_json::Value> = ticket_ids
        .iter()
        .map(|id| {
            let ticket = ticket_map.get(id);
            json!({
                "id": id,
                "title": ticket.and_then(|t| t.title.clone()),
                "status": ticket.and_then(|t| t.status).map(|s| s.to_string()),
            })
        })
        .collect();

    let edges_json: Vec<serde_json::Value> = edges
        .iter()
        .map(|e| {
            json!({
                "from": e.from,
                "to": e.to,
                "type": match e.edge_type {
                    EdgeType::Blocks => "blocks",
                    EdgeType::Spawned => "spawned",
                },
            })
        })
        .collect();

    CommandOutput::new(json!({
        "format": format,
        "nodes": nodes_json,
        "edges": edges_json,
        "graph": graph_output,
    }))
    .with_text(graph_output)
    .print(output_json)
}
