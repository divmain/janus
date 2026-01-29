use crate::error::{JanusError, Result};
use crate::locator::ticket_path;
use crate::utils::{extract_id_from_path, validate_identifier};
use std::path::PathBuf;

fn validate_partial_id(id: &str) -> Result<String> {
    // Use the generic identifier validator and convert errors to ticket-specific types
    validate_identifier(id, "Ticket ID").map_err(|e| {
        // Map generic errors to specific ticket errors
        let msg = e.to_string();
        if msg.contains("cannot be empty") {
            JanusError::EmptyTicketId
        } else {
            JanusError::InvalidTicketIdCharacters
        }
    })
}

pub async fn find_ticket_by_id(partial_id: &str) -> Result<PathBuf> {
    let partial_id = validate_partial_id(partial_id)?;
    crate::finder::find_ticket_by_id(&partial_id).await
}

/// Simple locator for ticket files
///
/// Encapsulates the relationship between a ticket's ID and its file path on disk.
#[derive(Debug, Clone)]
pub struct TicketLocator {
    pub file_path: PathBuf,
    pub id: String,
}

impl TicketLocator {
    /// Create a locator from an existing file path
    ///
    /// Extracts the ticket ID from the file path's stem.
    pub fn new(file_path: PathBuf) -> Result<Self> {
        let id = extract_id_from_path(&file_path, "ticket")?;
        Ok(TicketLocator { file_path, id })
    }

    /// Find a ticket by its (partial) ID
    ///
    /// Searches for a ticket matching the given partial ID.
    pub async fn find(partial_id: &str) -> Result<Self> {
        let file_path = find_ticket_by_id(partial_id).await?;
        TicketLocator::new(file_path)
    }

    /// Create a locator for a new ticket with the given ID
    ///
    /// This is used when creating new tickets. The file does not need to exist.
    pub fn with_id(id: &str) -> Self {
        TicketLocator {
            file_path: ticket_path(id),
            id: id.to_string(),
        }
    }

    /// Get the file path for a given ticket ID
    ///
    /// Does not verify that the file exists.
    pub fn file_path_for_id(id: &str) -> PathBuf {
        ticket_path(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_partial_id_empty() {
        let result = validate_partial_id("");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::EmptyTicketId => {}
            _ => panic!("Expected EmptyTicketId error for empty ID"),
        }
    }

    #[test]
    fn test_validate_partial_id_whitespace() {
        let result = validate_partial_id("   ");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::EmptyTicketId => {}
            _ => panic!("Expected EmptyTicketId error for whitespace-only ID"),
        }
    }

    #[test]
    fn test_validate_partial_id_special_chars() {
        let result = validate_partial_id("j@b1");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::InvalidTicketIdCharacters => {}
            _ => panic!("Expected InvalidTicketIdCharacters error for invalid characters"),
        }
    }

    #[test]
    fn test_validate_partial_id_valid() {
        let result = validate_partial_id("j-a1b2");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "j-a1b2");
    }

    #[test]
    fn test_validate_partial_id_valid_with_underscore() {
        let result = validate_partial_id("j_a1b2");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "j_a1b2");
    }

    #[test]
    fn test_validate_partial_id_trims_whitespace() {
        let result = validate_partial_id("  j-a1b2  ");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "j-a1b2");
    }

    #[test]
    fn test_ticket_locator_new_valid_path() {
        let path = PathBuf::from("/path/to/j-a1b2.md");
        let result = TicketLocator::new(path.clone());
        assert!(result.is_ok());
        let locator = result.unwrap();
        assert_eq!(locator.id, "j-a1b2");
        assert_eq!(locator.file_path, path);
    }

    #[test]
    fn test_ticket_locator_new_valid_path_with_underscores() {
        let path = PathBuf::from("/path/to/ticket_123.md");
        let result = TicketLocator::new(path.clone());
        assert!(result.is_ok());
        let locator = result.unwrap();
        assert_eq!(locator.id, "ticket_123");
        assert_eq!(locator.file_path, path);
    }

    #[test]
    fn test_ticket_locator_with_id() {
        let locator = TicketLocator::with_id("j-test");
        assert_eq!(locator.id, "j-test");
        assert!(locator.file_path.ends_with("j-test.md"));
    }

    #[test]
    fn test_ticket_locator_file_path_for_id() {
        let path = TicketLocator::file_path_for_id("j-test");
        assert!(path.ends_with("j-test.md"));
        assert!(path.to_string_lossy().contains("items"));
    }
}
