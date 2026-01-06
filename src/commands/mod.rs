mod add_note;
pub mod create;
mod dep;
mod edit;
mod link;
mod ls;
mod query;
mod show;
mod status;

pub use add_note::cmd_add_note;
pub use create::{cmd_create, CreateOptions};
pub use dep::{cmd_dep_add, cmd_dep_remove, cmd_dep_tree};
pub use edit::cmd_edit;
pub use link::{cmd_link_add, cmd_link_remove};
pub use ls::{cmd_blocked, cmd_closed, cmd_ls, cmd_ready};
pub use query::cmd_query;
pub use show::cmd_show;
pub use status::{cmd_close, cmd_reopen, cmd_start, cmd_status};

use crate::types::{TicketMetadata, TicketStatus};
use owo_colors::OwoColorize;

/// Format options for ticket display
pub struct FormatOptions {
    pub show_priority: bool,
    pub suffix: Option<String>,
}

impl Default for FormatOptions {
    fn default() -> Self {
        FormatOptions {
            show_priority: false,
            suffix: None,
        }
    }
}

/// Format a ticket for single-line display
pub fn format_ticket_line(ticket: &TicketMetadata, options: FormatOptions) -> String {
    let id = ticket.id.as_deref().unwrap_or("???");
    let id_padded = format!("{:8}", id);

    let priority_str = if options.show_priority {
        format!("[P{}]", ticket.priority.map(|p| p.to_string()).unwrap_or("2".to_string()))
    } else {
        String::new()
    };

    let status = ticket.status.unwrap_or_default();
    let status_str = format!("[{}]", status);

    let title = ticket.title.as_deref().unwrap_or("");
    let suffix = options.suffix.unwrap_or_default();

    // Apply colors based on status
    let colored_status = match status {
        TicketStatus::New => status_str.yellow().to_string(),
        TicketStatus::Complete => status_str.green().to_string(),
        TicketStatus::Cancelled => status_str.dimmed().to_string(),
    };

    let colored_id = id_padded.cyan().to_string();

    // Color priority if P0 or P1
    let colored_priority = if options.show_priority {
        match ticket.priority.map(|p| p.as_num()) {
            Some(0) => priority_str.red().to_string(),
            Some(1) => priority_str.yellow().to_string(),
            _ => priority_str,
        }
    } else {
        priority_str
    };

    format!(
        "{} {}{} - {}{}",
        colored_id, colored_priority, colored_status, title, suffix
    )
}

/// Format dependencies for display
pub fn format_deps(deps: &[String]) -> String {
    let deps_str = deps.join(", ");
    if deps_str.is_empty() {
        " <- []".to_string()
    } else {
        format!(" <- [{}]", deps_str)
    }
}

/// Format a ticket as a bullet point (for show command sections)
pub fn format_ticket_bullet(ticket: &TicketMetadata) -> String {
    let id = ticket.id.as_deref().unwrap_or("???");
    let status = ticket.status.unwrap_or_default();
    let title = ticket.title.as_deref().unwrap_or("");
    format!("- {} [{}] {}", id.cyan(), status, title)
}

/// Sort tickets by priority (ascending) then by ID
pub fn sort_by_priority(tickets: &mut [TicketMetadata]) {
    tickets.sort_by(|a, b| {
        let pa = a.priority_num();
        let pb = b.priority_num();
        if pa != pb {
            pa.cmp(&pb)
        } else {
            a.id.cmp(&b.id)
        }
    });
}
