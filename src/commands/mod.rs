use crate::cli::OutputOptions;

mod add_note;
mod board;
mod cache;
mod config;
pub mod create;
mod dep;
mod dep_tree;
mod doc;
mod doctor;
mod edit;
mod events;
pub mod graph;
pub mod hook;
pub mod interactive;

mod link;
mod ls;
mod next;
mod plan;
mod query;
mod remote_browse;
pub mod search;
mod set;
mod show;
mod status;
pub mod sync;
mod view;

pub use add_note::cmd_add_note;
pub use board::cmd_board;
pub use cache::{cmd_cache_prune, cmd_cache_rebuild, cmd_cache_status};
pub use config::{cmd_config_get, cmd_config_set, cmd_config_show};
pub use create::{CreateOptions, cmd_create};
pub use dep::{cmd_dep_add, cmd_dep_remove, cmd_dep_tree};
pub use doc::{cmd_doc_create, cmd_doc_edit, cmd_doc_ls, cmd_doc_search, cmd_doc_show};
pub use doctor::cmd_doctor;
pub use edit::cmd_edit;
pub use events::cmd_events_prune;
pub use graph::cmd_graph;
pub use hook::{
    cmd_hook_disable, cmd_hook_enable, cmd_hook_install, cmd_hook_list, cmd_hook_log, cmd_hook_run,
};
pub use link::{cmd_link_add, cmd_link_remove};
pub use ls::{LsOptions, cmd_ls_with_options};
pub use next::cmd_next;
pub use plan::{
    NextItemResult, cmd_plan_add_phase, cmd_plan_add_ticket, cmd_plan_create, cmd_plan_delete,
    cmd_plan_edit, cmd_plan_import, cmd_plan_ls, cmd_plan_move_ticket, cmd_plan_next,
    cmd_plan_remove_phase, cmd_plan_remove_ticket, cmd_plan_rename, cmd_plan_reorder,
    cmd_plan_show, cmd_plan_status, cmd_plan_verify, cmd_show_import_spec, get_next_items_phased,
    get_next_items_simple,
};
pub use query::cmd_query;
pub use remote_browse::cmd_remote_browse;
pub use search::cmd_search;
pub use set::cmd_set;
pub use show::cmd_show;
pub use status::{cmd_close, cmd_reopen, cmd_start, cmd_status};
pub use sync::{cmd_adopt, cmd_push, cmd_remote_link, cmd_sync};
pub use view::cmd_view;

use std::path::Path;

use crate::error::Result;
use crate::types::{TicketMetadata, TicketSize};
use crate::utils::{format_relative_path, is_stdin_tty, open_in_editor};
use serde_json::json;

/// Open a file in the default editor with appropriate messaging for the entity type.
///
/// This helper eliminates the duplicate TTY checking and messaging logic between
/// ticket and plan edit commands.
///
/// # Arguments
/// * `entity_type` - The type of entity being edited (e.g., "ticket", "plan")
/// * `file_path` - The path to the file to open
/// * `output` - Output options for controlling JSON output
///
/// # Errors
/// Returns an error if the editor fails to open
pub fn open_in_editor_for_entity(
    entity_type: &str,
    file_path: &Path,
    output: OutputOptions,
) -> Result<()> {
    if output.json {
        return Ok(());
    }

    if is_stdin_tty() {
        open_in_editor(file_path)?;
    } else {
        // Non-interactive mode: just print the file path
        println!(
            "Edit {} file: {}",
            entity_type,
            format_relative_path(file_path)
        );
    }

    Ok(())
}

/// Format a size value for display
/// Returns the size string if present, "-" if None
pub fn format_size(size: Option<TicketSize>) -> String {
    size.map(|s| s.to_string()).unwrap_or_else(|| "-".into())
}

/// Unified output abstraction for commands that support both JSON and text output.
///
/// This eliminates the repeated pattern of:
/// ```ignore
/// if output_json {
///     print_json(&json!({ ... }))?;
/// } else {
///     println!("{}", text);
/// }
/// ```
///
/// Instead, commands can use:
/// ```ignore
/// CommandOutput::new(json!({ ... }))
///     .with_text("Human readable text")
///     .print(output_json)
/// ```
pub struct CommandOutput {
    json: serde_json::Value,
    text: Option<String>,
}

impl CommandOutput {
    /// Create a new CommandOutput with JSON data.
    ///
    /// If no text is provided, the JSON will be pretty-printed for text output too.
    pub fn new(json: serde_json::Value) -> Self {
        Self { json, text: None }
    }

    /// Set the human-readable text output.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Print the output in the appropriate format.
    pub fn print(self, output: OutputOptions) -> Result<()> {
        if output.json {
            print_json(&self.json)?;
        } else if let Some(text) = self.text {
            println!("{text}");
        } else {
            // Fallback: pretty-print JSON for text output
            println!("{}", serde_json::to_string_pretty(&self.json)?);
        }
        Ok(())
    }
}

/// Re-export display formatting functions for convenience
pub use crate::display::{FormatOptions, format_deps, format_ticket_bullet, format_ticket_line};
pub use crate::query::{sort_by_priority, sort_tickets_by};

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
        "priority": ticket.priority.map(|p| p.as_num()),
        "size": ticket.size.map(|s| s.to_string()),
        "external_ref": ticket.external_ref,
        "parent": ticket.parent,
        "file_path": ticket.file_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        "remote": ticket.remote,
        "completion_summary": ticket.completion_summary,
        "spawned_from": ticket.spawned_from,
        "depth": ticket.depth,
    })
}

/// Create minimal ticket JSON object with basic fields
///
/// Used for ticket references in lists, dependencies, and relationships.
pub fn ticket_minimal_json(ticket: &TicketMetadata) -> serde_json::Value {
    json!({
        "id": ticket.id,
        "title": ticket.title,
        "status": ticket.status.map(|s| s.to_string()),
    })
}

/// Create minimal ticket JSON object with exists flag
///
/// Used when tickets may not exist (e.g., in plan views where tickets are
/// referenced but may be deleted or not yet created).
pub fn ticket_minimal_json_with_exists(
    ticket_id: &str,
    ticket: Option<&TicketMetadata>,
) -> serde_json::Value {
    json!({
        "id": ticket_id,
        "status": ticket.and_then(|t| t.status).map(|s| s.to_string()),
        "title": ticket.and_then(|t| t.title.clone()),
        "exists": ticket.is_some(),
    })
}

/// Display initialization warnings from the store if any occurred.
///
/// This should be called by commands that use the store and are in text (non-JSON) mode.
/// The warnings inform users about files that were skipped during store initialization
/// due to parse errors, IO errors, or other non-fatal issues.
///
/// # Arguments
/// * `store` - The initialized ticket store
///
/// # Example
/// ```ignore
/// let store = get_or_init_store().await?;
/// if !output_json {
///     display_init_warnings(&store);
/// }
/// ```
pub fn display_init_warnings(store: &crate::store::TicketStore) {
    let warnings = store.get_init_warnings();
    let ticket_warnings = warnings.ticket_warnings();
    let plan_warnings = warnings.plan_warnings();
    let total_skipped = ticket_warnings.len() + plan_warnings.len();

    if total_skipped == 0 {
        return;
    }

    let ticket_count = store.get_all_tickets().len();
    let plan_count = store.get_all_plans().len();

    eprintln!(
        "Loaded {ticket_count} tickets and {plan_count} plans ({total_skipped} files skipped due to errors)"
    );

    // Show detailed warnings in debug mode
    #[cfg(debug_assertions)]
    {
        for warning in ticket_warnings.iter().chain(plan_warnings.iter()) {
            let file_str = warning
                .file_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            eprintln!(
                "  Warning [{entity}]: {file_str}: {message}",
                entity = warning.entity_type,
                file_str = file_str,
                message = warning.message
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size_with_value() {
        assert_eq!(format_size(Some(TicketSize::XSmall)), "xsmall");
        assert_eq!(format_size(Some(TicketSize::Small)), "small");
        assert_eq!(format_size(Some(TicketSize::Medium)), "medium");
        assert_eq!(format_size(Some(TicketSize::Large)), "large");
        assert_eq!(format_size(Some(TicketSize::XLarge)), "xlarge");
    }

    #[test]
    fn test_format_size_with_none() {
        assert_eq!(format_size(None), "-");
    }
}
