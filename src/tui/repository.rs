//! Ticket repository for loading tickets from the in-memory store

use crate::store::get_or_init_store;
use crate::types::{TicketMetadata, janus_root};

/// Result of initializing the ticket repository
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InitResult {
    /// Successfully loaded tickets
    #[default]
    Ok,
    /// The .janus directory doesn't exist
    NoJanusDir,
    /// The .janus directory exists but is empty
    EmptyDir,
}

/// Check if the Janus directory exists
pub fn janus_dir_exists() -> bool {
    janus_root().is_dir()
}

/// Repository for loading and managing ticket data
#[derive(Debug, Clone, Default)]
pub struct TicketRepository {
    /// All loaded tickets
    pub tickets: Vec<TicketMetadata>,
    /// Whether initialization failed due to missing directory
    pub init_error: Option<String>,
}

impl TicketRepository {
    /// Load all tickets from the in-memory store (async)
    pub async fn load_tickets() -> Vec<TicketMetadata> {
        if !janus_dir_exists() {
            return vec![];
        }

        match get_or_init_store().await {
            Ok(store) => store.get_all_tickets(),
            Err(_) => vec![],
        }
    }

    /// Re-read a specific ticket from disk and upsert it into the store.
    ///
    /// This should be called after a TUI mutation writes changes to disk,
    /// so the in-memory store is immediately consistent before `load_tickets()`
    /// is called. The filesystem watcher provides eventual consistency for
    /// external changes, but direct mutations need immediate store updates.
    pub async fn refresh_ticket_in_store(ticket_id: &str) {
        let ticket = match crate::ticket::Ticket::find(ticket_id).await {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Warning: failed to find ticket '{ticket_id}' for store refresh: {e}");
                return;
            }
        };
        let metadata = match ticket.read() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Warning: failed to read ticket '{ticket_id}' for store refresh: {e}");
                return;
            }
        };
        match get_or_init_store().await {
            Ok(store) => store.upsert_ticket(metadata),
            Err(e) => {
                eprintln!("Warning: failed to init store for ticket refresh: {e}");
            }
        }
    }

    /// Re-read a specific plan from disk and upsert it into the store.
    ///
    /// This is the plan equivalent of `refresh_ticket_in_store`. It should be
    /// called after a mutation writes plan changes to disk, so the in-memory
    /// store is immediately consistent.
    pub async fn refresh_plan_in_store(plan_id: &str) {
        let plan = match crate::plan::Plan::find(plan_id).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Warning: failed to find plan '{plan_id}' for store refresh: {e}");
                return;
            }
        };
        let metadata = match plan.read() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Warning: failed to read plan '{plan_id}' for store refresh: {e}");
                return;
            }
        };
        match get_or_init_store().await {
            Ok(store) => store.upsert_plan(metadata),
            Err(e) => {
                eprintln!("Warning: failed to init store for plan refresh: {e}");
            }
        }
    }

    /// Refresh a single ticket in a local ticket list after a mutation.
    ///
    /// Re-reads the ticket from the store and replaces it in-place in the
    /// provided vec, avoiding the O(n log n) cost of reloading all tickets.
    /// Returns the updated vec.
    pub async fn refresh_single_ticket(
        mut tickets: Vec<TicketMetadata>,
        ticket_id: &str,
    ) -> Vec<TicketMetadata> {
        if let Ok(store) = get_or_init_store().await {
            if let Some(updated) = store.get_ticket(ticket_id) {
                // Find and replace the ticket in the vec
                if let Some(pos) = tickets
                    .iter()
                    .position(|t| t.id.as_deref() == Some(ticket_id))
                {
                    tickets[pos] = updated;
                } else {
                    // Ticket was newly created — append and re-sort
                    tickets.push(updated);
                    tickets.sort_by(|a, b| {
                        a.id.as_deref()
                            .unwrap_or("")
                            .cmp(b.id.as_deref().unwrap_or(""))
                    });
                }
            }
        }
        tickets
    }

    /// Reload all tickets from disk (async)
    pub async fn reload_tickets(&mut self) {
        self.tickets = Self::load_tickets().await;
    }
}
