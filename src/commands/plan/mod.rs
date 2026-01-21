//! Plan command implementations
//!
//! This module implements plan commands:
//! - `plan create` - Create a new plan
//! - `plan show` - Display a plan with full reconstruction
//! - `plan edit` - Open plan in $EDITOR
//! - `plan ls` - List all plans
//! - `plan add-ticket` - Add a ticket to a plan
//! - `plan remove-ticket` - Remove a ticket from a plan
//! - `plan move-ticket` - Move a ticket between phases
//! - `plan add-phase` - Add a new phase to a plan
//! - `plan remove-phase` - Remove a phase from a plan
//! - `plan reorder` - Reorder tickets or phases
//! - `plan delete` - Delete a plan
//! - `plan rename` - Rename a plan
//! - `plan next` - Show the next actionable item(s)
//! - `plan status` - Show plan status summary
//! - `plan import` - Import an AI-generated plan document
//! - `plan import-spec` - Show the importable plan format specification

mod create;
mod delete;
mod edit;
mod import;
mod ls;
mod next;
mod phases;
mod reorder;
mod show;
mod status;
mod tickets;

pub use create::cmd_plan_create;
pub use delete::{cmd_plan_delete, cmd_plan_rename};
pub use edit::cmd_plan_edit;
pub use import::{cmd_plan_import, cmd_show_import_spec};
pub use ls::cmd_plan_ls;
pub use next::{NextItemResult, cmd_plan_next, get_next_items_phased, get_next_items_simple};
pub use phases::{cmd_plan_add_phase, cmd_plan_remove_phase};
pub use reorder::cmd_plan_reorder;
pub use show::cmd_plan_show;
pub use status::cmd_plan_status;
pub use tickets::{cmd_plan_add_ticket, cmd_plan_move_ticket, cmd_plan_remove_ticket};

use std::collections::HashMap;
use std::io::{Read, Write};

use owo_colors::OwoColorize;

use crate::display::format_status_colored;
use crate::error::{JanusError, Result};
use crate::types::{TicketMetadata, TicketStatus};
use crate::utils::{is_stdin_tty, open_in_editor};

// ============================================================================
// Shared Helper Functions
// ============================================================================

/// Print a ticket line with status for plan show command
///
/// # Arguments
/// * `index` - The 1-based index of the ticket in the list
/// * `ticket_id` - The ticket ID
/// * `ticket_map` - Map of ticket IDs to metadata
/// * `full_summary` - If true, show full completion summary; if false, show only first 2 lines
pub(crate) fn print_ticket_line(
    index: usize,
    ticket_id: &str,
    ticket_map: &HashMap<String, TicketMetadata>,
    full_summary: bool,
) {
    if let Some(ticket) = ticket_map.get(ticket_id) {
        let status = ticket.status.unwrap_or_default();
        let status_badge = format_status_colored(status);
        let title = ticket.title.as_deref().unwrap_or("");

        println!(
            "{}. {} {} - {}",
            index,
            status_badge,
            ticket_id.cyan(),
            title
        );

        // Print completion summary if complete and has one
        if status == TicketStatus::Complete
            && let Some(ref summary) = ticket.completion_summary
        {
            // Print as indented blockquote
            if full_summary {
                // Print all lines
                for line in summary.lines() {
                    println!("   > {}", line.dimmed());
                }
            } else {
                // Print only first 2 lines
                for line in summary.lines().take(2) {
                    println!("   > {}", line.dimmed());
                }
            }
        }
    } else {
        // Missing ticket
        println!("{}. {} {}", index, "[missing]".red(), ticket_id.dimmed());
    }
}

/// Open content in an editor and return the edited content
pub(crate) fn edit_in_editor(content: &str) -> Result<String> {
    if !is_stdin_tty() {
        return Err(JanusError::InteractiveTerminalRequired);
    }

    // Create a temp file
    let mut temp_file = tempfile::NamedTempFile::new()?;
    temp_file.write_all(content.as_bytes())?;
    temp_file.flush()?;

    let temp_path = temp_file.path().to_path_buf();

    // Open in editor
    open_in_editor(&temp_path)?;

    // Read the edited content
    let mut edited = String::new();
    std::fs::File::open(&temp_path)
        .map_err(|e| {
            JanusError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to open temp file for editing at {}: {}",
                    temp_path.display(),
                    e
                ),
            ))
        })?
        .read_to_string(&mut edited)
        .map_err(|e| {
            JanusError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to read temp file at {}: {}", temp_path.display(), e),
            ))
        })?;

    Ok(edited)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_status_colored() {
        // Just verify it doesn't panic for all statuses
        let statuses = [
            TicketStatus::New,
            TicketStatus::Next,
            TicketStatus::InProgress,
            TicketStatus::Complete,
            TicketStatus::Cancelled,
        ];

        for status in statuses {
            let badge = format_status_colored(status);
            assert!(badge.contains(&status.to_string()));
        }
    }
}
