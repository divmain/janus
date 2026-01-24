//! File I/O helpers for commands

use crate::error::Result;
use crate::ticket::Ticket;
use std::fs;
use std::path::Path;

/// Wrapper for ticket file operations with consistent error handling
pub struct TicketFileOps;

impl TicketFileOps {
    /// Read ticket content with contextual error
    pub fn read_to_string(ticket: &Ticket) -> Result<String> {
        fs::read_to_string(&ticket.file_path).map_err(|e| {
            crate::error::JanusError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to read ticket at {}: {}",
                    ticket.file_path.display(),
                    e
                ),
            ))
        })
    }

    /// Write content to ticket with contextual error
    pub fn write_to_string(ticket: &Ticket, content: &str) -> Result<()> {
        fs::write(&ticket.file_path, content).map_err(|e| {
            crate::error::JanusError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to write ticket at {}: {}",
                    ticket.file_path.display(),
                    e
                ),
            ))
        })
    }

    /// Modify content in-place (read, modify, write)
    pub fn modify<F>(ticket: &Ticket, mut f: F) -> Result<()>
    where
        F: FnMut(&mut String),
    {
        let mut content = Self::read_to_string(ticket)?;
        f(&mut content);
        Self::write_to_string(ticket, &content)
    }

    /// Ensure parent directory exists
    pub fn ensure_parent_dir(path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                crate::error::JanusError::Io(std::io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to create directory for ticket at {}: {}",
                        parent.display(),
                        e
                    ),
                ))
            })
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_file_ops_read_write() {
        let temp_dir = TempDir::new().unwrap();
        let ticket_dir = temp_dir.path().join("tickets");
        fs::create_dir_all(&ticket_dir).unwrap();

        let file_path = ticket_dir.join("test-ticket.md");
        fs::write(&file_path, "original content").unwrap();

        let ticket = Ticket::new(file_path).unwrap();

        let content = TicketFileOps::read_to_string(&ticket).unwrap();
        assert_eq!(content, "original content");

        TicketFileOps::write_to_string(&ticket, "modified content").unwrap();
        let modified = TicketFileOps::read_to_string(&ticket).unwrap();
        assert_eq!(modified, "modified content");
    }

    #[test]
    fn test_file_ops_modify() {
        let temp_dir = TempDir::new().unwrap();
        let ticket_dir = temp_dir.path().join("tickets");
        fs::create_dir_all(&ticket_dir).unwrap();

        let file_path = ticket_dir.join("test-ticket.md");
        fs::write(&file_path, "hello ").unwrap();

        let ticket = Ticket::new(file_path).unwrap();

        let suffix = String::from("world");
        TicketFileOps::modify(&ticket, |content| {
            content.push_str(&suffix);
        })
        .unwrap();

        let result = TicketFileOps::read_to_string(&ticket).unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_file_ops_ensure_parent_dir() {
        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir.path().join("a/b/c/ticket.md");

        assert!(!nested_path.parent().unwrap().exists());
        TicketFileOps::ensure_parent_dir(&nested_path).unwrap();
        assert!(nested_path.parent().unwrap().exists());
    }

    #[test]
    fn test_file_ops_read_error() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("non-existent.md");

        let ticket = Ticket::new(file_path).unwrap();

        let result = TicketFileOps::read_to_string(&ticket);
        assert!(result.is_err());
    }
}
