//! Shared state types and ticket loading helpers for TUI views

use std::path::Path;

use crate::cache;
use crate::ticket::get_all_tickets_from_disk;
use crate::types::{TICKETS_DIR, TicketMetadata, TicketStatus};

/// Active pane in the issue browser
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pane {
    #[default]
    Search,
    List,
    Detail,
}

/// Result of initializing the TUI state
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
    Path::new(TICKETS_DIR).is_dir()
}

/// Shared TUI state for loading and managing tickets
#[derive(Debug, Clone, Default)]
pub struct TuiState {
    /// All loaded tickets
    pub all_tickets: Vec<TicketMetadata>,
    /// Whether initialization failed due to missing directory
    pub init_error: Option<String>,
}

impl TuiState {
    /// Create a new TUI state with all tickets loaded
    pub async fn new() -> Self {
        if !janus_dir_exists() {
            return Self {
                all_tickets: vec![],
                init_error: Some("No .janus directory found".to_string()),
            };
        }

        let all_tickets = if let Some(cache) = cache::get_or_init_cache().await {
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
        };

        Self {
            all_tickets,
            init_error: None,
        }
    }

    /// Create a new TUI state with initialization result tracking
    pub async fn init() -> (Self, InitResult) {
        if !janus_dir_exists() {
            return (
                Self {
                    all_tickets: vec![],
                    init_error: Some("No .janus directory found".to_string()),
                },
                InitResult::NoJanusDir,
            );
        }

        let all_tickets = if let Some(cache) = cache::get_or_init_cache().await {
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
        };

        let result = if all_tickets.is_empty() {
            InitResult::EmptyDir
        } else {
            InitResult::Ok
        };

        (
            Self {
                all_tickets,
                init_error: None,
            },
            result,
        )
    }

    /// Reload all tickets from disk
    pub async fn reload(&mut self) {
        // Sync cache (incremental update happens automatically in get_or_init_cache)
        if let Some(cache) = cache::get_or_init_cache().await {
            match cache.get_all_tickets().await {
                Ok(tickets) => {
                    self.all_tickets = tickets;
                    return;
                }
                Err(e) => {
                    eprintln!("Warning: cache reload failed: {}. Using file reads.", e);
                }
            }
        }

        // FALLBACK: Original implementation
        self.all_tickets = get_all_tickets_from_disk();
    }

    /// Get tickets filtered by status
    pub fn tickets_by_status(&self, status: TicketStatus) -> Vec<&TicketMetadata> {
        self.all_tickets
            .iter()
            .filter(|t| t.status == Some(status))
            .collect()
    }

    /// Get the total count of tickets
    pub fn ticket_count(&self) -> usize {
        self.all_tickets.len()
    }

    /// Get counts for each status (for kanban board column headers)
    pub fn status_counts(&self) -> StatusCounts {
        let mut counts = StatusCounts::default();
        for ticket in &self.all_tickets {
            match ticket.status {
                Some(TicketStatus::New) => counts.new += 1,
                Some(TicketStatus::Next) => counts.next += 1,
                Some(TicketStatus::InProgress) => counts.in_progress += 1,
                Some(TicketStatus::Complete) => counts.complete += 1,
                Some(TicketStatus::Cancelled) => counts.cancelled += 1,
                None => counts.new += 1, // Default to new
            }
        }
        counts
    }

    /// Sort tickets by priority (ascending), then by ID
    pub fn sort_by_priority(&mut self) {
        self.all_tickets.sort_by(|a, b| {
            let pa = a.priority_num();
            let pb = b.priority_num();
            if pa != pb {
                pa.cmp(&pb)
            } else {
                a.id.cmp(&b.id)
            }
        });
    }
}

/// Counts of tickets by status
#[derive(Debug, Clone, Copy, Default)]
pub struct StatusCounts {
    pub new: usize,
    pub next: usize,
    pub in_progress: usize,
    pub complete: usize,
    pub cancelled: usize,
}

impl StatusCounts {
    /// Get count for a specific status
    pub fn for_status(&self, status: TicketStatus) -> usize {
        match status {
            TicketStatus::New => self.new,
            TicketStatus::Next => self.next,
            TicketStatus::InProgress => self.in_progress,
            TicketStatus::Complete => self.complete,
            TicketStatus::Cancelled => self.cancelled,
        }
    }

    /// Get total count of all tickets
    pub fn total(&self) -> usize {
        self.new + self.next + self.in_progress + self.complete + self.cancelled
    }
}

/// Get the git user name for default assignee
pub fn get_git_user_name() -> Option<String> {
    std::process::Command::new("git")
        .args(["config", "user.name"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            } else {
                None
            }
        })
}

impl TuiState {
    /// Create a new TUI state with all tickets loaded (sync wrapper for backward compatibility)
    pub fn new_sync() -> Self {
        tokio::runtime::Handle::try_current()
            .ok()
            .map(|h| h.block_on(Self::new()))
            .unwrap_or_else(|| {
                // No tokio runtime available, fall back to sync implementation
                Self {
                    all_tickets: if janus_dir_exists() {
                        get_all_tickets_from_disk()
                    } else {
                        vec![]
                    },
                    init_error: None,
                }
            })
    }

    /// Create a new TUI state with initialization result tracking (sync wrapper)
    pub fn init_sync() -> (Self, InitResult) {
        tokio::runtime::Handle::try_current()
            .ok()
            .map(|h| h.block_on(Self::init()))
            .unwrap_or_else(|| {
                // No tokio runtime available, fall back to sync implementation
                if !janus_dir_exists() {
                    return (
                        Self {
                            all_tickets: vec![],
                            init_error: Some("No .janus directory found".to_string()),
                        },
                        InitResult::NoJanusDir,
                    );
                }

                let tickets = get_all_tickets_from_disk();
                let result = if tickets.is_empty() {
                    InitResult::EmptyDir
                } else {
                    InitResult::Ok
                };

                (
                    Self {
                        all_tickets: tickets,
                        init_error: None,
                    },
                    result,
                )
            })
    }

    /// Reload all tickets from disk (sync wrapper)
    pub fn reload_sync(&mut self) {
        if let Ok(h) = tokio::runtime::Handle::try_current() {
            h.block_on(self.reload());
        }
    }
}
