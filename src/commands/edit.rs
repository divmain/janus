use serde_json::json;

use super::CommandOutput;
use crate::error::{JanusError, Result};
use crate::ticket::Ticket;
use crate::utils::{is_stdin_tty, open_in_editor};

/// Open a ticket in the default editor
pub async fn cmd_edit(id: &str, output_json: bool) -> Result<()> {
    let ticket = Ticket::find(id).await?;

    // Output in JSON format if requested (skip editor)
    if output_json {
        return CommandOutput::new(json!({
            "id": ticket.id,
            "file_path": ticket.file_path.to_string_lossy(),
            "action": "edit",
        }))
        .print(output_json);
    }

    if is_stdin_tty() {
        open_in_editor(&ticket.file_path)?;
    } else {
        return Err(JanusError::InteractiveTerminalRequired(
            ticket.file_path.clone(),
        ));
    }

    Ok(())
}
