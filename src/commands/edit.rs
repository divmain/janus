use serde_json::json;

use super::CommandOutput;
use crate::error::Result;
use crate::ticket::Ticket;
use crate::utils::{is_stdin_tty, open_in_editor};

/// Open a ticket in the default editor
pub async fn cmd_edit(id: &str, output_json: bool) -> Result<()> {
    let ticket = Ticket::find(id).await?;

    if is_stdin_tty() {
        open_in_editor(&ticket.file_path)?;
    } else {
        // Non-interactive mode: just print the file path
        println!("Edit ticket file: {}", ticket.file_path.display());
    }

    // Output in JSON format if requested
    if output_json {
        return CommandOutput::new(json!({
            "id": ticket.id,
            "file_path": ticket.file_path.to_string_lossy(),
            "action": "edit",
        }))
        .print(output_json);
    }

    Ok(())
}
