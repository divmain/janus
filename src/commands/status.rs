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
        write_completion_summary(&ticket, summary_text)?;
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

/// Write a completion summary section to a ticket file
fn write_completion_summary(ticket: &Ticket, summary: &str) -> Result<()> {
    let content = ticket.read_content()?;

    let section_start = find_completion_summary_section(&content);

    let new_content = if let Some(start_idx) = section_start {
        let after_header = &content[start_idx..];
        let header_end = after_header
            .find('\n')
            .ok_or_else(|| {
                JanusError::Other(
                    "Invalid ticket file structure: '## Completion Summary' header found but missing newline"
                        .to_string(),
                )
            })?;
        let section_content_start = start_idx + header_end;

        let section_content = &content[section_content_start..];
        let next_h2_re = regex::Regex::new(r"(?m)^## ").expect("regex should compile");
        let section_end = next_h2_re
            .find(section_content)
            .map(|m| section_content_start + m.start())
            .unwrap_or(content.len());

        let before = &content[..start_idx];
        let after = &content[section_end..];

        format!(
            "{}## Completion Summary\n\n{}\n{}",
            before,
            summary,
            if after.is_empty() { "" } else { "\n" }.to_owned() + after.trim_start_matches('\n')
        )
    } else {
        let trimmed = content.trim_end();
        format!("{}\n\n## Completion Summary\n\n{}\n", trimmed, summary)
    };

    ticket.write(&new_content)
}

/// Find the start position of the Completion Summary section (case-insensitive)
fn find_completion_summary_section(content: &str) -> Option<usize> {
    let section_pattern =
        regex::Regex::new(r"(?mi)^## completion summary\s*$").expect("regex should compile");
    section_pattern.find(content).map(|m| m.start())
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
