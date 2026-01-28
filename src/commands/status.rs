use serde_json::json;

use super::CommandOutput;
use crate::error::{JanusError, Result};
use crate::events::log_status_changed;
use crate::ticket::Ticket;
use crate::types::{TicketStatus, VALID_STATUSES};

/// Update a ticket's status
async fn update_status(id: &str, new_status: TicketStatus, output_json: bool) -> Result<()> {
    update_status_with_summary(id, new_status, None, output_json).await
}

/// Update a ticket's status with an optional completion summary
async fn update_status_with_summary(
    id: &str,
    new_status: TicketStatus,
    summary: Option<&str>,
    output_json: bool,
) -> Result<()> {
    let ticket = Ticket::find(id).await?;
    let metadata = ticket.read()?;
    let previous_status = metadata.status.unwrap_or_default();

    ticket.update_field("status", &new_status.to_string())?;

    // Write completion summary if provided
    if let Some(summary_text) = summary {
        ticket.write_completion_summary(summary_text)?;
    }

    // Get completion summary for event logging (either provided or from file)
    let summary_for_log = if let Some(s) = summary {
        Some(s.to_string())
    } else if new_status.is_terminal() {
        // Re-read to get completion summary if present
        ticket.read().ok().and_then(|m| m.completion_summary)
    } else {
        None
    };

    // Log the event
    log_status_changed(
        &ticket.id,
        &previous_status.to_string(),
        &new_status.to_string(),
        summary_for_log.as_deref(),
    );

    CommandOutput::new(json!({
        "id": ticket.id,
        "action": "status_changed",
        "previous_status": previous_status.to_string(),
        "new_status": new_status.to_string(),
    }))
    .with_text(format!("Updated {} -> {}", ticket.id, new_status))
    .print(output_json)
}

/// Set a ticket's status to "in_progress" (start working on it)
pub async fn cmd_start(id: &str, output_json: bool) -> Result<()> {
    update_status(id, TicketStatus::InProgress, output_json).await
}

/// Set a ticket's status to "complete" or "cancelled"
///
/// Requires either a summary or explicit --no-summary flag.
pub async fn cmd_close(
    id: &str,
    summary: Option<&str>,
    no_summary: bool,
    cancel: bool,
    output_json: bool,
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

    update_status_with_summary(id, new_status, summary, output_json).await
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
