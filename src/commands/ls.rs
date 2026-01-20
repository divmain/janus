use super::{
    CommandOutput, FormatOptions, format_deps, format_ticket_line, sort_by_priority, ticket_to_json,
};
use crate::error::Result;
use crate::ticket::get_all_tickets_with_map;
use crate::types::{TicketMetadata, TicketStatus};

/// List all tickets, optionally filtered by status or other criteria
pub async fn cmd_ls(
    filter_ready: bool,
    filter_blocked: bool,
    filter_closed: bool,
    include_all: bool,
    status_filter: Option<&str>,
    limit: Option<usize>,
    output_json: bool,
) -> Result<()> {
    let (tickets, ticket_map) = get_all_tickets_with_map().await;

    let filtered: Vec<TicketMetadata> = tickets
        .iter()
        .filter(|t| {
            // Check if we should include closed/cancelled tickets
            let is_closed = matches!(
                t.status,
                Some(TicketStatus::Complete) | Some(TicketStatus::Cancelled)
            );

            // --status flag is mutually exclusive with --ready, --blocked, --closed
            // (enforced by clap's conflicts_with_all in main.rs)
            if let Some(filter) = status_filter {
                let ticket_status = match t.status {
                    Some(status) => status.to_string(),
                    None => {
                        eprintln!(
                            "Warning: ticket '{}' has missing status field, treating as 'new'",
                            t.id.as_deref().unwrap_or("unknown")
                        );
                        TicketStatus::New.to_string()
                    }
                };
                return ticket_status == filter;
            }

            // Calculate individual filter results
            let is_ready = if filter_ready {
                if matches!(t.status, Some(TicketStatus::New) | Some(TicketStatus::Next)) {
                    // All deps must be complete
                    t.deps.iter().all(|dep_id| {
                        ticket_map
                            .get(dep_id)
                            .map(|dep| dep.status == Some(TicketStatus::Complete))
                            .unwrap_or(false)
                    })
                } else {
                    false
                }
            } else {
                false
            };

            let is_blocked = if filter_blocked {
                if matches!(t.status, Some(TicketStatus::New) | Some(TicketStatus::Next)) {
                    // Must have deps
                    if !t.deps.is_empty() {
                        // Check if any dep is incomplete
                        t.deps.iter().any(|dep_id| {
                            ticket_map
                                .get(dep_id)
                                .map(|dep| dep.status != Some(TicketStatus::Complete))
                                .unwrap_or(true)
                        })
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };

            // Calculate final result based on filter combination
            if filter_ready || filter_blocked || filter_closed {
                // At least one special filter is active - use union behavior
                is_ready || is_blocked || is_closed
            } else {
                // No special filters - apply default behavior
                // Exclude closed tickets unless --all is set
                !is_closed || include_all
            }
        })
        .cloned()
        .collect();

    // Sort by priority then apply limit if specified
    let mut display_tickets = filtered;
    sort_by_priority(&mut display_tickets);

    // Apply limit: if --closed with no explicit --limit, default to 20
    let limit = limit.unwrap_or(if filter_closed { 20 } else { usize::MAX });
    if limit < display_tickets.len() {
        display_tickets.truncate(limit);
    }

    let json_tickets: Vec<_> = display_tickets.iter().map(ticket_to_json).collect();

    // Build text output eagerly
    let text_output = display_tickets
        .iter()
        .map(|t| {
            let opts = FormatOptions {
                suffix: Some(format_deps(&t.deps)),
                ..Default::default()
            };
            format_ticket_line(t, opts)
        })
        .collect::<Vec<_>>()
        .join("\n");

    CommandOutput::new(serde_json::Value::Array(json_tickets))
        .with_text(text_output)
        .print(output_json)
}
