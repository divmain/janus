//! Graph edge building logic

use std::collections::{HashMap, HashSet};

use crate::types::TicketMetadata;

use super::types::{Edge, EdgeType, RelationshipFilter};

/// Build edges between tickets based on filter
pub fn build_edges(
    ticket_ids: &HashSet<String>,
    ticket_map: &HashMap<String, TicketMetadata>,
    filter: RelationshipFilter,
) -> Vec<Edge> {
    let mut edges = Vec::new();

    for id in ticket_ids {
        if let Some(ticket) = ticket_map.get(id) {
            if filter != RelationshipFilter::Spawn {
                for dep in &ticket.deps {
                    if ticket_ids.contains(dep.as_ref()) {
                        edges.push(Edge {
                            from: id.clone(),
                            to: dep.to_string(),
                            edge_type: EdgeType::Blocks,
                        });
                    }
                }
            }

            if filter != RelationshipFilter::Deps
                && let Some(parent) = &ticket.spawned_from
                && ticket_ids.contains(parent.as_ref())
            {
                edges.push(Edge {
                    from: parent.to_string(),
                    to: id.clone(),
                    edge_type: EdgeType::Spawned,
                });
            }
        }
    }

    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TicketId;

    #[test]
    fn test_build_edges_deps_only() {
        let mut ticket_ids = HashSet::new();
        ticket_ids.insert("j-a".to_string());
        ticket_ids.insert("j-b".to_string());

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-a")),
                deps: vec![TicketId::new_unchecked("j-b")],
                spawned_from: Some(TicketId::new_unchecked("j-b")),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-b".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-b")),
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
                id: Some(TicketId::new_unchecked("j-a")),
                deps: vec![TicketId::new_unchecked("j-b")],
                spawned_from: Some(TicketId::new_unchecked("j-b")),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-b".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-b")),
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
                id: Some(TicketId::new_unchecked("j-a")),
                deps: vec![TicketId::new_unchecked("j-b")],
                spawned_from: Some(TicketId::new_unchecked("j-b")),
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-b".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-b")),
                ..Default::default()
            },
        );

        let edges = build_edges(&ticket_ids, &ticket_map, RelationshipFilter::All);
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn test_build_edges_filters_outside_tickets() {
        let mut ticket_ids = HashSet::new();
        ticket_ids.insert("j-a".to_string());

        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-a")),
                deps: vec![TicketId::new_unchecked("j-b")],
                spawned_from: Some(TicketId::new_unchecked("j-c")),
                ..Default::default()
            },
        );

        let edges = build_edges(&ticket_ids, &ticket_map, RelationshipFilter::All);
        assert_eq!(edges.len(), 0);
    }
}
