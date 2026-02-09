//! Graph output formatters - DOT and Mermaid generation

use std::collections::HashMap;

use crate::types::TicketMetadata;

use super::types::{Edge, EdgeType};

/// Generate DOT format output
pub fn generate_dot(
    ticket_ids: &std::collections::HashSet<String>,
    edges: &[Edge],
    ticket_map: &HashMap<String, TicketMetadata>,
) -> String {
    let mut lines = vec![
        "digraph janus {".to_string(),
        "  rankdir=TB;".to_string(),
        "  node [shape=box];".to_string(),
        String::new(),
    ];

    let mut sorted_ids: Vec<_> = ticket_ids.iter().collect();
    sorted_ids.sort();

    lines.push("  // Nodes".to_string());
    for id in &sorted_ids {
        let title = ticket_map
            .get(*id)
            .and_then(|t| t.title.as_ref())
            .map(|t| truncate_title(t, 30))
            .unwrap_or_default();
        let label = format!("{}\\n{}", escape_dot(id), escape_dot(&title));
        lines.push(format!("  \"{id}\" [label=\"{label}\"];"));
    }

    if !edges.is_empty() {
        lines.push(String::new());
        lines.push("  // Edges".to_string());

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
pub fn generate_mermaid(
    ticket_ids: &std::collections::HashSet<String>,
    edges: &[Edge],
    ticket_map: &HashMap<String, TicketMetadata>,
) -> String {
    let mut lines = Vec::new();
    lines.push("graph TD".to_string());

    let mut sorted_ids: Vec<_> = ticket_ids.iter().collect();
    sorted_ids.sort();

    for id in &sorted_ids {
        let title = ticket_map
            .get(*id)
            .and_then(|t| t.title.as_ref())
            .map(|t| truncate_title(t, 30))
            .unwrap_or_default();
        let safe_id = id.replace('-', "_");
        let label = format!("{}<br/>{}", escape_mermaid(id), escape_mermaid(&title));
        lines.push(format!("  {safe_id}[\"{label}\"]"));
    }

    if !edges.is_empty() {
        lines.push(String::new());

        let mut sorted_edges = edges.to_vec();
        sorted_edges.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));

        for edge in &sorted_edges {
            let from_safe = edge.from.replace('-', "_");
            let to_safe = edge.to.replace('-', "_");
            match edge.edge_type {
                EdgeType::Blocks => {
                    lines.push(format!("  {from_safe} -->|blocks| {to_safe}"));
                }
                EdgeType::Spawned => {
                    lines.push(format!("  {from_safe} -.->|spawned| {to_safe}"));
                }
            }
        }
    }

    lines.join("\n")
}

fn truncate_title(title: &str, max_len: usize) -> String {
    if title.chars().count() <= max_len {
        title.to_string()
    } else {
        let truncated: String = title.chars().take(max_len.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

fn escape_dot(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn escape_mermaid(s: &str) -> String {
    s.replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\n', "<br/>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TicketId;

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
        let ticket_ids = std::collections::HashSet::new();
        let edges = Vec::new();
        let ticket_map = HashMap::new();

        let output = generate_dot(&ticket_ids, &edges, &ticket_map);
        assert!(output.contains("digraph janus"));
        assert!(output.contains("rankdir=TB"));
    }

    #[test]
    fn test_generate_dot_with_nodes() {
        let mut ticket_ids = std::collections::HashSet::new();
        ticket_ids.insert("j-a1b2".to_string());

        let edges = Vec::new();

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a1b2".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-a1b2")),
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
        let mut ticket_ids = std::collections::HashSet::new();
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
        let mut ticket_ids = std::collections::HashSet::new();
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
        let ticket_ids = std::collections::HashSet::new();
        let edges = Vec::new();
        let ticket_map = HashMap::new();

        let output = generate_mermaid(&ticket_ids, &edges, &ticket_map);
        assert!(output.contains("graph TD"));
    }

    #[test]
    fn test_generate_mermaid_with_nodes() {
        let mut ticket_ids = std::collections::HashSet::new();
        ticket_ids.insert("j-a1b2".to_string());

        let edges = Vec::new();

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a1b2".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-a1b2")),
                title: Some("Test Ticket".to_string()),
                ..Default::default()
            },
        );

        let output = generate_mermaid(&ticket_ids, &edges, &ticket_map);
        assert!(output.contains("j_a1b2"));
        assert!(output.contains("Test Ticket"));
    }

    #[test]
    fn test_generate_mermaid_with_edges() {
        let mut ticket_ids = std::collections::HashSet::new();
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
        let mut ticket_ids = std::collections::HashSet::new();
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
}
