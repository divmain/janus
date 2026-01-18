use serde_json::json;

use super::print_json;
use crate::error::{JanusError, Result};
use crate::ticket::Ticket;
use crate::types::{TicketStatus, VALID_STATUSES};

/// Update a ticket's status
async fn update_status(id: &str, new_status: TicketStatus, output_json: bool) -> Result<()> {
    let ticket = Ticket::find(id).await?;
    let metadata = ticket.read()?;
    let previous_status = metadata.status.unwrap_or_default();

    ticket.update_field("status", &new_status.to_string())?;

    if output_json {
        print_json(&json!({
            "id": ticket.id,
            "action": "status_changed",
            "previous_status": previous_status.to_string(),
            "new_status": new_status.to_string(),
        }))?;
    } else {
        println!("Updated {} -> {}", ticket.id, new_status);
    }
    Ok(())
}

/// Set a ticket's status to "in_progress" (start working on it)
pub async fn cmd_start(id: &str, output_json: bool) -> Result<()> {
    update_status(id, TicketStatus::InProgress, output_json).await
}

/// Set a ticket's status to "complete"
pub async fn cmd_close(id: &str, output_json: bool) -> Result<()> {
    update_status(id, TicketStatus::Complete, output_json).await
}

/// Reopen a ticket (set status back to "new")
pub async fn cmd_reopen(id: &str, output_json: bool) -> Result<()> {
    update_status(id, TicketStatus::New, output_json).await
}

/// Set a ticket's status to an arbitrary value
pub async fn cmd_status(id: &str, status: &str, output_json: bool) -> Result<()> {
    let parsed_status: TicketStatus = status.parse().map_err(|_| {
        JanusError::InvalidStatus(format!(
            "'{}'. Must be one of: {}",
            status,
            VALID_STATUSES.join(", ")
        ))
    })?;

    update_status(id, parsed_status, output_json).await
}
