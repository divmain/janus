//! Query builder pattern for filtering tickets following SRP principles.
//!
//! This module provides a flexible, composable way to filter tickets using
//! the builder pattern and trait-based filters.

use std::collections::HashMap;

use dashmap::DashSet;

use crate::error::Result;
use crate::status::{all_deps_satisfied, has_unsatisfied_dep};
use crate::ticket::build_ticket_map;
use crate::types::{TicketData, TicketMetadata, TicketSize, TicketStatus, TicketType};

pub mod sort;

pub use sort::{SortField, sort_by_created, sort_by_id, sort_by_priority, sort_tickets_by};

/// Context passed to filters containing shared state
pub struct TicketFilterContext {
    pub ticket_map: HashMap<String, TicketMetadata>,
    pub warned_dangling: DashSet<String>,
}

impl TicketFilterContext {
    /// Create a new context with the provided ticket map.
    /// The ticket map should be built once by the caller and passed in
    /// to avoid rebuilding it for each query execution.
    pub fn new(ticket_map: HashMap<String, TicketMetadata>) -> Self {
        Self {
            ticket_map,
            warned_dangling: DashSet::new(),
        }
    }

    /// Create a new context by building the ticket map from disk.
    /// This is a convenience method for when the caller doesn't have
    /// the ticket map readily available.
    pub async fn new_from_disk() -> Result<Self> {
        let ticket_map = build_ticket_map().await?;
        Ok(Self::new(ticket_map))
    }

    /// Warn about a dangling dependency if we haven't already warned about it.
    /// Returns true if this is a new dangling dependency that was just warned about.
    pub fn warn_dangling(&self, ticket_id: &str, dep_id: &str) -> bool {
        if self.warned_dangling.insert(dep_id.to_string()) {
            eprintln!("Warning: Ticket {ticket_id} references dangling dependency {dep_id}");
            true
        } else {
            false
        }
    }
}

/// Trait for ticket filters
pub trait TicketFilter: Send + Sync {
    fn matches(&self, ticket: &TicketMetadata, context: &TicketFilterContext) -> bool;
}

/// Filter tickets by status
pub struct StatusFilter {
    target_status: TicketStatus,
}

impl StatusFilter {
    pub fn new(status: TicketStatus) -> Self {
        Self {
            target_status: status,
        }
    }
}

impl TicketFilter for StatusFilter {
    fn matches(&self, ticket: &TicketMetadata, _context: &TicketFilterContext) -> bool {
        let ticket_status = match ticket.status {
            Some(status) => status,
            None => {
                eprintln!(
                    "Warning: ticket '{}' has missing status field, treating as 'new'",
                    ticket.id.as_deref().unwrap_or("unknown")
                );
                TicketStatus::New
            }
        };
        ticket_status == self.target_status
    }
}

/// Filter tickets by type
pub struct TypeFilter {
    target_type: TicketType,
}

impl TypeFilter {
    pub fn new(ticket_type: TicketType) -> Self {
        Self {
            target_type: ticket_type,
        }
    }
}

impl TicketFilter for TypeFilter {
    fn matches(&self, ticket: &TicketMetadata, _context: &TicketFilterContext) -> bool {
        let ticket_type = ticket.ticket_type.unwrap_or_default();
        ticket_type == self.target_type
    }
}

/// Filter tickets by spawned_from relationship
pub struct SpawningFilter {
    spawned_from: Option<String>,
    depth: Option<u32>,
    max_depth: Option<u32>,
}

impl SpawningFilter {
    pub fn new(spawned_from: Option<&str>, depth: Option<u32>, max_depth: Option<u32>) -> Self {
        Self {
            spawned_from: spawned_from.map(|s| s.to_string()),
            depth,
            max_depth,
        }
    }
}

impl TicketFilter for SpawningFilter {
    fn matches(&self, ticket: &TicketMetadata, context: &TicketFilterContext) -> bool {
        // Filter by spawned_from (direct children only)
        if let Some(ref parent_id) = self.spawned_from {
            match ticket.spawned_from.as_deref() {
                Some(spawned_from) if spawned_from == parent_id => {}
                _ => return false,
            }
        }

        // Compute depth once if needed for either filter
        let ticket_depth = if self.depth.is_some() || self.max_depth.is_some() {
            ticket.compute_depth(&context.ticket_map)
        } else {
            0 // value won't be used
        };

        // Filter by exact depth
        if let Some(target_depth) = self.depth {
            if ticket_depth != target_depth {
                return false;
            }
        }

        // Filter by max depth
        if let Some(max) = self.max_depth {
            if ticket_depth > max {
                return false;
            }
        }

        true
    }
}

/// Filter tickets by size
pub struct SizeFilter {
    sizes: Vec<TicketSize>,
}

impl SizeFilter {
    pub fn new(sizes: Vec<TicketSize>) -> Self {
        Self { sizes }
    }
}

impl TicketFilter for SizeFilter {
    fn matches(&self, ticket: &TicketMetadata, _context: &TicketFilterContext) -> bool {
        let ticket_size = ticket.size;
        self.sizes
            .iter()
            .any(|filter_size| ticket_size == Some(*filter_size))
    }
}

/// Filter tickets by triaged status
pub struct TriagedFilter {
    triaged_value: bool,
}

impl TriagedFilter {
    pub fn new(triaged: bool) -> Self {
        Self {
            triaged_value: triaged,
        }
    }
}

impl TicketFilter for TriagedFilter {
    fn matches(&self, ticket: &TicketMetadata, _context: &TicketFilterContext) -> bool {
        // Treat triaged: None as false for backward compatibility
        let ticket_triaged = ticket.triaged.unwrap_or(false);
        ticket_triaged == self.triaged_value
    }
}

/// Filter tickets that are "ready" (New/Next status with all deps satisfied)
pub struct ReadyFilter;

impl TicketFilter for ReadyFilter {
    fn matches(&self, ticket: &TicketMetadata, context: &TicketFilterContext) -> bool {
        if !matches!(
            ticket.status,
            Some(TicketStatus::New) | Some(TicketStatus::Next)
        ) {
            return false;
        }

        // Warn about dangling deps before the shared check
        let ticket_id = ticket.id.as_deref().unwrap_or("unknown");
        for dep_id in &ticket.deps {
            if !context.ticket_map.contains_key(dep_id.as_ref()) {
                context.warn_dangling(ticket_id, dep_id.as_ref());
            }
        }

        // All deps must be satisfied (terminal status; orphans block)
        all_deps_satisfied(ticket, &context.ticket_map)
    }
}

/// Filter tickets that are "blocked" (New/Next status with unsatisfied deps)
pub struct BlockedFilter;

impl TicketFilter for BlockedFilter {
    fn matches(&self, ticket: &TicketMetadata, context: &TicketFilterContext) -> bool {
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

        // Warn about dangling deps before the shared check
        let ticket_id = ticket.id.as_deref().unwrap_or("unknown");
        for dep_id in &ticket.deps {
            if !context.ticket_map.contains_key(dep_id.as_ref()) {
                context.warn_dangling(ticket_id, dep_id.as_ref());
            }
        }

        // Check if any dep is unsatisfied (not terminal, or orphan)
        has_unsatisfied_dep(ticket, &context.ticket_map)
    }
}

/// Filter tickets that are closed (Complete or Cancelled)
pub struct ClosedFilter;

impl TicketFilter for ClosedFilter {
    fn matches(&self, ticket: &TicketMetadata, _context: &TicketFilterContext) -> bool {
        matches!(
            ticket.status,
            Some(TicketStatus::Complete) | Some(TicketStatus::Cancelled)
        )
    }
}

/// Filter tickets that are active (not closed)
pub struct ActiveFilter;

impl TicketFilter for ActiveFilter {
    fn matches(&self, ticket: &TicketMetadata, _context: &TicketFilterContext) -> bool {
        !matches!(
            ticket.status,
            Some(TicketStatus::Complete) | Some(TicketStatus::Cancelled)
        )
    }
}

/// An executed query configuration that can be applied to ticket data.
/// This separates query configuration from execution, following SRP.
pub struct TicketQuery {
    filters: Vec<Box<dyn TicketFilter>>,
    or_filter_groups: Vec<Vec<Box<dyn TicketFilter>>>,
    sort_by: SortField,
    limit: Option<usize>,
}

impl TicketQuery {
    /// Apply this query to the provided tickets and ticket map.
    /// The caller is responsible for providing the ticket_map, which allows
    /// for better control over data sources and avoids tight coupling.
    pub fn apply(
        &self,
        tickets: Vec<TicketMetadata>,
        ticket_map: &HashMap<String, TicketMetadata>,
    ) -> Vec<TicketMetadata> {
        let context = TicketFilterContext::new(ticket_map.clone());

        // Apply AND filters first
        let mut filtered: Vec<TicketMetadata> = tickets
            .into_iter()
            .filter(|t| self.filters.iter().all(|f| f.matches(t, &context)))
            .collect();

        // Apply OR filter groups as a union across all groups
        if !self.or_filter_groups.is_empty() {
            use std::collections::HashSet;

            use crate::types::TicketId;

            // Collect all tickets that match ANY filter in ANY group
            // We iterate over filtered (the AND results), not the original tickets
            let mut matched_ids: HashSet<TicketId> = HashSet::new();

            for or_group in &self.or_filter_groups {
                // Check which filtered tickets match ANY filter in this group
                for ticket in &filtered {
                    if or_group.iter().any(|f| f.matches(ticket, &context)) {
                        if let Some(id) = &ticket.id {
                            matched_ids.insert(id.clone());
                        }
                    }
                }
            }

            // Keep only filtered tickets that matched any OR group
            filtered.retain(|t| t.id.as_ref().is_some_and(|id| matched_ids.contains(id)));
        }

        // Sort using local sort function
        sort::sort_tickets_by(&mut filtered, self.sort_by);

        // Apply limit
        if let Some(limit) = self.limit {
            if limit < filtered.len() {
                filtered.truncate(limit);
            }
        }

        filtered
    }
}

/// Query builder for filtering and sorting tickets
pub struct TicketQueryBuilder {
    filters: Vec<Box<dyn TicketFilter>>,
    or_filter_groups: Vec<Vec<Box<dyn TicketFilter>>>,
    sort_by: SortField,
    limit: Option<usize>,
}

impl TicketQueryBuilder {
    /// Create a new query builder with default settings
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
            or_filter_groups: Vec::new(),
            sort_by: SortField::default(),
            limit: None,
        }
    }

    /// Add a filter to the query (AND composition)
    pub fn with_filter(mut self, filter: Box<dyn TicketFilter>) -> Self {
        self.filters.push(filter);
        self
    }

    /// Add a group of filters that will be OR-composed together.
    /// Tickets matching ANY filter in the group will be included.
    pub fn with_or_filters(mut self, filters: Vec<Box<dyn TicketFilter>>) -> Self {
        if !filters.is_empty() {
            self.or_filter_groups.push(filters);
        }
        self
    }

    /// Set the sort field
    pub fn with_sort(mut self, sort_by: SortField) -> Self {
        self.sort_by = sort_by;
        self
    }

    /// Set the result limit
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Build the query configuration from this builder.
    /// Returns a TicketQuery that can be applied to ticket data.
    pub fn build(self) -> TicketQuery {
        TicketQuery {
            filters: self.filters,
            or_filter_groups: self.or_filter_groups,
            sort_by: self.sort_by,
            limit: self.limit,
        }
    }

    /// Execute the query against the provided tickets.
    /// This is a convenience method that builds the query and applies it.
    /// For more control, use `build()` followed by `TicketQuery::apply()`.
    pub async fn execute(self, tickets: Vec<TicketMetadata>) -> Result<Vec<TicketMetadata>> {
        // Build ticket map once from the provided tickets
        let ticket_map: HashMap<String, TicketMetadata> = tickets
            .iter()
            .filter_map(|t| t.id.as_ref().map(|id| (id.to_string(), t.clone())))
            .collect();

        let query = self.build();
        Ok(query.apply(tickets, &ticket_map))
    }
}

impl Default for TicketQueryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use dashmap::DashSet;

    use super::*;
    use crate::types::TicketId;

    fn make_ticket_with_status(id: &str, status: TicketStatus) -> TicketMetadata {
        TicketMetadata {
            id: Some(TicketId::new_unchecked(id)),
            status: Some(status),
            ..Default::default()
        }
    }

    fn empty_context() -> TicketFilterContext {
        TicketFilterContext {
            ticket_map: HashMap::new(),
            warned_dangling: DashSet::new(),
        }
    }

    #[test]
    fn test_status_filter_matches_correct_status() {
        let context = empty_context();
        let ticket = make_ticket_with_status("t-1", TicketStatus::New);
        let filter = StatusFilter::new(TicketStatus::New);
        assert!(filter.matches(&ticket, &context));
    }

    #[test]
    fn test_status_filter_rejects_wrong_status() {
        let context = empty_context();
        let ticket = make_ticket_with_status("t-1", TicketStatus::New);
        let filter = StatusFilter::new(TicketStatus::Complete);
        assert!(!filter.matches(&ticket, &context));
    }

    #[test]
    fn test_status_filter_all_variants() {
        let context = empty_context();
        let statuses = [
            TicketStatus::New,
            TicketStatus::Next,
            TicketStatus::InProgress,
            TicketStatus::Complete,
            TicketStatus::Cancelled,
        ];

        for status in statuses {
            let ticket = make_ticket_with_status("t-1", status);
            let filter = StatusFilter::new(status);
            assert!(
                filter.matches(&ticket, &context),
                "StatusFilter({status}) should match ticket with status {status}"
            );

            // Should not match other statuses
            for other in statuses {
                if other != status {
                    let other_filter = StatusFilter::new(other);
                    assert!(
                        !other_filter.matches(&ticket, &context),
                        "StatusFilter({other}) should NOT match ticket with status {status}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_invalid_status_string_parse_fails() {
        // These should all fail to parse - verifying that invalid strings
        // cannot silently become valid statuses
        assert!("typo".parse::<TicketStatus>().is_err());
        assert!("open".parse::<TicketStatus>().is_err());
        assert!("done".parse::<TicketStatus>().is_err());
        assert!("closed".parse::<TicketStatus>().is_err());
        assert!("".parse::<TicketStatus>().is_err());
        assert!("in-progress".parse::<TicketStatus>().is_err()); // hyphen instead of underscore
    }
}
