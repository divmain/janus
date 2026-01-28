use std::collections::{HashMap, HashSet};

use super::{
    CommandOutput, FormatOptions, format_deps, format_ticket_line, get_next_items_phased,
    get_next_items_simple, sort_tickets_by, ticket_to_json,
};
use crate::error::{JanusError, Result};
use crate::plan::Plan;
use crate::ticket::{build_ticket_map, find_ticket_by_id, get_all_tickets_with_map};
use crate::types::{TicketMetadata, TicketStatus};

/// Options for spawning-related filters
#[derive(Default)]
struct SpawningFilters<'a> {
    /// Filter by parent ticket ID (spawned_from field must match)
    spawned_from: Option<&'a str>,
    /// Filter by exact depth
    depth: Option<u32>,
    /// Filter by maximum depth
    max_depth: Option<u32>,
}

/// Engine for filtering tickets based on dependency status
struct TicketFilterEngine<'a> {
    ticket_map: &'a HashMap<String, TicketMetadata>,
}

impl<'a> TicketFilterEngine<'a> {
    fn new(ticket_map: &'a HashMap<String, TicketMetadata>) -> Self {
        Self { ticket_map }
    }

    /// Check if a ticket is "ready" - has New/Next status and all deps are complete
    fn is_ready(&self, ticket: &TicketMetadata) -> bool {
        if !matches!(
            ticket.status,
            Some(TicketStatus::New) | Some(TicketStatus::Next)
        ) {
            return false;
        }
        // All deps must be complete
        ticket.deps.iter().all(|dep_id| {
            self.ticket_map
                .get(dep_id)
                .map(|dep| dep.status == Some(TicketStatus::Complete))
                .unwrap_or(false)
        })
    }

    /// Check if a ticket is "blocked" - has New/Next status, has deps, and any dep is incomplete
    fn is_blocked(&self, ticket: &TicketMetadata) -> bool {
        if !matches!(
            ticket.status,
            Some(TicketStatus::New) | Some(TicketStatus::Next)
        ) {
            return false;
        }
        // Must have deps
        if ticket.deps.is_empty() {
            return false;
        }
        // Check if any dep is incomplete
        ticket.deps.iter().any(|dep_id| {
            self.ticket_map
                .get(dep_id)
                .map(|dep| dep.status != Some(TicketStatus::Complete))
                .unwrap_or(true)
        })
    }
}

/// List all tickets, optionally filtered by status or other criteria
#[allow(clippy::too_many_arguments)]
pub async fn cmd_ls(
    filter_ready: bool,
    filter_blocked: bool,
    filter_closed: bool,
    include_all: bool,
    status_filter: Option<&str>,
    spawned_from: Option<&str>,
    depth: Option<u32>,
    max_depth: Option<u32>,
    next_in_plan: Option<&str>,
    phase: Option<u32>,
    triaged: Option<&str>,
    limit: Option<usize>,
    sort_by: &str,
    output_json: bool,
) -> Result<()> {
    // Handle --next-in-plan filter specially as it uses different logic
    if let Some(plan_id) = next_in_plan {
        // --phase cannot be used with --next-in-plan
        if phase.is_some() {
            return Err(JanusError::Other(
                "--phase cannot be used with --next-in-plan".to_string(),
            ));
        }
        return cmd_ls_next_in_plan(plan_id, limit, sort_by, output_json).await;
    }

    let (tickets, ticket_map) = get_all_tickets_with_map().await?;

    // Resolve spawned_from partial ID to full ID if provided
    let resolved_spawned_from = if let Some(partial_id) = spawned_from {
        let ticket_path = find_ticket_by_id(partial_id).await?;
        Some(
            ticket_path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| JanusError::InvalidFormat("Invalid ticket path".to_string()))?
                .to_string(),
        )
    } else {
        None
    };

    // Build spawning filters
    let spawning_filters = SpawningFilters {
        spawned_from: resolved_spawned_from.as_deref(),
        depth,
        max_depth,
    };

    // Create filter engine for dependency-based filtering
    let filter_engine = TicketFilterEngine::new(&ticket_map);

    let filtered: Vec<TicketMetadata> = tickets
        .iter()
        .filter(|t| {
            // Apply spawning filters first (these are AND conditions)
            if !matches_spawning_filters(t, &spawning_filters) {
                return false;
            }

            // Filter by triaged status if specified
            // Treat triaged: None as false for backward compatibility
            if let Some(filter_triaged) = triaged {
                let ticket_triaged = t.triaged.unwrap_or(false);
                let filter_value = filter_triaged == "true";
                if ticket_triaged != filter_value {
                    return false;
                }
            }

            // Check if we should include closed/cancelled tickets
            let is_closed = matches!(
                t.status,
                Some(TicketStatus::Complete) | Some(TicketStatus::Cancelled)
            );

            // --status flag is mutually exclusive with --ready, --blocked, --closed
            // (enforced by clap's conflicts_with_all in main.rs)
            if let Some(filter) = status_filter {
                let ticket_status = match t.status {
                    Some(status) => status.to_string(),
                    None => {
                        eprintln!(
                            "Warning: ticket '{}' has missing status field, treating as 'new'",
                            t.id.as_deref().unwrap_or("unknown")
                        );
                        TicketStatus::New.to_string()
                    }
                };
                return ticket_status == filter;
            }

            // Calculate individual filter results using the filter engine
            let is_ready = filter_ready && filter_engine.is_ready(t);
            let is_blocked = filter_blocked && filter_engine.is_blocked(t);

            // Calculate final result based on filter combination
            if filter_ready || filter_blocked || filter_closed {
                // At least one special filter is active - use union behavior
                is_ready || is_blocked || is_closed
            } else {
                // No special filters - apply default behavior
                // Exclude closed tickets unless --all is set
                !is_closed || include_all
            }
        })
        .cloned()
        .collect();

    // Sort by priority then apply limit if specified
    let mut display_tickets = filtered;
    sort_tickets_by(&mut display_tickets, sort_by);

    // Apply limit (unlimited if not specified)
    if let Some(limit) = limit
        && limit < display_tickets.len()
    {
        display_tickets.truncate(limit);
    }

    let json_tickets: Vec<_> = display_tickets.iter().map(ticket_to_json).collect();

    // Build text output eagerly
    let text_output = display_tickets
        .iter()
        .map(|t| {
            let opts = FormatOptions {
                suffix: Some(format_deps(&t.deps)),
                ..Default::default()
            };
            format_ticket_line(t, opts)
        })
        .collect::<Vec<_>>()
        .join("\n");

    CommandOutput::new(serde_json::Value::Array(json_tickets))
        .with_text(text_output)
        .print(output_json)
}

/// Check if a ticket matches the spawning filters
fn matches_spawning_filters(ticket: &TicketMetadata, filters: &SpawningFilters) -> bool {
    // Filter by spawned_from (direct children only)
    if let Some(parent_id) = filters.spawned_from {
        match &ticket.spawned_from {
            Some(spawned_from) if spawned_from == parent_id => {}
            _ => return false,
        }
    }

    // Filter by exact depth
    if let Some(target_depth) = filters.depth {
        // depth 0 means root tickets (no spawned_from OR explicit depth: 0)
        let ticket_depth = ticket.depth.unwrap_or_else(|| {
            // If no explicit depth, infer: if no spawned_from, it's depth 0
            if ticket.spawned_from.is_none() { 0 } else { 1 }
        });
        if ticket_depth != target_depth {
            return false;
        }
    }

    // Filter by max depth
    if let Some(max) = filters.max_depth {
        let ticket_depth = ticket
            .depth
            .unwrap_or_else(|| if ticket.spawned_from.is_none() { 0 } else { 1 });
        if ticket_depth > max {
            return false;
        }
    }

    true
}

/// Handle --next-in-plan filter using plan next logic
async fn cmd_ls_next_in_plan(
    plan_id: &str,
    limit: Option<usize>,
    sort_by: &str,
    output_json: bool,
) -> Result<()> {
    let plan = Plan::find(plan_id).await?;
    let metadata = plan.read()?;
    let ticket_map = build_ticket_map().await?;

    // Use a large count to get all next items, then apply limit
    let count = limit.unwrap_or(usize::MAX);

    // Collect next items based on plan type
    let next_items = if metadata.is_phased() {
        // Get next items from all incomplete phases
        get_next_items_phased(&metadata, &ticket_map, false, true, count)
    } else {
        get_next_items_simple(&metadata, &ticket_map, count)
    };

    // Collect all ticket IDs from next items
    let mut next_ticket_ids: HashSet<String> = HashSet::new();
    for item in &next_items {
        for (ticket_id, _) in &item.tickets {
            next_ticket_ids.insert(ticket_id.clone());
        }
    }

    // Get the full ticket metadata for each next ticket
    let mut display_tickets: Vec<TicketMetadata> = next_ticket_ids
        .iter()
        .filter_map(|id| ticket_map.get(id).cloned())
        .collect();

    // Sort by priority
    sort_tickets_by(&mut display_tickets, sort_by);

    // Apply limit
    if let Some(limit) = limit {
        display_tickets.truncate(limit);
    }

    let json_tickets: Vec<_> = display_tickets.iter().map(ticket_to_json).collect();

    // Build text output
    let text_output = display_tickets
        .iter()
        .map(|t| {
            let opts = FormatOptions {
                suffix: Some(format_deps(&t.deps)),
                ..Default::default()
            };
            format_ticket_line(t, opts)
        })
        .collect::<Vec<_>>()
        .join("\n");

    CommandOutput::new(serde_json::Value::Array(json_tickets))
        .with_text(text_output)
        .print(output_json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ticket(id: &str, spawned_from: Option<&str>, depth: Option<u32>) -> TicketMetadata {
        TicketMetadata {
            id: Some(id.to_string()),
            spawned_from: spawned_from.map(|s| s.to_string()),
            depth,
            ..Default::default()
        }
    }

    #[test]
    fn test_matches_spawning_filters_spawned_from() {
        let ticket = make_ticket("child-1", Some("parent-1"), Some(1));
        let filters = SpawningFilters {
            spawned_from: Some("parent-1"),
            ..Default::default()
        };
        assert!(matches_spawning_filters(&ticket, &filters));

        let filters_wrong_parent = SpawningFilters {
            spawned_from: Some("parent-2"),
            ..Default::default()
        };
        assert!(!matches_spawning_filters(&ticket, &filters_wrong_parent));

        // Root ticket should not match spawned_from filter
        let root = make_ticket("root-1", None, None);
        let filters_parent = SpawningFilters {
            spawned_from: Some("parent-1"),
            ..Default::default()
        };
        assert!(!matches_spawning_filters(&root, &filters_parent));
    }

    #[test]
    fn test_matches_spawning_filters_depth_exact() {
        // Root ticket (no spawned_from, no depth) should match depth 0
        let root = make_ticket("root-1", None, None);
        let filters_depth_0 = SpawningFilters {
            depth: Some(0),
            ..Default::default()
        };
        assert!(matches_spawning_filters(&root, &filters_depth_0));

        // Root ticket should not match depth 1
        let filters_depth_1 = SpawningFilters {
            depth: Some(1),
            ..Default::default()
        };
        assert!(!matches_spawning_filters(&root, &filters_depth_1));

        // Child with explicit depth 1 should match depth 1
        let child = make_ticket("child-1", Some("root-1"), Some(1));
        assert!(matches_spawning_filters(&child, &filters_depth_1));
        assert!(!matches_spawning_filters(&child, &filters_depth_0));

        // Child with explicit depth 0 (unusual but valid) should match depth 0
        let explicit_root = make_ticket("explicit-root", None, Some(0));
        assert!(matches_spawning_filters(&explicit_root, &filters_depth_0));
    }

    #[test]
    fn test_matches_spawning_filters_max_depth() {
        let root = make_ticket("root-1", None, None);
        let child = make_ticket("child-1", Some("root-1"), Some(1));
        let grandchild = make_ticket("grandchild-1", Some("child-1"), Some(2));

        let filters_max_1 = SpawningFilters {
            max_depth: Some(1),
            ..Default::default()
        };

        assert!(matches_spawning_filters(&root, &filters_max_1));
        assert!(matches_spawning_filters(&child, &filters_max_1));
        assert!(!matches_spawning_filters(&grandchild, &filters_max_1));

        let filters_max_0 = SpawningFilters {
            max_depth: Some(0),
            ..Default::default()
        };
        assert!(matches_spawning_filters(&root, &filters_max_0));
        assert!(!matches_spawning_filters(&child, &filters_max_0));
    }

    #[test]
    fn test_matches_spawning_filters_no_filters() {
        let root = make_ticket("root-1", None, None);
        let child = make_ticket("child-1", Some("root-1"), Some(1));

        let no_filters = SpawningFilters::default();

        assert!(matches_spawning_filters(&root, &no_filters));
        assert!(matches_spawning_filters(&child, &no_filters));
    }

    #[test]
    fn test_matches_spawning_filters_combined() {
        let child = make_ticket("child-1", Some("parent-1"), Some(1));

        // Should match: spawned_from matches AND depth matches
        let filters = SpawningFilters {
            spawned_from: Some("parent-1"),
            depth: Some(1),
            ..Default::default()
        };
        assert!(matches_spawning_filters(&child, &filters));

        // Should not match: spawned_from matches but depth doesn't
        let filters_wrong_depth = SpawningFilters {
            spawned_from: Some("parent-1"),
            depth: Some(2),
            ..Default::default()
        };
        assert!(!matches_spawning_filters(&child, &filters_wrong_depth));
    }
}
