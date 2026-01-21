use std::fs;

use serde_json::json;

use super::CommandOutput;
use crate::error::{JanusError, Result};
use crate::events::log_note_added;
use crate::ticket::Ticket;
use crate::utils::{is_stdin_tty, iso_date, read_stdin};

/// Add a timestamped note to a ticket
pub async fn cmd_add_note(id: &str, note_text: Option<&str>, output_json: bool) -> Result<()> {
    let ticket = Ticket::find(id).await?;

    // Get note text from argument or stdin
    let note = if let Some(text) = note_text {
        text.to_string()
    } else if !is_stdin_tty() {
        read_stdin()?
    } else {
        String::new()
    };

    // Validate that note is not empty or only whitespace
    if note.trim().is_empty() {
        return Err(JanusError::EmptyNote);
    }

    let mut content = fs::read_to_string(&ticket.file_path).map_err(|e| {
        JanusError::Io(std::io::Error::new(
            e.kind(),
            format!(
                "Failed to read ticket at {}: {}",
                ticket.file_path.display(),
                e
            ),
        ))
    })?;

    // Add Notes section if it doesn't exist
    if !content.contains("## Notes") {
        content.push_str("\n## Notes");
    }

    // Add the note with timestamp
    let timestamp = iso_date();
    content.push_str(&format!("\n\n**{}**\n\n{}", timestamp, note));

    fs::write(&ticket.file_path, content).map_err(|e| {
        JanusError::Io(std::io::Error::new(
            e.kind(),
            format!(
                "Failed to write ticket at {}: {}",
                ticket.file_path.display(),
                e
            ),
        ))
    })?;

    // Log the event
    log_note_added(&ticket.id, &note);

    CommandOutput::new(json!({
        "id": ticket.id,
        "action": "note_added",
        "timestamp": timestamp,
        "note": note,
    }))
    .with_text(format!("Note added to {}", ticket.id))
    .print(output_json)
}
