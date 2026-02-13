//! Shared state types and helpers for TUI views

use crate::tui::analytics::{StatusCounts, TicketAnalytics};
use crate::tui::repository::TicketRepository;
use crate::types::{TicketMetadata, TicketStatus};

pub use crate::tui::repository::InitResult;

/// Active pane in the issue browser
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pane {
    #[default]
    Search,
    List,
    Detail,
}

/// Shared TUI state for managing tickets in views
///
/// This is a thin UI-specific wrapper that composes TicketRepository (data fetching)
/// and TicketAnalytics (filtering, counting, analysis).
#[derive(Debug, Clone, Default)]
pub struct TuiState {
    /// Repository for loading and managing ticket data
    pub repository: TicketRepository,
}

impl TuiState {
    /// Get all tickets
    pub fn all_tickets(&self) -> &[TicketMetadata] {
        &self.repository.tickets
    }

    /// Get tickets filtered by status
    pub fn tickets_by_status(&self, status: TicketStatus) -> Vec<&TicketMetadata> {
        TicketAnalytics::tickets_by_status(&self.repository.tickets, status)
    }

    /// Get the total count of tickets
    pub fn ticket_count(&self) -> usize {
        TicketAnalytics::ticket_count(&self.repository.tickets)
    }

    /// Get counts for each status (for kanban board column headers)
    pub fn status_counts(&self) -> StatusCounts {
        TicketAnalytics::status_counts(&self.repository.tickets)
    }

    /// Sort tickets by priority (ascending), then by ID
    pub fn sort_by_priority(&mut self) {
        TicketAnalytics::sort_by_priority(&mut self.repository.tickets);
    }
}

/// Get the git user name (utility function)
///
/// SECURITY: This function executes `git config user.name` as a shell command.
/// Arguments are hardcoded using the safe array form `.args(["config", "user.name"])`
/// to prevent shell injection vulnerabilities. DO NOT modify this function to accept
/// user input in the arguments - any change introducing user input would create a
/// shell injection vulnerability. If you need dynamic arguments, use proper
/// sanitization or the safe array form exclusively.
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
