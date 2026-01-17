//! Ticket repository for loading tickets from cache or disk

use crate::cache;
use crate::ticket::get_all_tickets_from_disk;
use crate::types::{TICKETS_DIR, TicketMetadata};

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
    std::path::Path::new(TICKETS_DIR).is_dir()
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
                    get_all_tickets_from_disk()
                }
            }
        } else {
            get_all_tickets_from_disk()
        }
    }

    /// Reload all tickets from disk (async)
    pub async fn reload_tickets(&mut self) {
        self.tickets = Self::load_tickets().await;
    }
}

/// Synchronous wrapper for TicketRepository operations
impl TicketRepository {
    /// Create a new repository with all tickets loaded (sync wrapper for backward compatibility)
    pub fn new_sync() -> Self {
        let tickets = tokio::runtime::Handle::try_current()
            .ok()
            .map(|h| h.block_on(Self::load_tickets()))
            .unwrap_or_else(|| {
                if janus_dir_exists() {
                    get_all_tickets_from_disk()
                } else {
                    vec![]
                }
            });

        Self {
            tickets,
            init_error: None,
        }
    }

    /// Create a new repository with initialization result tracking (sync wrapper)
    pub fn init_sync() -> (Self, InitResult) {
        if !janus_dir_exists() {
            return (
                Self {
                    tickets: vec![],
                    init_error: Some("No .janus directory found".to_string()),
                },
                InitResult::NoJanusDir,
            );
        }

        let tickets = tokio::runtime::Handle::try_current()
            .ok()
            .map(|h| h.block_on(Self::load_tickets()))
            .unwrap_or_else(get_all_tickets_from_disk);

        let result = if tickets.is_empty() {
            InitResult::EmptyDir
        } else {
            InitResult::Ok
        };

        (
            Self {
                tickets,
                init_error: None,
            },
            result,
        )
    }

    /// Reload all tickets from disk (sync wrapper)
    pub fn reload_sync(&mut self) {
        if let Ok(h) = tokio::runtime::Handle::try_current() {
            h.block_on(self.reload_tickets());
        }
    }
}
