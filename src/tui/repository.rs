//! Ticket repository for loading tickets from cache or disk

use crate::cache;
use crate::ticket::get_all_tickets_from_disk;
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
    /// Load all tickets from cache or disk (async)
    pub async fn load_tickets() -> Vec<TicketMetadata> {
        if !janus_dir_exists() {
            return vec![];
        }

        if let Some(cache) = cache::get_or_init_cache().await {
            match cache.get_all_tickets().await {
                Ok(tickets) => tickets,
                Err(e) => {
                    eprintln!(
                        "Warning: failed to load from cache: {}. Using file reads.",
                        e
                    );
                    let result = get_all_tickets_from_disk();
                    if result.has_failures() {
                        for (file, error) in &result.failed {
                            eprintln!("Warning: failed to load {}: {}", file, error);
                        }
                    }
                    result.items
                }
            }
        } else {
            let result = get_all_tickets_from_disk();
            if result.has_failures() {
                for (file, error) in &result.failed {
                    eprintln!("Warning: failed to load {}: {}", file, error);
                }
            }
            result.items
        }
    }

    /// Reload all tickets from disk (async)
    pub async fn reload_tickets(&mut self) {
        self.tickets = Self::load_tickets().await;
    }
}
