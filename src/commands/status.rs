use crate::error::{JanusError, Result};
use crate::ticket::Ticket;
use crate::types::{TicketStatus, VALID_STATUSES};

/// Update a ticket's status
fn update_status(id: &str, status: TicketStatus) -> Result<()> {
    let ticket = Ticket::find(id)?;
    ticket.update_field("status", &status.to_string())?;
    println!("Updated {} -> {}", ticket.id, status);
    Ok(())
}

/// Set a ticket's status to "in_progress" (start working on it)
pub fn cmd_start(id: &str) -> Result<()> {
    update_status(id, TicketStatus::InProgress)
}

/// Set a ticket's status to "complete"
pub fn cmd_close(id: &str) -> Result<()> {
    update_status(id, TicketStatus::Complete)
}

/// Reopen a ticket (set status back to "new")
pub fn cmd_reopen(id: &str) -> Result<()> {
    update_status(id, TicketStatus::New)
}

/// Set a ticket's status to an arbitrary value
pub fn cmd_status(id: &str, status: &str) -> Result<()> {
    let parsed_status: TicketStatus = status.parse().map_err(|_| {
        JanusError::InvalidStatus(format!(
            "'{}'. Must be one of: {}",
            status,
            VALID_STATUSES.join(", ")
        ))
    })?;

    update_status(id, parsed_status)
}
