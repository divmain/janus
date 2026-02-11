use serde_json::json;

use super::{CommandOutput, open_in_editor_for_entity};
use crate::error::Result;
use crate::ticket::Ticket;

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

    open_in_editor_for_entity("ticket", &ticket.file_path, output_json)
}
