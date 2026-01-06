use std::fs;

use crate::error::Result;
use crate::ticket::Ticket;
use crate::utils::{is_stdin_tty, iso_date, read_stdin};

/// Add a timestamped note to a ticket
pub fn cmd_add_note(id: &str, note_text: Option<&str>) -> Result<()> {
    let ticket = Ticket::find(id)?;

    // Get note text from argument or stdin
    let note = if let Some(text) = note_text {
        text.to_string()
    } else if !is_stdin_tty() {
        read_stdin()?
    } else {
        String::new()
    };

    let mut content = fs::read_to_string(&ticket.file_path)?;

    // Add Notes section if it doesn't exist
    if !content.contains("## Notes") {
        content.push_str("\n## Notes");
    }

    // Add the note with timestamp
    let timestamp = iso_date();
    content.push_str(&format!("\n\n**{}**\n\n{}", timestamp, note));

    fs::write(&ticket.file_path, content)?;
    println!("Note added to {}", ticket.id);

    Ok(())
}
