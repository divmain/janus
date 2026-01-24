//! Graph command - output ticket relationship graphs in DOT and Mermaid formats

use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::json;

use super::CommandOutput;
use crate::error::{JanusError, Result};
use crate::plan::Plan;
use crate::ticket::{build_ticket_map, resolve_id_partial};
use crate::types::TicketMetadata;

/// Output format for the graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphFormat {
    #[default]
    Dot,
    Mermaid,
}

impl std::str::FromStr for GraphFormat {
    type Err = JanusError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "dot" => Ok(GraphFormat::Dot),
            "mermaid" => Ok(GraphFormat::Mermaid),
            _ => Err(JanusError::Other(format!(
                "Invalid graph format '{}'. Must be 'dot' or 'mermaid'",
                s
            ))),
        }
    }
}

/// What types of relationships to include in the graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RelationshipFilter {
    /// Only show dependency relationships (blocks/blocked-by)
    Deps,
    /// Only show spawning relationships (parent/child via spawned_from)
    Spawn,
    /// Show both deps and spawning relationships
    #[default]
    All,
}

/// Edge in the graph
#[derive(Debug, Clone)]
struct Edge {
    from: String,
    to: String,
    edge_type: EdgeType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeType {
    /// Dependency: from blocks to (to must complete before from can start)
    Blocks,
    /// Spawning: from spawned to
    Spawned,
}

/// Build the graph command output
pub async fn cmd_graph(
    deps_only: bool,
    spawn_only: bool,
    _all: bool, // Used for explicit --all flag, but All is the default
    format: &str,
    root: Option<&str>,
    plan: Option<&str>,
    output_json: bool,
) -> Result<()> {
    // Determine relationship filter
    let filter = if deps_only && spawn_only {
        // Both flags = all (weird but handle it)
        RelationshipFilter::All
    } else if deps_only {
        RelationshipFilter::Deps
    } else if spawn_only {
        RelationshipFilter::Spawn
    } else {
        RelationshipFilter::All
    };

    // Parse format
    let graph_format: GraphFormat = format.parse()?;

    // Get all tickets
    let ticket_map = build_ticket_map().await?;

    // Determine which tickets to include
    let ticket_ids: HashSet<String> = if let Some(root_id) = root {
        // Start from specific ticket, do BFS traversal
        get_reachable_tickets(root_id, &ticket_map, filter)?
    } else if let Some(plan_id) = plan {
        // Get all tickets in the plan
        get_plan_tickets(plan_id).await?
    } else {
        // All tickets
        ticket_map.keys().cloned().collect()
    };

    // Build edges based on filter
    let edges = build_edges(&ticket_ids, &ticket_map, filter);

    // Generate output
    let graph_output = match graph_format {
        GraphFormat::Dot => generate_dot(&ticket_ids, &edges, &ticket_map),
        GraphFormat::Mermaid => generate_mermaid(&ticket_ids, &edges, &ticket_map),
    };

    // Build JSON output
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

/// Get all tickets reachable from a root ticket via relationships
fn get_reachable_tickets(
    root_id: &str,
    ticket_map: &HashMap<String, TicketMetadata>,
    filter: RelationshipFilter,
) -> Result<HashSet<String>> {
    let root = resolve_id_partial(root_id, ticket_map)?;

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(root.clone());

    while let Some(current) = queue.pop_front() {
        if visited.contains(&current) {
            continue;
        }
        visited.insert(current.clone());

        if let Some(ticket) = ticket_map.get(&current) {
            // Follow deps edges (outgoing)
            if filter != RelationshipFilter::Spawn {
                for dep in &ticket.deps {
                    if !visited.contains(dep) {
                        queue.push_back(dep.clone());
                    }
                }
            }

            // Follow spawned_from edges (outgoing - to parent)
            if filter != RelationshipFilter::Deps
                && let Some(parent) = &ticket.spawned_from
                && !visited.contains(parent)
            {
                queue.push_back(parent.clone());
            }
        }

        // Also follow reverse edges (tickets that depend on or were spawned from current)
        for (id, other_ticket) in ticket_map {
            if visited.contains(id) {
                continue;
            }

            // Reverse deps: other depends on current
            if filter != RelationshipFilter::Spawn && other_ticket.deps.contains(&current) {
                queue.push_back(id.clone());
            }

            // Reverse spawn: other was spawned from current
            if filter != RelationshipFilter::Deps
                && other_ticket.spawned_from.as_ref() == Some(&current)
            {
                queue.push_back(id.clone());
            }
        }
    }

    Ok(visited)
}

/// Get all tickets from a plan
async fn get_plan_tickets(plan_id: &str) -> Result<HashSet<String>> {
    let plan = Plan::find(plan_id).await?;
    let metadata = plan.read()?;
    Ok(metadata
        .all_tickets()
        .into_iter()
        .map(String::from)
        .collect())
}

/// Build edges between tickets based on filter
fn build_edges(
    ticket_ids: &HashSet<String>,
    ticket_map: &HashMap<String, TicketMetadata>,
    filter: RelationshipFilter,
) -> Vec<Edge> {
    let mut edges = Vec::new();

    for id in ticket_ids {
        if let Some(ticket) = ticket_map.get(id) {
            // Add dependency edges
            if filter != RelationshipFilter::Spawn {
                for dep in &ticket.deps {
                    if ticket_ids.contains(dep) {
                        edges.push(Edge {
                            from: id.clone(),
                            to: dep.clone(),
                            edge_type: EdgeType::Blocks,
                        });
                    }
                }
            }

            // Add spawning edges
            if filter != RelationshipFilter::Deps
                && let Some(parent) = &ticket.spawned_from
                && ticket_ids.contains(parent)
            {
                edges.push(Edge {
                    from: parent.clone(),
                    to: id.clone(),
                    edge_type: EdgeType::Spawned,
                });
            }
        }
    }

    edges
}

/// Truncate title to a maximum length
fn truncate_title(title: &str, max_len: usize) -> String {
    if title.len() <= max_len {
        title.to_string()
    } else {
        format!("{}...", &title[..max_len - 3])
    }
}

/// Escape a string for DOT format
fn escape_dot(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Escape a string for Mermaid format
fn escape_mermaid(s: &str) -> String {
    s.replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\n', "<br/>")
}

/// Generate DOT format output
fn generate_dot(
    ticket_ids: &HashSet<String>,
    edges: &[Edge],
    ticket_map: &HashMap<String, TicketMetadata>,
) -> String {
    let mut lines = vec![
        "digraph janus {".to_string(),
        "  rankdir=TB;".to_string(),
        "  node [shape=box];".to_string(),
        String::new(),
    ];

    // Sort ticket IDs for deterministic output
    let mut sorted_ids: Vec<_> = ticket_ids.iter().collect();
    sorted_ids.sort();

    // Nodes
    lines.push("  // Nodes".to_string());
    for id in &sorted_ids {
        let title = ticket_map
            .get(*id)
            .and_then(|t| t.title.as_ref())
            .map(|t| truncate_title(t, 30))
            .unwrap_or_default();
        let label = format!("{}\\n{}", escape_dot(id), escape_dot(&title));
        lines.push(format!("  \"{}\" [label=\"{}\"];", id, label));
    }

    if !edges.is_empty() {
        lines.push(String::new());
        lines.push("  // Edges".to_string());

        // Sort edges for deterministic output
        let mut sorted_edges = edges.to_vec();
        sorted_edges.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));

        for edge in &sorted_edges {
            match edge.edge_type {
                EdgeType::Blocks => {
                    lines.push(format!(
                        "  \"{}\" -> \"{}\" [label=\"blocks\"];",
                        edge.from, edge.to
                    ));
                }
                EdgeType::Spawned => {
                    lines.push(format!(
                        "  \"{}\" -> \"{}\" [style=dashed, label=\"spawned\"];",
                        edge.from, edge.to
                    ));
                }
            }
        }
    }

    lines.push("}".to_string());
    lines.join("\n")
}

/// Generate Mermaid format output
fn generate_mermaid(
    ticket_ids: &HashSet<String>,
    edges: &[Edge],
    ticket_map: &HashMap<String, TicketMetadata>,
) -> String {
    let mut lines = Vec::new();
    lines.push("graph TD".to_string());

    // Sort ticket IDs for deterministic output
    let mut sorted_ids: Vec<_> = ticket_ids.iter().collect();
    sorted_ids.sort();

    // Nodes
    for id in &sorted_ids {
        let title = ticket_map
            .get(*id)
            .and_then(|t| t.title.as_ref())
            .map(|t| truncate_title(t, 30))
            .unwrap_or_default();
        // Mermaid node format: id["label"]
        // Need to escape special chars and replace hyphens in IDs
        let safe_id = id.replace('-', "_");
        let label = format!("{}<br/>{}", escape_mermaid(id), escape_mermaid(&title));
        lines.push(format!("  {}[\"{}\"]", safe_id, label));
    }

    if !edges.is_empty() {
        lines.push(String::new());

        // Sort edges for deterministic output
        let mut sorted_edges = edges.to_vec();
        sorted_edges.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));

        for edge in &sorted_edges {
            let from_safe = edge.from.replace('-', "_");
            let to_safe = edge.to.replace('-', "_");
            match edge.edge_type {
                EdgeType::Blocks => {
                    lines.push(format!("  {} -->|blocks| {}", from_safe, to_safe));
                }
                EdgeType::Spawned => {
                    lines.push(format!("  {} -.->|spawned| {}", from_safe, to_safe));
                }
            }
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_format_from_str() {
        assert_eq!("dot".parse::<GraphFormat>().unwrap(), GraphFormat::Dot);
        assert_eq!("DOT".parse::<GraphFormat>().unwrap(), GraphFormat::Dot);
        assert_eq!(
            "mermaid".parse::<GraphFormat>().unwrap(),
            GraphFormat::Mermaid
        );
        assert_eq!(
            "MERMAID".parse::<GraphFormat>().unwrap(),
            GraphFormat::Mermaid
        );
        assert!("invalid".parse::<GraphFormat>().is_err());
    }

    #[test]
    fn test_truncate_title() {
        assert_eq!(truncate_title("short", 30), "short");
        assert_eq!(
            truncate_title("this is a very long title that needs truncation", 20),
            "this is a very lo..."
        );
    }

    #[test]
    fn test_escape_dot() {
        assert_eq!(escape_dot("hello"), "hello");
        assert_eq!(escape_dot("hello \"world\""), "hello \\\"world\\\"");
        assert_eq!(escape_dot("line1\nline2"), "line1\\nline2");
    }

    #[test]
    fn test_escape_mermaid() {
        assert_eq!(escape_mermaid("hello"), "hello");
        assert_eq!(escape_mermaid("hello \"world\""), "hello &quot;world&quot;");
        assert_eq!(escape_mermaid("<tag>"), "&lt;tag&gt;");
    }

    #[test]
    fn test_generate_dot_empty() {
        let ticket_ids = HashSet::new();
        let edges = Vec::new();
        let ticket_map = HashMap::new();

        let output = generate_dot(&ticket_ids, &edges, &ticket_map);
        assert!(output.contains("digraph janus"));
        assert!(output.contains("rankdir=TB"));
    }

    #[test]
    fn test_generate_dot_with_nodes() {
        let mut ticket_ids = HashSet::new();
        ticket_ids.insert("j-a1b2".to_string());

        let edges = Vec::new();

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a1b2".to_string(),
            TicketMetadata {
                id: Some("j-a1b2".to_string()),
                title: Some("Test Ticket".to_string()),
                ..Default::default()
            },
        );

        let output = generate_dot(&ticket_ids, &edges, &ticket_map);
        assert!(output.contains("\"j-a1b2\""));
        assert!(output.contains("Test Ticket"));
    }

    #[test]
    fn test_generate_dot_with_edges() {
        let mut ticket_ids = HashSet::new();
        ticket_ids.insert("j-a1b2".to_string());
        ticket_ids.insert("j-c3d4".to_string());

        let edges = vec![Edge {
            from: "j-a1b2".to_string(),
            to: "j-c3d4".to_string(),
            edge_type: EdgeType::Blocks,
        }];

        let ticket_map = HashMap::new();

        let output = generate_dot(&ticket_ids, &edges, &ticket_map);
        assert!(output.contains("\"j-a1b2\" -> \"j-c3d4\""));
        assert!(output.contains("blocks"));
    }

    #[test]
    fn test_generate_dot_spawned_edge() {
        let mut ticket_ids = HashSet::new();
        ticket_ids.insert("j-parent".to_string());
        ticket_ids.insert("j-child".to_string());

        let edges = vec![Edge {
            from: "j-parent".to_string(),
            to: "j-child".to_string(),
            edge_type: EdgeType::Spawned,
        }];

        let ticket_map = HashMap::new();

        let output = generate_dot(&ticket_ids, &edges, &ticket_map);
        assert!(output.contains("style=dashed"));
        assert!(output.contains("spawned"));
    }

    #[test]
    fn test_generate_mermaid_empty() {
        let ticket_ids = HashSet::new();
        let edges = Vec::new();
        let ticket_map = HashMap::new();

        let output = generate_mermaid(&ticket_ids, &edges, &ticket_map);
        assert!(output.contains("graph TD"));
    }

    #[test]
    fn test_generate_mermaid_with_nodes() {
        let mut ticket_ids = HashSet::new();
        ticket_ids.insert("j-a1b2".to_string());

        let edges = Vec::new();

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a1b2".to_string(),
            TicketMetadata {
                id: Some("j-a1b2".to_string()),
                title: Some("Test Ticket".to_string()),
                ..Default::default()
            },
        );

        let output = generate_mermaid(&ticket_ids, &edges, &ticket_map);
        assert!(output.contains("j_a1b2")); // Hyphen replaced with underscore
        assert!(output.contains("Test Ticket"));
    }

    #[test]
    fn test_generate_mermaid_with_edges() {
        let mut ticket_ids = HashSet::new();
        ticket_ids.insert("j-a1b2".to_string());
        ticket_ids.insert("j-c3d4".to_string());

        let edges = vec![Edge {
            from: "j-a1b2".to_string(),
            to: "j-c3d4".to_string(),
            edge_type: EdgeType::Blocks,
        }];

        let ticket_map = HashMap::new();

        let output = generate_mermaid(&ticket_ids, &edges, &ticket_map);
        assert!(output.contains("j_a1b2 -->|blocks| j_c3d4"));
    }

    #[test]
    fn test_generate_mermaid_spawned_edge() {
        let mut ticket_ids = HashSet::new();
        ticket_ids.insert("j-parent".to_string());
        ticket_ids.insert("j-child".to_string());

        let edges = vec![Edge {
            from: "j-parent".to_string(),
            to: "j-child".to_string(),
            edge_type: EdgeType::Spawned,
        }];

        let ticket_map = HashMap::new();

        let output = generate_mermaid(&ticket_ids, &edges, &ticket_map);
        assert!(output.contains("-.->"));
        assert!(output.contains("spawned"));
    }

    #[test]
    fn test_build_edges_deps_only() {
        let mut ticket_ids = HashSet::new();
        ticket_ids.insert("j-a".to_string());
        ticket_ids.insert("j-b".to_string());

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a".to_string(),
            TicketMetadata {
                id: Some("j-a".to_string()),
                deps: vec!["j-b".to_string()],
                spawned_from: Some("j-b".to_string()),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-b".to_string(),
            TicketMetadata {
                id: Some("j-b".to_string()),
                ..Default::default()
            },
        );

        let edges = build_edges(&ticket_ids, &ticket_map, RelationshipFilter::Deps);
        assert_eq!(edges.len(), 1);
        assert!(matches!(edges[0].edge_type, EdgeType::Blocks));
    }

    #[test]
    fn test_build_edges_spawn_only() {
        let mut ticket_ids = HashSet::new();
        ticket_ids.insert("j-a".to_string());
        ticket_ids.insert("j-b".to_string());

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a".to_string(),
            TicketMetadata {
                id: Some("j-a".to_string()),
                deps: vec!["j-b".to_string()],
                spawned_from: Some("j-b".to_string()),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-b".to_string(),
            TicketMetadata {
                id: Some("j-b".to_string()),
                ..Default::default()
            },
        );

        let edges = build_edges(&ticket_ids, &ticket_map, RelationshipFilter::Spawn);
        assert_eq!(edges.len(), 1);
        assert!(matches!(edges[0].edge_type, EdgeType::Spawned));
    }

    #[test]
    fn test_build_edges_all() {
        let mut ticket_ids = HashSet::new();
        ticket_ids.insert("j-a".to_string());
        ticket_ids.insert("j-b".to_string());

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a".to_string(),
            TicketMetadata {
                id: Some("j-a".to_string()),
                deps: vec!["j-b".to_string()],
                spawned_from: Some("j-b".to_string()),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-b".to_string(),
            TicketMetadata {
                id: Some("j-b".to_string()),
                ..Default::default()
            },
        );

        let edges = build_edges(&ticket_ids, &ticket_map, RelationshipFilter::All);
        assert_eq!(edges.len(), 2);
    }
}
