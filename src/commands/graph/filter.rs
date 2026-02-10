//! Graph filtering logic - BFS traversal and relationship filtering

use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::Result;
use crate::graph::resolve_id_from_map;
use crate::plan::Plan;
use crate::types::TicketMetadata;

use super::types::RelationshipFilter;

/// Pre-computed reverse edges for efficient graph traversal
struct ReverseEdges {
    /// Tickets that depend on each ticket (reverse of deps)
    reverse_deps: HashMap<String, Vec<String>>,
    /// Tickets spawned from each parent ticket (reverse of spawned_from)
    reverse_spawned: HashMap<String, Vec<String>>,
}

impl ReverseEdges {
    fn build(ticket_map: &HashMap<String, TicketMetadata>) -> Self {
        let mut reverse_deps: HashMap<String, Vec<String>> = HashMap::new();
        let mut reverse_spawned: HashMap<String, Vec<String>> = HashMap::new();

        for (id, ticket) in ticket_map {
            // Build reverse_deps: for each dep, add this ticket to its reverse list
            for dep in &ticket.deps {
                reverse_deps
                    .entry(dep.clone())
                    .or_default()
                    .push(id.clone());
            }

            // Build reverse_spawned: for each parent, add this ticket to its children list
            if let Some(parent) = &ticket.spawned_from {
                reverse_spawned
                    .entry(parent.to_string())
                    .or_default()
                    .push(id.clone());
            }
        }

        Self {
            reverse_deps,
            reverse_spawned,
        }
    }

    fn get_dependents(&self, ticket_id: &str) -> &[String] {
        self.reverse_deps
            .get(ticket_id)
            .map_or(&[], |v| v.as_slice())
    }

    fn get_children(&self, ticket_id: &str) -> &[String] {
        self.reverse_spawned
            .get(ticket_id)
            .map_or(&[], |v| v.as_slice())
    }
}

/// Get all tickets reachable from a root ticket via relationships
pub fn get_reachable_tickets(
    root_id: &str,
    ticket_map: &HashMap<String, TicketMetadata>,
    filter: RelationshipFilter,
) -> Result<HashSet<String>> {
    let root = resolve_id_from_map(root_id, ticket_map)?;

    let reverse_edges = ReverseEdges::build(ticket_map);

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(root.clone());

    while let Some(current) = queue.pop_front() {
        if visited.contains(&current) {
            continue;
        }
        visited.insert(current.clone());

        if let Some(ticket) = ticket_map.get(&current) {
            if filter != RelationshipFilter::Spawn {
                for dep in &ticket.deps {
                    if !visited.contains(dep) {
                        queue.push_back(dep.clone());
                    }
                }
            }

            if filter != RelationshipFilter::Deps
                && let Some(parent) = &ticket.spawned_from
                && !visited.contains(parent.as_ref())
            {
                queue.push_back(parent.to_string());
            }
        }

        // O(1) lookups for reverse relationships instead of O(n) scan
        if filter != RelationshipFilter::Spawn {
            for dependent in reverse_edges.get_dependents(&current) {
                if !visited.contains(dependent) {
                    queue.push_back(dependent.clone());
                }
            }
        }

        if filter != RelationshipFilter::Deps {
            for child in reverse_edges.get_children(&current) {
                if !visited.contains(child) {
                    queue.push_back(child.clone());
                }
            }
        }
    }

    Ok(visited)
}

/// Get all tickets from a plan
pub async fn get_plan_tickets(plan_id: &str) -> Result<HashSet<String>> {
    let plan = Plan::find(plan_id).await?;
    let metadata = plan.read()?;
    Ok(metadata
        .all_tickets()
        .into_iter()
        .map(String::from)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TicketId;

    #[test]
    fn test_get_reachable_tickets_basic() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-a")),
                deps: vec!["j-b".to_string()],
                spawned_from: None,
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-b".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-b")),
                deps: vec![],
                spawned_from: None,
                ..Default::default()
            },
        );

        let result = get_reachable_tickets("j-a", &ticket_map, RelationshipFilter::All).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains("j-a"));
        assert!(result.contains("j-b"));
    }

    #[test]
    fn test_get_reachable_tickets_spawn_only() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-parent".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-parent")),
                deps: vec![],
                spawned_from: None,
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-child".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-child")),
                deps: vec![],
                spawned_from: Some(TicketId::new_unchecked("j-parent")),
                ..Default::default()
            },
        );

        let result =
            get_reachable_tickets("j-child", &ticket_map, RelationshipFilter::Spawn).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains("j-parent"));
        assert!(result.contains("j-child"));
    }

    #[test]
    fn test_get_reachable_tickets_with_cycles() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-a")),
                deps: vec!["j-b".to_string()],
                spawned_from: None,
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-b".to_string(),
            TicketMetadata {
                id: Some(TicketId::new_unchecked("j-b")),
                deps: vec!["j-a".to_string()],
                spawned_from: None,
                ..Default::default()
            },
        );

        let result = get_reachable_tickets("j-a", &ticket_map, RelationshipFilter::Deps).unwrap();
        assert_eq!(result.len(), 2);
    }
}
