//! Core algorithm for the `janus next` command.
//!
//! This module provides the `NextWorkFinder` which computes the optimal work queue
//! based on ticket priorities, dependencies, and status.

use std::collections::{HashMap, HashSet};

use crate::error::Result;
use crate::status::all_deps_satisfied;
use crate::types::{TicketMetadata, TicketStatus};

/// Reason why a ticket is included in the next work queue
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InclusionReason {
    /// This ticket is ready and high priority
    Ready,
    /// This ticket blocks another ticket (include the blocked ticket's ID)
    Blocking(String),
    /// This ticket is the target but currently blocked
    TargetBlocked,
}

/// A ticket in the work queue with context about why it's included
#[derive(Debug, Clone)]
pub struct WorkItem {
    pub ticket_id: String,
    pub metadata: TicketMetadata,
    pub reason: InclusionReason,
    pub blocks: Option<String>,
}

/// Core algorithm for finding the next work items
pub struct NextWorkFinder<'a> {
    ticket_map: &'a HashMap<String, TicketMetadata>,
}

impl<'a> NextWorkFinder<'a> {
    /// Create a new NextWorkFinder with a reference to the ticket map
    pub fn new(ticket_map: &'a HashMap<String, TicketMetadata>) -> Self {
        Self { ticket_map }
    }

    /// Get the next work items up to the specified limit
    ///
    /// The algorithm:
    /// 1. Get all workable tickets (status new or next)
    /// 2. Separate into ready (no incomplete deps) and blocked
    /// 3. For each blocked ticket in priority order (shorter chains first):
    ///    - Find ready dependencies via DFS
    ///    - Add them with Blocking reason (pointing to this blocked ticket)
    ///    - Add the blocked ticket with TargetBlocked reason
    /// 4. For remaining ready tickets not already added:
    ///    - Add with Ready reason
    /// 5. Use visited set to avoid duplicates
    /// 6. Truncate to limit
    pub fn get_next_work(&self, limit: usize) -> Result<Vec<WorkItem>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let workable = self.get_workable_tickets();
        if workable.is_empty() {
            return Ok(Vec::new());
        }

        let mut result = Vec::new();
        let mut visited = HashSet::new();

        // Process blocked tickets first to ensure their dependencies are marked as Blocking
        // Sort blocked tickets by dependency depth (shorter chains first) so that
        // dependencies are processed before dependents
        let mut blocked_tickets: Vec<&TicketMetadata> = workable
            .iter()
            .filter(|t| !self.is_ready(t))
            .copied()
            .collect();

        // Sort by dependency depth (shorter chains first), then priority, then date
        blocked_tickets.sort_by(|a, b| {
            let depth_a = self.dependency_depth(a);
            let depth_b = self.dependency_depth(b);
            let depth_cmp = depth_a.cmp(&depth_b);
            if depth_cmp != std::cmp::Ordering::Equal {
                return depth_cmp;
            }
            let priority_cmp = a.priority_num().cmp(&b.priority_num());
            if priority_cmp != std::cmp::Ordering::Equal {
                return priority_cmp;
            }
            a.created.cmp(&b.created)
        });

        for ticket in blocked_tickets {
            if result.len() >= limit {
                break;
            }

            let ticket_id = ticket
                .id
                .as_ref()
                .map(|id| id.to_string())
                .unwrap_or_default();

            // Skip if already visited (might have been added as a dependency of another blocked ticket)
            if visited.contains(&ticket_id) {
                continue;
            }

            // Check for circular dependencies
            if let Some(cycle) = self.detect_cycle(&ticket_id) {
                eprintln!(
                    "Warning: circular dependency detected for ticket '{}': {}",
                    ticket_id,
                    cycle.join(" -> ")
                );
                continue;
            }

            // Collect ready dependencies for this ticket
            let deps = self.collect_all_ready_deps(ticket, &ticket_id, &visited);

            // Add ready dependencies with Blocking reason
            for (dep_id, target_id) in deps {
                if result.len() >= limit {
                    break;
                }

                if visited.contains(&dep_id) {
                    continue;
                }

                if let Some(dep_metadata) = self.ticket_map.get(&dep_id) {
                    visited.insert(dep_id.clone());
                    result.push(WorkItem {
                        ticket_id: dep_id.clone(),
                        metadata: dep_metadata.clone(),
                        reason: InclusionReason::Blocking(target_id),
                        blocks: Some(ticket_id.clone()),
                    });
                }
            }

            // Add the blocked target ticket
            if result.len() < limit && !visited.contains(&ticket_id) {
                visited.insert(ticket_id.clone());
                result.push(WorkItem {
                    ticket_id: ticket_id.clone(),
                    metadata: ticket.clone(),
                    reason: InclusionReason::TargetBlocked,
                    blocks: None,
                });
            }
        }

        // Add remaining ready tickets that aren't dependencies of blocked tickets
        for ticket in workable {
            if result.len() >= limit {
                break;
            }

            let ticket_id = ticket
                .id
                .as_ref()
                .map(|id| id.to_string())
                .unwrap_or_default();

            if visited.contains(&ticket_id) {
                continue;
            }

            if !self.is_ready(ticket) {
                continue; // Skip blocked tickets (already handled)
            }

            // This is a ready ticket that's not a dependency of any blocked ticket
            visited.insert(ticket_id.clone());
            result.push(WorkItem {
                ticket_id: ticket_id.clone(),
                metadata: ticket.clone(),
                reason: InclusionReason::Ready,
                blocks: None,
            });
        }

        Ok(result)
    }

    /// Get all workable tickets (status new or next)
    fn get_workable_tickets(&self) -> Vec<&TicketMetadata> {
        let mut workable: Vec<&TicketMetadata> = self
            .ticket_map
            .values()
            .filter(|t| matches!(t.status, Some(TicketStatus::New) | Some(TicketStatus::Next)))
            .collect();

        // Sort by priority (lower number = higher priority), then by created date
        workable.sort_by(|a, b| {
            let priority_cmp = a.priority_num().cmp(&b.priority_num());
            if priority_cmp != std::cmp::Ordering::Equal {
                return priority_cmp;
            }
            // If priorities are equal, sort by created date (older first)
            a.created.cmp(&b.created)
        });

        workable
    }

    /// Compute the dependency depth for a ticket (length of longest dependency chain)
    /// A ready ticket has depth 0, a ticket depending on a ready ticket has depth 1, etc.
    fn dependency_depth(&self, ticket: &TicketMetadata) -> usize {
        let mut max_depth = 0;
        let mut stack = vec![(
            ticket
                .id
                .as_ref()
                .map(|id| id.to_string())
                .unwrap_or_default(),
            0,
        )];
        let mut visited = HashSet::new();

        while let Some((current_id, current_depth)) = stack.pop() {
            if visited.contains(&current_id) {
                continue;
            }
            visited.insert(current_id.clone());

            max_depth = max_depth.max(current_depth);

            if let Some(current_ticket) = self.ticket_map.get(&current_id) {
                for dep_id in &current_ticket.deps {
                    if let Some(dep) = self.ticket_map.get(dep_id)
                        && !self.is_ready(dep)
                    {
                        // This dependency is also blocked, recurse
                        stack.push((dep_id.clone(), current_depth + 1));
                    }
                }
            }
        }

        max_depth
    }

    /// Collect all ready dependencies for a ticket (recursive)
    /// Returns a vector of (dep_id, target_id) pairs where target_id is the ultimate
    /// blocked ticket that dep_id is blocking.
    fn collect_all_ready_deps(
        &self,
        ticket: &TicketMetadata,
        target_id: &str,
        visited: &HashSet<String>,
    ) -> Vec<(String, String)> {
        let mut result = Vec::new();
        let mut stack = vec![
            ticket
                .id
                .as_ref()
                .map(|id| id.to_string())
                .unwrap_or_default(),
        ];
        let mut local_visited = HashSet::new();

        while let Some(current_id) = stack.pop() {
            if local_visited.contains(&current_id) {
                continue;
            }
            local_visited.insert(current_id.clone());

            if let Some(current_ticket) = self.ticket_map.get(&current_id) {
                for dep_id in &current_ticket.deps {
                    // Skip if already in result or globally visited
                    if result.iter().any(|(id, _)| id == dep_id)
                        || visited.contains(dep_id)
                        || local_visited.contains(dep_id)
                    {
                        continue;
                    }

                    if let Some(dep) = self.ticket_map.get(dep_id) {
                        if self.is_ready(dep) {
                            // This dependency is ready - add to result with the original target
                            result.push((dep_id.clone(), target_id.to_string()));
                        } else if matches!(
                            dep.status,
                            Some(TicketStatus::New) | Some(TicketStatus::Next)
                        ) {
                            // This dependency is also workable but blocked - recurse
                            stack.push(dep_id.clone());
                        }
                        // If dep is Complete, Cancelled, or InProgress, skip it
                    }
                    // Orphan dependency - nothing to traverse or add
                }
            }
        }

        // Sort the result by priority and created date for consistent ordering
        result.sort_by(|(a_id, _), (b_id, _)| {
            let a_meta = self.ticket_map.get(a_id).unwrap();
            let b_meta = self.ticket_map.get(b_id).unwrap();
            let priority_cmp = a_meta.priority_num().cmp(&b_meta.priority_num());
            if priority_cmp != std::cmp::Ordering::Equal {
                return priority_cmp;
            }
            a_meta.created.cmp(&b_meta.created)
        });

        result
    }

    /// Check if a ticket is ready (all dependencies are satisfied AND ticket is workable)
    fn is_ready(&self, ticket: &TicketMetadata) -> bool {
        // Ticket must have workable status (New or Next)
        let is_workable = matches!(
            ticket.status,
            Some(TicketStatus::New) | Some(TicketStatus::Next)
        );

        if !is_workable {
            return false;
        }

        // All dependencies must be satisfied (terminal status; orphans block)
        all_deps_satisfied(ticket, self.ticket_map)
    }

    /// Detect if a ticket is part of a circular dependency
    /// Returns Some(cycle_path) if a cycle is detected, None otherwise
    fn detect_cycle(&self, ticket_id: &str) -> Option<Vec<String>> {
        let mut visited = HashSet::new();
        let mut path = Vec::new();

        self.detect_cycle_dfs(ticket_id, &mut visited, &mut path)
    }

    fn detect_cycle_dfs(
        &self,
        ticket_id: &str,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        if path.contains(&ticket_id.to_string()) {
            // Found a cycle - extract the cycle from the path
            let cycle_start = path.iter().position(|p| p == ticket_id).unwrap();
            let mut cycle = path[cycle_start..].to_vec();
            cycle.push(ticket_id.to_string());
            return Some(cycle);
        }

        if visited.contains(ticket_id) {
            return None;
        }

        visited.insert(ticket_id.to_string());
        path.push(ticket_id.to_string());

        if let Some(ticket) = self.ticket_map.get(ticket_id) {
            for dep_id in &ticket.deps {
                if let Some(cycle) = self.detect_cycle_dfs(dep_id, visited, path) {
                    return Some(cycle);
                }
            }
        }

        path.pop();
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_ticket(
        id: &str,
        status: TicketStatus,
        priority: u8,
        deps: Vec<&str>,
        created: &str,
    ) -> TicketMetadata {
        let ticket_priority = match priority {
            0 => crate::types::TicketPriority::P0,
            1 => crate::types::TicketPriority::P1,
            2 => crate::types::TicketPriority::P2,
            3 => crate::types::TicketPriority::P3,
            _ => crate::types::TicketPriority::P4,
        };

        TicketMetadata {
            id: Some(crate::types::TicketId::new_unchecked(id)),
            uuid: None,
            title: Some(format!("Ticket {id}")),
            status: Some(status),
            deps: deps.iter().map(|s| s.to_string()).collect(),
            links: Vec::new(),
            created: Some(crate::types::CreatedAt::new_unchecked(created)),
            ticket_type: Some(crate::types::TicketType::Task),
            priority: Some(ticket_priority),
            size: None,
            external_ref: None,
            remote: None,
            parent: None,
            spawned_from: None,
            spawn_context: None,
            depth: None,
            triaged: None,
            file_path: None,
            completion_summary: None,
            body: None,
        }
    }

    #[test]
    fn test_single_ready_ticket() {
        let mut map = HashMap::new();
        map.insert(
            "j-a1b2".to_string(),
            create_test_ticket(
                "j-a1b2",
                TicketStatus::New,
                2,
                vec![],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(10).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].ticket_id, "j-a1b2");
        assert_eq!(result[0].reason, InclusionReason::Ready);
    }

    #[test]
    fn test_priority_ordering() {
        let mut map = HashMap::new();
        map.insert(
            "j-low".to_string(),
            create_test_ticket(
                "j-low",
                TicketStatus::New,
                2,
                vec![],
                "2024-01-01T00:00:00Z",
            ),
        );
        map.insert(
            "j-high".to_string(),
            create_test_ticket(
                "j-high",
                TicketStatus::New,
                0,
                vec![],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(10).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].ticket_id, "j-high"); // P0 first
        assert_eq!(result[1].ticket_id, "j-low"); // P2 second
    }

    #[test]
    fn test_blocked_returns_deps_first() {
        let mut map = HashMap::new();
        map.insert(
            "j-dep".to_string(),
            create_test_ticket(
                "j-dep",
                TicketStatus::New,
                2,
                vec![],
                "2024-01-01T00:00:00Z",
            ),
        );
        map.insert(
            "j-blocked".to_string(),
            create_test_ticket(
                "j-blocked",
                TicketStatus::New,
                2,
                vec!["j-dep"],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(10).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].ticket_id, "j-dep");
        assert_eq!(
            result[0].reason,
            InclusionReason::Blocking("j-blocked".to_string())
        );
        assert_eq!(result[1].ticket_id, "j-blocked");
        assert_eq!(result[1].reason, InclusionReason::TargetBlocked);
    }

    #[test]
    fn test_deep_dependency_chain() {
        let mut map = HashMap::new();
        // Chain: j-c depends on j-b, j-b depends on j-a
        map.insert(
            "j-a".to_string(),
            create_test_ticket("j-a", TicketStatus::New, 2, vec![], "2024-01-01T00:00:00Z"),
        );
        map.insert(
            "j-b".to_string(),
            create_test_ticket(
                "j-b",
                TicketStatus::New,
                2,
                vec!["j-a"],
                "2024-01-01T00:00:00Z",
            ),
        );
        map.insert(
            "j-c".to_string(),
            create_test_ticket(
                "j-c",
                TicketStatus::New,
                2,
                vec!["j-b"],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(10).unwrap();

        assert_eq!(result.len(), 3);
        // Order should be: j-a (ready dep), j-b (blocked, added when processing j-c), j-c (target blocked)
        // OR: j-a (ready dep of j-b), j-b (target blocked), j-c (target blocked, depends on j-b)
        // Actually, with depth sorting: j-b (depth 1) comes before j-c (depth 2)
        // So: j-a (dep of j-b), j-b (target), j-c (target, its dep j-b is already visited)
        assert_eq!(result[0].ticket_id, "j-a");
        assert_eq!(
            result[0].reason,
            InclusionReason::Blocking("j-b".to_string())
        );
        assert_eq!(result[1].ticket_id, "j-b");
        assert_eq!(result[1].reason, InclusionReason::TargetBlocked);
        assert_eq!(result[2].ticket_id, "j-c");
        assert_eq!(result[2].reason, InclusionReason::TargetBlocked);
    }

    #[test]
    fn test_diamond_dependency() {
        let mut map = HashMap::new();
        // Diamond: A depends on B and C, both B and C depend on D
        map.insert(
            "j-d".to_string(),
            create_test_ticket("j-d", TicketStatus::New, 2, vec![], "2024-01-01T00:00:00Z"),
        );
        map.insert(
            "j-b".to_string(),
            create_test_ticket(
                "j-b",
                TicketStatus::New,
                2,
                vec!["j-d"],
                "2024-01-01T00:00:00Z",
            ),
        );
        map.insert(
            "j-c".to_string(),
            create_test_ticket(
                "j-c",
                TicketStatus::New,
                2,
                vec!["j-d"],
                "2024-01-01T00:00:00Z",
            ),
        );
        map.insert(
            "j-a".to_string(),
            create_test_ticket(
                "j-a",
                TicketStatus::New,
                2,
                vec!["j-b", "j-c"],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(10).unwrap();

        // j-d should appear only once, then j-b and j-c (order depends on processing)
        let ids: Vec<String> = result.iter().map(|w| w.ticket_id.clone()).collect();
        assert!(ids.contains(&"j-d".to_string()));
        assert!(ids.contains(&"j-b".to_string()));
        assert!(ids.contains(&"j-c".to_string()));
        assert!(ids.contains(&"j-a".to_string()));

        // j-d should be ready (no deps) - it's added as a dependency of j-b or j-c
        // but since it has no deps of its own, it should be marked as Blocking
        let d_item = result.iter().find(|w| w.ticket_id == "j-d").unwrap();
        assert!(matches!(d_item.reason, InclusionReason::Blocking(_)));
    }

    #[test]
    fn test_circular_deps_handled() {
        let mut map = HashMap::new();
        // Cycle: a -> b -> c -> a
        map.insert(
            "j-a".to_string(),
            create_test_ticket(
                "j-a",
                TicketStatus::New,
                2,
                vec!["j-c"],
                "2024-01-01T00:00:00Z",
            ),
        );
        map.insert(
            "j-b".to_string(),
            create_test_ticket(
                "j-b",
                TicketStatus::New,
                2,
                vec!["j-a"],
                "2024-01-01T00:00:00Z",
            ),
        );
        map.insert(
            "j-c".to_string(),
            create_test_ticket(
                "j-c",
                TicketStatus::New,
                2,
                vec!["j-b"],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(10).unwrap();

        // All tickets in cycle should be skipped
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_limit_parameter() {
        let mut map = HashMap::new();
        for i in 0..10 {
            map.insert(
                format!("j-{i}"),
                create_test_ticket(
                    &format!("j-{i}"),
                    TicketStatus::New,
                    2,
                    vec![],
                    &format!("2024-01-{:02}T00:00:00Z", i + 1),
                ),
            );
        }

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(5).unwrap();

        assert_eq!(result.len(), 5);
    }

    #[test]
    fn test_limit_zero() {
        let mut map = HashMap::new();
        map.insert(
            "j-a1b2".to_string(),
            create_test_ticket(
                "j-a1b2",
                TicketStatus::New,
                2,
                vec![],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(0).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_no_tickets() {
        let map: HashMap<String, TicketMetadata> = HashMap::new();

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(10).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_all_complete_or_cancelled() {
        let mut map = HashMap::new();
        map.insert(
            "j-complete".to_string(),
            create_test_ticket(
                "j-complete",
                TicketStatus::Complete,
                2,
                vec![],
                "2024-01-01T00:00:00Z",
            ),
        );
        map.insert(
            "j-cancelled".to_string(),
            create_test_ticket(
                "j-cancelled",
                TicketStatus::Cancelled,
                2,
                vec![],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(10).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_orphan_dep_treated_as_blocking() {
        let mut map = HashMap::new();
        // Ticket depends on non-existent ticket
        map.insert(
            "j-orphan".to_string(),
            create_test_ticket(
                "j-orphan",
                TicketStatus::New,
                2,
                vec!["j-nonexistent"],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(10).unwrap();

        // Orphan dep should be treated as blocking (safer default)
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].ticket_id, "j-orphan");
        assert_eq!(result[0].reason, InclusionReason::TargetBlocked);
    }

    #[test]
    fn test_cancelled_dep_treated_as_satisfied() {
        let mut map = HashMap::new();
        // Ticket depends on a cancelled ticket — should be ready
        map.insert(
            "j-dep".to_string(),
            create_test_ticket(
                "j-dep",
                TicketStatus::Cancelled,
                2,
                vec![],
                "2024-01-01T00:00:00Z",
            ),
        );
        map.insert(
            "j-ticket".to_string(),
            create_test_ticket(
                "j-ticket",
                TicketStatus::New,
                2,
                vec!["j-dep"],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(10).unwrap();

        // Cancelled dep should be treated as satisfied, so j-ticket is ready
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].ticket_id, "j-ticket");
        assert_eq!(result[0].reason, InclusionReason::Ready);
    }

    #[test]
    fn test_next_status_is_workable() {
        let mut map = HashMap::new();
        map.insert(
            "j-next".to_string(),
            create_test_ticket(
                "j-next",
                TicketStatus::Next,
                2,
                vec![],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(10).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].ticket_id, "j-next");
    }

    #[test]
    fn test_in_progress_not_workable() {
        let mut map = HashMap::new();
        map.insert(
            "j-inprogress".to_string(),
            create_test_ticket(
                "j-inprogress",
                TicketStatus::InProgress,
                2,
                vec![],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(10).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_all_blocked_with_no_ready_deps() {
        let mut map = HashMap::new();
        // a depends on b, where b is in progress (not ready, not workable)
        map.insert(
            "j-a".to_string(),
            create_test_ticket(
                "j-a",
                TicketStatus::New,
                2,
                vec!["j-b"],
                "2024-01-01T00:00:00Z",
            ),
        );
        map.insert(
            "j-b".to_string(),
            create_test_ticket(
                "j-b",
                TicketStatus::InProgress,
                2,
                vec![],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(10).unwrap();

        // j-a is workable but blocked, j-b is not workable (in_progress)
        // Since j-b is not workable and not complete, j-a's dep is not ready
        // j-a will be added as TargetBlocked (no ready deps to add)
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].ticket_id, "j-a");
        assert_eq!(result[0].reason, InclusionReason::TargetBlocked);
    }

    #[test]
    fn test_duplicate_avoidance() {
        let mut map = HashMap::new();
        // Two tickets both depend on the same ticket
        map.insert(
            "j-shared".to_string(),
            create_test_ticket(
                "j-shared",
                TicketStatus::New,
                2,
                vec![],
                "2024-01-01T00:00:00Z",
            ),
        );
        map.insert(
            "j-a".to_string(),
            create_test_ticket(
                "j-a",
                TicketStatus::New,
                2,
                vec!["j-shared"],
                "2024-01-01T00:00:00Z",
            ),
        );
        map.insert(
            "j-b".to_string(),
            create_test_ticket(
                "j-b",
                TicketStatus::New,
                1,
                vec!["j-shared"],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(10).unwrap();

        // j-shared should only appear once
        let shared_count = result.iter().filter(|w| w.ticket_id == "j-shared").count();
        assert_eq!(shared_count, 1);

        // Total should be 3 (j-shared, j-b, j-a) - j-b has higher priority (P1 vs P2)
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_created_date_sorting() {
        let mut map = HashMap::new();
        map.insert(
            "j-newer".to_string(),
            create_test_ticket(
                "j-newer",
                TicketStatus::New,
                2,
                vec![],
                "2024-01-02T00:00:00Z",
            ),
        );
        map.insert(
            "j-older".to_string(),
            create_test_ticket(
                "j-older",
                TicketStatus::New,
                2,
                vec![],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let result = finder.get_next_work(10).unwrap();

        // Same priority, older created date should come first
        assert_eq!(result[0].ticket_id, "j-older");
        assert_eq!(result[1].ticket_id, "j-newer");
    }

    #[test]
    fn test_detect_cycle_simple() {
        let mut map = HashMap::new();
        // a -> b -> c -> a (cycle)
        map.insert(
            "j-a".to_string(),
            create_test_ticket(
                "j-a",
                TicketStatus::New,
                2,
                vec!["j-c"],
                "2024-01-01T00:00:00Z",
            ),
        );
        map.insert(
            "j-b".to_string(),
            create_test_ticket(
                "j-b",
                TicketStatus::New,
                2,
                vec!["j-a"],
                "2024-01-01T00:00:00Z",
            ),
        );
        map.insert(
            "j-c".to_string(),
            create_test_ticket(
                "j-c",
                TicketStatus::New,
                2,
                vec!["j-b"],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let cycle = finder.detect_cycle("j-a");

        assert!(cycle.is_some());
        let cycle_path = cycle.unwrap();
        assert!(cycle_path.contains(&"j-a".to_string()));
        assert!(cycle_path.contains(&"j-b".to_string()));
        assert!(cycle_path.contains(&"j-c".to_string()));
    }

    #[test]
    fn test_detect_cycle_none() {
        let mut map = HashMap::new();
        // a -> b -> c (no cycle)
        map.insert(
            "j-a".to_string(),
            create_test_ticket(
                "j-a",
                TicketStatus::New,
                2,
                vec!["j-b"],
                "2024-01-01T00:00:00Z",
            ),
        );
        map.insert(
            "j-b".to_string(),
            create_test_ticket(
                "j-b",
                TicketStatus::New,
                2,
                vec!["j-c"],
                "2024-01-01T00:00:00Z",
            ),
        );
        map.insert(
            "j-c".to_string(),
            create_test_ticket("j-c", TicketStatus::New, 2, vec![], "2024-01-01T00:00:00Z"),
        );

        let finder = NextWorkFinder::new(&map);
        let cycle = finder.detect_cycle("j-a");

        assert!(cycle.is_none());
    }

    #[test]
    fn test_self_dependency_cycle() {
        let mut map = HashMap::new();
        // a -> a (self cycle)
        map.insert(
            "j-a".to_string(),
            create_test_ticket(
                "j-a",
                TicketStatus::New,
                2,
                vec!["j-a"],
                "2024-01-01T00:00:00Z",
            ),
        );

        let finder = NextWorkFinder::new(&map);
        let cycle = finder.detect_cycle("j-a");

        assert!(cycle.is_some());
    }
}
