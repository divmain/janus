use serde_json::json;

use super::CommandOutput;
use crate::cli::OutputOptions;
use crate::error::{JanusError, Result};
use crate::ticket::Ticket;
use crate::types::TicketStatus;

/// Update a ticket's status
async fn update_status(id: &str, new_status: TicketStatus, output: OutputOptions) -> Result<()> {
    update_status_with_summary(id, new_status, None, output).await
}

/// Update a ticket's status with an optional completion summary
async fn update_status_with_summary(
    id: &str,
    new_status: TicketStatus,
    summary: Option<&str>,
    output: OutputOptions,
) -> Result<()> {
    let ticket = Ticket::find(id).await?;

    // Use the domain method that handles status updates and event logging
    ticket.update_status(new_status, summary)?;

    CommandOutput::new(json!({
        "id": ticket.id,
        "action": "status_changed",
        "new_status": new_status.to_string(),
    }))
    .with_text(format!("Updated {} -> {}", ticket.id, new_status))
    .print(output)
}

/// Set a ticket's status to "in_progress" (start working on it)
pub async fn cmd_start(id: &str, output: OutputOptions) -> Result<()> {
    update_status(id, TicketStatus::InProgress, output).await
}

/// Set a ticket's status to "complete" or "cancelled"
///
/// Requires either a summary or explicit --no-summary flag.
pub async fn cmd_close(
    id: &str,
    summary: Option<&str>,
    no_summary: bool,
    cancel: bool,
    output: OutputOptions,
) -> Result<()> {
    // Require either --summary or --no-summary
    if summary.is_none() && !no_summary {
        return Err(JanusError::SummaryRequired);
    }

    let new_status = if cancel {
        TicketStatus::Cancelled
    } else {
        TicketStatus::Complete
    };

    update_status_with_summary(id, new_status, summary, output).await
}

/// Reopen a ticket (set status back to "new")
pub async fn cmd_reopen(id: &str, output: OutputOptions) -> Result<()> {
    update_status(id, TicketStatus::New, output).await
}

/// Set a ticket's status to an arbitrary value
pub async fn cmd_status(id: &str, status: TicketStatus, output: OutputOptions) -> Result<()> {
    update_status(id, status, output).await
}
