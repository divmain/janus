//! Service layer for TUI business operations
//!
//! This module provides a clean separation between UI components and business logic.
//! UI components should use these services to perform operations on tickets rather
//! than directly manipulating business entities.

mod edit;
mod validator;

pub use edit::TicketEditService;
pub use validator::{TicketFormValidator, ValidationResult};

use crate::error::Result;
use crate::ticket::{Ticket, TicketBuilder};
use crate::tui::edit::extract_body_for_edit;
use crate::types::{TicketMetadata, TicketPriority, TicketStatus, TicketType};

/// Service for ticket-related business operations
///
/// This service encapsulates all ticket manipulation logic, providing a clean
/// interface for UI components to interact with tickets without containing
/// direct business logic.
pub struct TicketService;

impl TicketService {
    /// Cycle a ticket's status to the next value
    ///
    /// Status cycle: New -> Next -> InProgress -> Complete -> New
    /// Cancelled tickets reset to New
    pub async fn cycle_status(ticket_id: &str) -> Result<TicketStatus> {
        let ticket = Ticket::find(ticket_id).await?;
        let metadata = ticket.read()?;
        let current_status = metadata.status.unwrap_or_default();
        let next_status = Self::next_status(current_status);
        ticket.update_field("status", &next_status.to_string())?;
        Ok(next_status)
    }

    /// Update a ticket's status to a specific value
    pub async fn set_status(ticket_id: &str, status: TicketStatus) -> Result<()> {
        let ticket = Ticket::find(ticket_id).await?;
        ticket.update_field("status", &status.to_string())?;
        Ok(())
    }

    /// Load ticket data for editing
    ///
    /// Returns the ticket metadata and body content suitable for the edit form.
    pub async fn load_for_edit(ticket_id: &str) -> Result<(TicketMetadata, String)> {
        let ticket = Ticket::find(ticket_id).await?;
        let metadata = ticket.read()?;
        let content = ticket.read_content()?;
        let body = extract_body_for_edit(&content);
        Ok((metadata, body))
    }

    /// Update an existing ticket
    ///
    /// Updates all editable fields (title, status, type, priority, body).
    pub async fn update_ticket(
        id: &str,
        title: &str,
        status: TicketStatus,
        ticket_type: TicketType,
        priority: TicketPriority,
        body: &str,
    ) -> Result<()> {
        let ticket = Ticket::find(id).await?;

        // Update individual fields
        ticket.update_field("status", &status.to_string())?;
        ticket.update_field("type", &ticket_type.to_string())?;
        ticket.update_field("priority", &priority.to_string())?;

        // Rewrite the body section
        let content = ticket.read_content()?;
        let new_content = Self::rewrite_body(&content, title, body);
        ticket.write(&new_content)?;

        Ok(())
    }

    // =========================================================================
    // Helper methods
    // =========================================================================

    /// Get the next status in the cycle
    fn next_status(current: TicketStatus) -> TicketStatus {
        match current {
            TicketStatus::New => TicketStatus::Next,
            TicketStatus::Next => TicketStatus::InProgress,
            TicketStatus::InProgress => TicketStatus::Complete,
            TicketStatus::Complete => TicketStatus::New,
            TicketStatus::Cancelled => TicketStatus::New,
        }
    }

    /// Create a new ticket
    ///
    /// Note: This method is synchronous because TicketBuilder::build() is sync.
    /// Returns the generated ticket ID on success.
    pub fn create_ticket(
        title: &str,
        status: TicketStatus,
        ticket_type: TicketType,
        priority: TicketPriority,
        body: &str,
    ) -> Result<String> {
        let description = if body.is_empty() {
            None
        } else {
            Some(body.to_string())
        };

        let (id, _path) = TicketBuilder::new(title)
            .description(description)
            .status(status.to_string())
            .ticket_type(ticket_type.to_string())
            .priority(priority.as_num().to_string())
            .include_uuid(false)
            .run_hooks(false)
            .build()?;

        Ok(id)
    }

    /// Rewrite the body section of a ticket file while preserving frontmatter
    fn rewrite_body(content: &str, title: &str, body: &str) -> String {
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() >= 3 {
            let frontmatter = parts[1];
            let mut new_body = format!("# {}", title);
            if !body.is_empty() {
                new_body.push_str("\n\n");
                new_body.push_str(body);
            }
            format!("---{}---\n{}\n", frontmatter, new_body)
        } else {
            // Fallback: just return original content
            content.to_string()
        }
    }

    /// Mark a ticket as triaged
    pub async fn mark_triaged(ticket_id: &str, triaged: bool) -> Result<()> {
        let ticket = Ticket::find(ticket_id).await?;
        let value = if triaged { "true" } else { "false" };
        ticket.update_field("triaged", value)?;
        Ok(())
    }

    /// Add a note to a ticket
    ///
    /// Adds a timestamped note to the ticket's Notes section.
    /// Creates the Notes section if it doesn't exist.
    pub async fn add_note(ticket_id: &str, note: &str) -> Result<()> {
        use crate::utils::iso_date;
        use std::fs;

        let ticket = Ticket::find(ticket_id).await?;

        let mut content = fs::read_to_string(&ticket.file_path)?;

        // Add Notes section if it doesn't exist
        if !content.contains("## Notes") {
            content.push_str("\n## Notes");
        }

        // Add the note with timestamp
        let timestamp = iso_date();
        content.push_str(&format!("\n\n**{}**\n\n{}", timestamp, note));

        fs::write(&ticket.file_path, content)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_status_cycle() {
        assert_eq!(
            TicketService::next_status(TicketStatus::New),
            TicketStatus::Next
        );
        assert_eq!(
            TicketService::next_status(TicketStatus::Next),
            TicketStatus::InProgress
        );
        assert_eq!(
            TicketService::next_status(TicketStatus::InProgress),
            TicketStatus::Complete
        );
        assert_eq!(
            TicketService::next_status(TicketStatus::Complete),
            TicketStatus::New
        );
        assert_eq!(
            TicketService::next_status(TicketStatus::Cancelled),
            TicketStatus::New
        );
    }

    #[test]
    fn test_rewrite_body() {
        let content = r#"---
id: test-1234
status: new
---
# Old Title

Old body content.
"#;
        let result = TicketService::rewrite_body(content, "New Title", "New body content.");
        assert!(result.contains("# New Title"));
        assert!(result.contains("New body content."));
        assert!(result.contains("id: test-1234"));
        assert!(!result.contains("Old Title"));
    }

    #[test]
    fn test_rewrite_body_empty() {
        let content = r#"---
id: test-1234
status: new
---
# Old Title
"#;
        let result = TicketService::rewrite_body(content, "New Title", "");
        assert!(result.contains("# New Title"));
        assert!(!result.contains("\n\n"));
    }
}
