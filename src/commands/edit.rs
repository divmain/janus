use std::process::Command;

use crate::error::Result;
use crate::ticket::Ticket;
use crate::utils::is_stdin_tty;

/// Open a ticket in the default editor
pub fn cmd_edit(id: &str) -> Result<()> {
    let ticket = Ticket::find(id)?;

    if is_stdin_tty() {
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

        let status = Command::new(&editor)
            .arg(&ticket.file_path)
            .status()?;

        if !status.success() {
            eprintln!("Editor exited with code {:?}", status.code());
        }
    } else {
        // Non-interactive mode: just print the file path
        println!("Edit ticket file: {}", ticket.file_path.display());
    }

    Ok(())
}
