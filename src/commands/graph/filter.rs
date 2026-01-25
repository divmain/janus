//! Graph filtering logic - BFS traversal and relationship filtering

use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::Result;
use crate::plan::Plan;
use crate::ticket::resolve_id_partial;
use crate::types::TicketMetadata;

use super::types::RelationshipFilter;

/// Get all tickets reachable from a root ticket via relationships
pub fn get_reachable_tickets(
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
            if filter != RelationshipFilter::Spawn {
                for dep in &ticket.deps {
                    if !visited.contains(dep) {
                        queue.push_back(dep.clone());
                    }
                }
            }

            if filter != RelationshipFilter::Deps
                && let Some(parent) = &ticket.spawned_from
                && !visited.contains(parent)
            {
                queue.push_back(parent.clone());
            }
        }

        for (id, other_ticket) in ticket_map {
            if visited.contains(id) {
                continue;
            }

            if filter != RelationshipFilter::Spawn && other_ticket.deps.contains(&current) {
                queue.push_back(id.clone());
            }

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

    #[test]
    fn test_get_reachable_tickets_basic() {
        let mut ticket_map = HashMap::new();
        ticket_map.insert(
            "j-a".to_string(),
            TicketMetadata {
                id: Some("j-a".to_string()),
                deps: vec!["j-b".to_string()],
                spawned_from: None,
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-b".to_string(),
            TicketMetadata {
                id: Some("j-b".to_string()),
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
                id: Some("j-parent".to_string()),
                deps: vec![],
                spawned_from: None,
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-child".to_string(),
            TicketMetadata {
                id: Some("j-child".to_string()),
                deps: vec![],
                spawned_from: Some("j-parent".to_string()),
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
                id: Some("j-a".to_string()),
                deps: vec!["j-b".to_string()],
                spawned_from: None,
                ..Default::default()
            },
        );
        ticket_map.insert(
            "j-b".to_string(),
            TicketMetadata {
                id: Some("j-b".to_string()),
                deps: vec!["j-a".to_string()],
                spawned_from: None,
                ..Default::default()
            },
        );

        let result = get_reachable_tickets("j-a", &ticket_map, RelationshipFilter::Deps).unwrap();
        assert_eq!(result.len(), 2);
    }
}
