use std::collections::HashSet;
use std::fmt::Write;

use super::{
    CommandOutput, FormatOptions, format_deps, format_ticket_line, get_next_items_phased,
    get_next_items_simple, sort_tickets_by, ticket_to_json,
};
use crate::error::{JanusError, Result};
use crate::plan::Plan;
use crate::query::{
    ActiveFilter, ClosedFilter, SizeFilter, SpawningFilter, StatusFilter, TicketFilter,
    TicketQueryBuilder, TriagedFilter,
};
use crate::ticket::{build_ticket_map, find_ticket_by_id, get_all_tickets_with_map};
use crate::types::{TicketMetadata, TicketSize};

/// Formats a list of tickets for output, handling both JSON and text formats.
/// This helper consolidates the common output formatting logic used by listing commands.
fn format_ticket_list(display_tickets: &[TicketMetadata], output_json: bool) -> Result<()> {
    let json_tickets: Vec<_> = display_tickets.iter().map(ticket_to_json).collect();

    // Build text output incrementally to avoid intermediate allocations
    let mut text_output = String::new();
    for (i, t) in display_tickets.iter().enumerate() {
        let opts = FormatOptions {
            suffix: Some(format_deps(&t.deps)),
            ..Default::default()
        };
        if i > 0 {
            writeln!(text_output).unwrap();
        }
        write!(text_output, "{}", format_ticket_line(t, opts)).unwrap();
    }

    CommandOutput::new(serde_json::Value::Array(json_tickets))
        .with_text(text_output)
        .print(output_json)
}

/// List all tickets, optionally filtered by status or other criteria
#[allow(clippy::too_many_arguments)]
pub async fn cmd_ls(
    filter_ready: bool,
    filter_blocked: bool,
    filter_closed: bool,
    filter_active: bool,
    status_filter: Option<&str>,
    spawned_from: Option<&str>,
    depth: Option<u32>,
    max_depth: Option<u32>,
    next_in_plan: Option<&str>,
    phase: Option<u32>,
    triaged: Option<&str>,
    size_filter: Option<Vec<TicketSize>>,
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

    let (tickets, _ticket_map) = get_all_tickets_with_map().await?;

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

    // Build query using TicketQueryBuilder
    let mut builder = TicketQueryBuilder::new().with_sort(sort_by);

    // Add spawning filter if any spawning criteria are specified
    if resolved_spawned_from.is_some() || depth.is_some() || max_depth.is_some() {
        builder = builder.with_filter(Box::new(SpawningFilter::new(
            resolved_spawned_from.as_deref(),
            depth,
            max_depth,
        )));
    }

    // Add triaged filter if specified
    if let Some(triaged_value) = triaged {
        let filter_value = triaged_value == "true";
        builder = builder.with_filter(Box::new(TriagedFilter::new(filter_value)));
    }

    // Add size filter if specified
    if let Some(sizes) = size_filter {
        builder = builder.with_filter(Box::new(SizeFilter::new(sizes)));
    }

    // Add status-based filters
    if let Some(status) = status_filter {
        // --status flag is mutually exclusive with --ready, --blocked, --closed
        builder = builder.with_filter(Box::new(StatusFilter::new(status)));
    } else if filter_ready || filter_blocked || filter_closed || filter_active {
        // Union behavior: combine filters with OR logic
        // We use a custom approach here since filters are normally AND-based
        // For union filters, we need to handle them specially
        let (ready_tickets, blocked_tickets, closed_tickets, active_tickets) =
            if filter_ready || filter_blocked {
                // We need to compute these using the context
                use crate::query::{BlockedFilter, ReadyFilter, TicketFilterContext};
                let context = TicketFilterContext::new().await?;

                let ready: Vec<_> = if filter_ready {
                    tickets
                        .iter()
                        .filter(|t| ReadyFilter.matches(t, &context))
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                };

                let blocked: Vec<_> = if filter_blocked {
                    tickets
                        .iter()
                        .filter(|t| BlockedFilter.matches(t, &context))
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                };

                let closed: Vec<_> = if filter_closed {
                    tickets
                        .iter()
                        .filter(|t| ClosedFilter.matches(t, &context))
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                };

                let active: Vec<_> = if filter_active {
                    tickets
                        .iter()
                        .filter(|t| ActiveFilter.matches(t, &context))
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                };

                (ready, blocked, closed, active)
            } else {
                // Only closed/active filters, no need for complex context
                use crate::query::TicketFilterContext;
                let context = TicketFilterContext::new().await?;

                let closed: Vec<_> = if filter_closed {
                    tickets
                        .iter()
                        .filter(|t| ClosedFilter.matches(t, &context))
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                };

                let active: Vec<_> = if filter_active {
                    tickets
                        .iter()
                        .filter(|t| ActiveFilter.matches(t, &context))
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                };

                (Vec::new(), Vec::new(), closed, active)
            };

        // Combine all matching tickets (union)
        let mut display_tickets: Vec<TicketMetadata> = ready_tickets;
        display_tickets.extend(blocked_tickets);
        display_tickets.extend(closed_tickets);
        display_tickets.extend(active_tickets);

        // Remove duplicates
        display_tickets.sort_by(|a, b| a.id.cmp(&b.id));
        display_tickets.dedup_by(|a, b| a.id == b.id);

        // Sort and apply limit
        sort_tickets_by(&mut display_tickets, sort_by);
        if let Some(limit) = limit {
            if limit < display_tickets.len() {
                display_tickets.truncate(limit);
            }
        }

        return format_ticket_list(&display_tickets, output_json);
    } else {
        // Default: exclude closed tickets (use ActiveFilter as the base)
        builder = builder.with_filter(Box::new(ActiveFilter));
    }

    // Apply limit if specified
    if let Some(lim) = limit {
        builder = builder.with_limit(lim);
    }

    // Execute the query
    let display_tickets = builder.execute(tickets).await?;
    format_ticket_list(&display_tickets, output_json)
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

    format_ticket_list(&display_tickets, output_json)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;
    use crate::query::{SpawningFilter, TicketFilter, TicketFilterContext};

    fn make_ticket(id: &str, spawned_from: Option<&str>, depth: Option<u32>) -> TicketMetadata {
        TicketMetadata {
            id: Some(id.to_string()),
            spawned_from: spawned_from.map(|s| s.to_string()),
            depth,
            ..Default::default()
        }
    }

    fn empty_context() -> TicketFilterContext {
        TicketFilterContext {
            ticket_map: HashMap::new(),
            warned_dangling: RefCell::new(HashSet::new()),
        }
    }

    #[test]
    fn test_spawning_filter_spawned_from() {
        let context = empty_context();
        let ticket = make_ticket("child-1", Some("parent-1"), Some(1));
        let filter = SpawningFilter::new(Some("parent-1"), None, None);
        assert!(filter.matches(&ticket, &context));

        let filter_wrong_parent = SpawningFilter::new(Some("parent-2"), None, None);
        assert!(!filter_wrong_parent.matches(&ticket, &context));

        // Root ticket should not match spawned_from filter
        let root = make_ticket("root-1", None, None);
        let filter_parent = SpawningFilter::new(Some("parent-1"), None, None);
        assert!(!filter_parent.matches(&root, &context));
    }

    #[test]
    fn test_spawning_filter_depth_exact() {
        let context = empty_context();
        // Root ticket (no spawned_from, no depth) should match depth 0
        let root = make_ticket("root-1", None, None);
        let filter_depth_0 = SpawningFilter::new(None, Some(0), None);
        assert!(filter_depth_0.matches(&root, &context));

        // Root ticket should not match depth 1
        let filter_depth_1 = SpawningFilter::new(None, Some(1), None);
        assert!(!filter_depth_1.matches(&root, &context));

        // Child with explicit depth 1 should match depth 1
        let child = make_ticket("child-1", Some("root-1"), Some(1));
        assert!(filter_depth_1.matches(&child, &context));
        assert!(!filter_depth_0.matches(&child, &context));

        // Child with explicit depth 0 (unusual but valid) should match depth 0
        let explicit_root = make_ticket("explicit-root", None, Some(0));
        assert!(filter_depth_0.matches(&explicit_root, &context));
    }

    #[test]
    fn test_spawning_filter_max_depth() {
        let context = empty_context();
        let root = make_ticket("root-1", None, None);
        let child = make_ticket("child-1", Some("root-1"), Some(1));
        let grandchild = make_ticket("grandchild-1", Some("child-1"), Some(2));

        let filter_max_1 = SpawningFilter::new(None, None, Some(1));

        assert!(filter_max_1.matches(&root, &context));
        assert!(filter_max_1.matches(&child, &context));
        assert!(!filter_max_1.matches(&grandchild, &context));

        let filter_max_0 = SpawningFilter::new(None, None, Some(0));
        assert!(filter_max_0.matches(&root, &context));
        assert!(!filter_max_0.matches(&child, &context));
    }

    #[test]
    fn test_spawning_filter_no_filters() {
        let context = empty_context();
        let root = make_ticket("root-1", None, None);
        let child = make_ticket("child-1", Some("root-1"), Some(1));

        let no_filter = SpawningFilter::new(None, None, None);

        assert!(no_filter.matches(&root, &context));
        assert!(no_filter.matches(&child, &context));
    }

    #[test]
    fn test_spawning_filter_combined() {
        let context = empty_context();
        let child = make_ticket("child-1", Some("parent-1"), Some(1));

        // Should match: spawned_from matches AND depth matches
        let filter = SpawningFilter::new(Some("parent-1"), Some(1), None);
        assert!(filter.matches(&child, &context));

        // Should not match: spawned_from matches but depth doesn't
        let filter_wrong_depth = SpawningFilter::new(Some("parent-1"), Some(2), None);
        assert!(!filter_wrong_depth.matches(&child, &context));
    }
}
