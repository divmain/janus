//! Query builder pattern for filtering tickets following SRP principles.
//!
//! This module provides a flexible, composable way to filter tickets using
//! the builder pattern and trait-based filters.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::error::Result;
use crate::ticket::build_ticket_map;
use crate::types::{TicketMetadata, TicketSize, TicketStatus};

/// Context passed to filters containing shared state
pub struct TicketFilterContext {
    pub ticket_map: HashMap<String, TicketMetadata>,
    pub warned_dangling: RefCell<HashSet<String>>,
}

impl TicketFilterContext {
    pub async fn new() -> Result<Self> {
        let ticket_map = build_ticket_map().await?;
        Ok(Self {
            ticket_map,
            warned_dangling: RefCell::new(HashSet::new()),
        })
    }

    /// Warn about a dangling dependency if we haven't already warned about it.
    /// Returns true if this is a new dangling dependency that was just warned about.
    pub fn warn_dangling(&self, ticket_id: &str, dep_id: &str) -> bool {
        let mut warned = self.warned_dangling.borrow_mut();
        if warned.insert(dep_id.to_string()) {
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
    target_status: String,
}

impl StatusFilter {
    pub fn new(status: &str) -> Self {
        Self {
            target_status: status.to_string(),
        }
    }
}

impl TicketFilter for StatusFilter {
    fn matches(&self, ticket: &TicketMetadata, _context: &TicketFilterContext) -> bool {
        let ticket_status = match ticket.status {
            Some(status) => status.to_string(),
            None => {
                eprintln!(
                    "Warning: ticket '{}' has missing status field, treating as 'new'",
                    ticket.id.as_deref().unwrap_or("unknown")
                );
                TicketStatus::New.to_string()
            }
        };
        ticket_status == self.target_status
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
    fn matches(&self, ticket: &TicketMetadata, _context: &TicketFilterContext) -> bool {
        // Filter by spawned_from (direct children only)
        if let Some(ref parent_id) = self.spawned_from {
            match &ticket.spawned_from {
                Some(spawned_from) if spawned_from == parent_id => {}
                _ => return false,
            }
        }

        // Filter by exact depth
        if let Some(target_depth) = self.depth {
            if ticket.compute_depth() != target_depth {
                return false;
            }
        }

        // Filter by max depth
        if let Some(max) = self.max_depth {
            if ticket.compute_depth() > max {
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

/// Filter tickets that are "ready" (New/Next status with all deps complete)
pub struct ReadyFilter;

impl TicketFilter for ReadyFilter {
    fn matches(&self, ticket: &TicketMetadata, context: &TicketFilterContext) -> bool {
        if !matches!(
            ticket.status,
            Some(TicketStatus::New) | Some(TicketStatus::Next)
        ) {
            return false;
        }

        // All deps must be complete
        let ticket_id = ticket.id.as_deref().unwrap_or("unknown");
        ticket.deps.iter().all(|dep_id| {
            context
                .ticket_map
                .get(dep_id)
                .map(|dep| dep.status == Some(TicketStatus::Complete))
                .unwrap_or_else(|| {
                    context.warn_dangling(ticket_id, dep_id);
                    false
                })
        })
    }
}

/// Filter tickets that are "blocked" (New/Next status with incomplete deps)
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

        // Check if any dep is incomplete
        let ticket_id = ticket.id.as_deref().unwrap_or("unknown");
        ticket.deps.iter().any(|dep_id| {
            context
                .ticket_map
                .get(dep_id)
                .map(|dep| dep.status != Some(TicketStatus::Complete))
                .unwrap_or_else(|| {
                    context.warn_dangling(ticket_id, dep_id);
                    true
                })
        })
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

/// Query builder for filtering and sorting tickets
pub struct TicketQueryBuilder {
    filters: Vec<Box<dyn TicketFilter>>,
    sort_by: String,
    limit: Option<usize>,
}

impl TicketQueryBuilder {
    /// Create a new query builder with default settings
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
            sort_by: "priority".to_string(),
            limit: None,
        }
    }

    /// Add a filter to the query
    pub fn with_filter(mut self, filter: Box<dyn TicketFilter>) -> Self {
        self.filters.push(filter);
        self
    }

    /// Set the sort field
    pub fn with_sort(mut self, sort_by: &str) -> Self {
        self.sort_by = sort_by.to_string();
        self
    }

    /// Set the result limit
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Execute the query against the provided tickets
    pub async fn execute(self, tickets: Vec<TicketMetadata>) -> Result<Vec<TicketMetadata>> {
        let context = TicketFilterContext::new().await?;

        // Apply all filters
        let mut filtered: Vec<TicketMetadata> = tickets
            .into_iter()
            .filter(|t| self.filters.iter().all(|f| f.matches(t, &context)))
            .collect();

        // Sort
        crate::display::sort_tickets_by(&mut filtered, &self.sort_by);

        // Apply limit
        if let Some(limit) = self.limit {
            if limit < filtered.len() {
                filtered.truncate(limit);
            }
        }

        Ok(filtered)
    }
}

impl Default for TicketQueryBuilder {
    fn default() -> Self {
        Self::new()
    }
}
