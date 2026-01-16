mod add_note;
mod board;
mod cache;
mod config;
pub mod create;
mod dep;
mod edit;
mod hook;
mod link;
mod ls;
mod plan;
mod query;
mod remote_browse;
mod set;
mod show;
mod status;
mod sync;
mod view;

pub use add_note::cmd_add_note;
pub use board::cmd_board;
pub use cache::{cmd_cache_clear, cmd_cache_path, cmd_cache_rebuild, cmd_cache_status};
pub use config::{cmd_config_get, cmd_config_set, cmd_config_show};
pub use create::{CreateOptions, cmd_create};
pub use dep::{cmd_dep_add, cmd_dep_remove, cmd_dep_tree};
pub use edit::cmd_edit;
pub use hook::{cmd_hook_disable, cmd_hook_enable, cmd_hook_install, cmd_hook_list, cmd_hook_run};
pub use link::{cmd_link_add, cmd_link_remove};
pub use ls::cmd_ls;
pub use plan::{
    cmd_plan_add_phase, cmd_plan_add_ticket, cmd_plan_create, cmd_plan_delete, cmd_plan_edit,
    cmd_plan_import, cmd_plan_ls, cmd_plan_move_ticket, cmd_plan_next, cmd_plan_remove_phase,
    cmd_plan_remove_ticket, cmd_plan_rename, cmd_plan_reorder, cmd_plan_show, cmd_plan_status,
    cmd_show_import_spec,
};
pub use query::cmd_query;
pub use remote_browse::cmd_remote_browse;
pub use set::cmd_set;
pub use show::cmd_show;
pub use status::{cmd_close, cmd_reopen, cmd_start, cmd_status};
pub use sync::{cmd_adopt, cmd_push, cmd_remote_link, cmd_sync};
pub use view::cmd_view;

use crate::error::Result;
use crate::types::{TicketMetadata, TicketStatus};
use owo_colors::OwoColorize;
use serde_json::json;

/// Print a JSON value as pretty-printed output
///
/// This helper centralizes JSON output formatting for all commands,
/// ensuring consistent output structure and reducing boilerplate.
pub fn print_json(value: &serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// Convert a ticket metadata to JSON value
pub fn ticket_to_json(ticket: &TicketMetadata) -> serde_json::Value {
    json!({
        "id": ticket.id,
        "uuid": ticket.uuid,
        "title": ticket.title,
        "status": ticket.status.map(|s| s.to_string()),
        "deps": ticket.deps,
        "links": ticket.links,
        "created": ticket.created,
        "type": ticket.ticket_type.map(|t| t.to_string()),
        "priority": ticket.priority.map(|p| p.to_string()),
        "external-ref": ticket.external_ref,
        "parent": ticket.parent,
        "filePath": ticket.file_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        "remote": ticket.remote,
        "completion_summary": ticket.completion_summary,
    })
}

/// Format options for ticket display
#[derive(Default)]
pub struct FormatOptions {
    pub show_priority: bool,
    pub suffix: Option<String>,
}

/// Format a ticket for single-line display
pub fn format_ticket_line(ticket: &TicketMetadata, options: FormatOptions) -> String {
    let id = ticket.id.as_deref().unwrap_or("???");
    let id_padded = format!("{:8}", id);

    let priority_str = if options.show_priority {
        format!(
            "[P{}]",
            ticket
                .priority
                .map(|p| p.to_string())
                .unwrap_or("2".to_string())
        )
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
        TicketStatus::Next => status_str.magenta().to_string(),
        TicketStatus::InProgress => status_str.cyan().to_string(),
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
