use serde_json::json;

use super::CommandOutput;
use crate::cli::OutputOptions;
use crate::error::{JanusError, Result};
use crate::ticket::Ticket;
use crate::utils::{is_stdin_tty, iso_date, read_stdin};

/// Add a timestamped note to a ticket
pub async fn cmd_add_note(id: &str, note_text: Option<&str>, output: OutputOptions) -> Result<()> {
    let ticket = Ticket::find(id).await?;

    // Get note text from argument or stdin
    let note = if let Some(text) = note_text {
        text.to_string()
    } else if !is_stdin_tty() {
        read_stdin()?
    } else {
        return Err(JanusError::EmptyNote);
    };

    // Validate that note content is not empty or whitespace-only
    if note.trim().is_empty() {
        return Err(JanusError::EmptyNote);
    }

    // Validate that note does not exceed maximum length
    const MAX_NOTE_LENGTH: usize = 10000;
    if note.len() > MAX_NOTE_LENGTH {
        return Err(JanusError::NoteTooLong {
            max: MAX_NOTE_LENGTH,
            actual: note.len(),
        });
    }

    // Use the shared add_note method on Ticket
    ticket.add_note(&note)?;

    let timestamp = iso_date();

    CommandOutput::new(json!({
        "id": ticket.id,
        "action": "note_added",
        "timestamp": timestamp,
        "note": note,
    }))
    .with_text(format!("Note added to {}", ticket.id))
    .print(output)
}
