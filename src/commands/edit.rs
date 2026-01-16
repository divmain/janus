use serde_json::json;

use super::print_json;
use crate::error::Result;
use crate::ticket::Ticket;
use crate::utils::{is_stdin_tty, open_in_editor};

/// Open a ticket in the default editor
pub fn cmd_edit(id: &str, output_json: bool) -> Result<()> {
    let ticket = Ticket::find(id)?;

    if output_json {
        print_json(&json!({
            "id": ticket.id,
            "file_path": ticket.file_path.to_string_lossy(),
            "action": "edit",
        }))?;
        return Ok(());
    }

    if is_stdin_tty() {
        open_in_editor(&ticket.file_path)?;
    } else {
        // Non-interactive mode: just print the file path
        println!("Edit ticket file: {}", ticket.file_path.display());
    }

    Ok(())
}
