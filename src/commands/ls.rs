use std::collections::HashSet;
use std::fmt::Write;

use super::{
    CommandOutput, FormatOptions, format_deps, format_ticket_line, get_next_items_phased,
    get_next_items_simple, ticket_to_json,
};
use crate::error::{JanusError, Result};
use crate::plan::Plan;
use crate::query::{
    ActiveFilter, BlockedFilter, ClosedFilter, ReadyFilter, SizeFilter, SortField, SpawningFilter,
    StatusFilter, TicketQueryBuilder, TriagedFilter,
};
use crate::ticket::{Ticket, build_ticket_map, get_all_tickets_with_map};
use crate::types::{TicketMetadata, TicketSize, TicketStatus};

/// Options for the `ls` command, bundling all filter and display parameters.
pub struct LsOptions {
    pub filter_ready: bool,
    pub filter_blocked: bool,
    pub filter_closed: bool,
    pub filter_active: bool,
    pub status_filter: Option<TicketStatus>,
    pub spawned_from: Option<String>,
    pub depth: Option<u32>,
    pub max_depth: Option<u32>,
    pub next_in_plan: Option<String>,
    pub phase: Option<u32>,
    pub triaged: Option<bool>,
    pub size_filter: Option<Vec<TicketSize>>,
    pub limit: Option<usize>,
    pub sort_by: SortField,
    pub output_json: bool,
}

impl LsOptions {
    /// Create a new LsOptions with sensible defaults.
    pub fn new() -> Self {
        Self {
            filter_ready: false,
            filter_blocked: false,
            filter_closed: false,
            filter_active: false,
            status_filter: None,
            spawned_from: None,
            depth: None,
            max_depth: None,
            next_in_plan: None,
            phase: None,
            triaged: None,
            size_filter: None,
            limit: None,
            sort_by: SortField::default(),
            output_json: false,
        }
    }

    /// Builder method to set status filter from a string.
    /// Returns an error if the string is not a valid status.
    pub fn with_status_str(mut self, status: &str) -> crate::error::Result<Self> {
        self.status_filter = Some(status.parse::<TicketStatus>()?);
        Ok(self)
    }

    /// Builder method to set triaged filter from a string.
    /// Returns an error if the string is not "true" or "false" (case-insensitive).
    pub fn with_triaged_str(mut self, triaged: &str) -> crate::error::Result<Self> {
        match triaged.to_lowercase().as_str() {
            "true" => self.triaged = Some(true),
            "false" => self.triaged = Some(false),
            _ => {
                return Err(JanusError::InvalidInput(format!(
                    "invalid triaged value '{triaged}': must be 'true' or 'false'"
                )));
            }
        }
        Ok(self)
    }

    /// Returns true if any status-based filter flags are set (--ready, --blocked, --closed, --active)
    fn has_status_flags(&self) -> bool {
        self.filter_ready || self.filter_blocked || self.filter_closed || self.filter_active
    }
}

impl Default for LsOptions {
    fn default() -> Self {
        Self::new()
    }
}

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

/// List all tickets, optionally filtered by status or other criteria.
/// This is the main entry point using the LsOptions struct.
pub async fn cmd_ls_with_options(opts: LsOptions) -> Result<()> {
    // Handle --next-in-plan filter specially as it uses different logic
    if let Some(ref plan_id) = opts.next_in_plan {
        // --phase cannot be used with --next-in-plan
        if opts.phase.is_some() {
            return Err(JanusError::ConflictingFlags(
                "--phase cannot be used with --next-in-plan".to_string(),
            ));
        }
        return cmd_ls_next_in_plan(plan_id, opts.limit, opts.sort_by, opts.output_json).await;
    }

    let (tickets, _ticket_map) = get_all_tickets_with_map().await?;

    // Resolve spawned_from partial ID to full ID if provided
    let resolved_spawned_from = if let Some(ref partial_id) = opts.spawned_from {
        let ticket = Ticket::find(partial_id).await?;
        Some(ticket.id)
    } else {
        None
    };

    // Build query using TicketQueryBuilder
    let mut builder = TicketQueryBuilder::new().with_sort(opts.sort_by);

    // Add spawning filter if any spawning criteria are specified
    if resolved_spawned_from.is_some() || opts.depth.is_some() || opts.max_depth.is_some() {
        builder = builder.with_filter(Box::new(SpawningFilter::new(
            resolved_spawned_from.as_deref(),
            opts.depth,
            opts.max_depth,
        )));
    }

    // Add triaged filter if specified
    if let Some(filter_value) = opts.triaged {
        builder = builder.with_filter(Box::new(TriagedFilter::new(filter_value)));
    }

    // Add size filter if specified
    if let Some(ref sizes) = opts.size_filter {
        builder = builder.with_filter(Box::new(SizeFilter::new(sizes.clone())));
    }

    // Add status-based filters
    if let Some(status) = opts.status_filter {
        // --status flag is mutually exclusive with --ready, --blocked, --closed
        builder = builder.with_filter(Box::new(StatusFilter::new(status)));
    } else if opts.has_status_flags() {
        // Use OR-composition for status filters via the query builder
        let mut or_filters: Vec<Box<dyn crate::query::TicketFilter>> = Vec::new();

        if opts.filter_ready {
            or_filters.push(Box::new(ReadyFilter));
        }
        if opts.filter_blocked {
            or_filters.push(Box::new(BlockedFilter));
        }
        if opts.filter_closed {
            or_filters.push(Box::new(ClosedFilter));
        }
        if opts.filter_active {
            or_filters.push(Box::new(ActiveFilter));
        }

        if !or_filters.is_empty() {
            builder = builder.with_or_filters(or_filters);
        }
    } else {
        // Default: exclude closed tickets (use ActiveFilter as the base)
        builder = builder.with_filter(Box::new(ActiveFilter));
    }

    // Apply limit if specified
    if let Some(lim) = opts.limit {
        builder = builder.with_limit(lim);
    }

    // Execute the query
    let display_tickets = builder.execute(tickets).await?;
    format_ticket_list(&display_tickets, opts.output_json)
}

/// List all tickets, optionally filtered by status or other criteria.
/// This legacy function accepts individual parameters for backward compatibility.
#[deprecated(
    since = "0.1.0",
    note = "Use cmd_ls_with_options with LsOptions instead"
)]
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
    // Parse and validate status filter
    let parsed_status = match status_filter {
        Some(s) => Some(s.parse::<TicketStatus>()?),
        None => None,
    };

    // Parse and validate triaged filter
    let parsed_triaged = match triaged {
        Some(s) => match s.to_lowercase().as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => {
                return Err(JanusError::InvalidInput(format!(
                    "invalid triaged value '{s}': must be 'true' or 'false'"
                )));
            }
        },
        None => None,
    };

    let opts = LsOptions {
        filter_ready,
        filter_blocked,
        filter_closed,
        filter_active,
        status_filter: parsed_status,
        spawned_from: spawned_from.map(|s| s.to_string()),
        depth,
        max_depth,
        next_in_plan: next_in_plan.map(|s| s.to_string()),
        phase,
        triaged: parsed_triaged,
        size_filter,
        limit,
        sort_by: sort_by.parse::<SortField>()?,
        output_json,
    };
    cmd_ls_with_options(opts).await
}

/// Handle --next-in-plan filter using plan next logic
async fn cmd_ls_next_in_plan(
    plan_id: &str,
    limit: Option<usize>,
    sort_by: SortField,
    output_json: bool,
) -> Result<()> {
    use crate::query::sort_tickets_by;

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
    use crate::types::TicketId;

    fn make_ticket(id: &str, spawned_from: Option<&str>, depth: Option<u32>) -> TicketMetadata {
        TicketMetadata {
            id: Some(TicketId::new_unchecked(id)),
            spawned_from: spawned_from.map(TicketId::new_unchecked),
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

    #[test]
    fn test_with_status_str_valid() {
        let opts = LsOptions::new().with_status_str("new").unwrap();
        assert_eq!(opts.status_filter, Some(TicketStatus::New));

        let opts = LsOptions::new().with_status_str("in_progress").unwrap();
        assert_eq!(opts.status_filter, Some(TicketStatus::InProgress));

        let opts = LsOptions::new().with_status_str("complete").unwrap();
        assert_eq!(opts.status_filter, Some(TicketStatus::Complete));

        let opts = LsOptions::new().with_status_str("cancelled").unwrap();
        assert_eq!(opts.status_filter, Some(TicketStatus::Cancelled));

        let opts = LsOptions::new().with_status_str("next").unwrap();
        assert_eq!(opts.status_filter, Some(TicketStatus::Next));
    }

    #[test]
    fn test_with_status_str_case_insensitive() {
        let opts = LsOptions::new().with_status_str("NEW").unwrap();
        assert_eq!(opts.status_filter, Some(TicketStatus::New));

        let opts = LsOptions::new().with_status_str("In_Progress").unwrap();
        assert_eq!(opts.status_filter, Some(TicketStatus::InProgress));
    }

    #[test]
    fn test_with_status_str_invalid_rejects() {
        let result = LsOptions::new().with_status_str("typo");
        assert!(result.is_err());

        let result = LsOptions::new().with_status_str("open");
        assert!(result.is_err());

        let result = LsOptions::new().with_status_str("done");
        assert!(result.is_err());

        let result = LsOptions::new().with_status_str("");
        assert!(result.is_err());
    }

    #[test]
    fn test_with_triaged_str_valid() {
        let opts = LsOptions::new().with_triaged_str("true").unwrap();
        assert_eq!(opts.triaged, Some(true));

        let opts = LsOptions::new().with_triaged_str("false").unwrap();
        assert_eq!(opts.triaged, Some(false));
    }

    #[test]
    fn test_with_triaged_str_case_insensitive() {
        let opts = LsOptions::new().with_triaged_str("TRUE").unwrap();
        assert_eq!(opts.triaged, Some(true));

        let opts = LsOptions::new().with_triaged_str("False").unwrap();
        assert_eq!(opts.triaged, Some(false));
    }

    #[test]
    fn test_with_triaged_str_invalid_rejects() {
        let result = LsOptions::new().with_triaged_str("yes");
        assert!(result.is_err());

        let result = LsOptions::new().with_triaged_str("1");
        assert!(result.is_err());

        let result = LsOptions::new().with_triaged_str("no");
        assert!(result.is_err());

        let result = LsOptions::new().with_triaged_str("");
        assert!(result.is_err());

        let result = LsOptions::new().with_triaged_str("tru");
        assert!(result.is_err());
    }
}
