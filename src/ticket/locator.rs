use crate::cache;
use crate::error::{JanusError, Result};
use crate::finder::Findable;
use crate::locator::Locator;
use crate::locator::TicketEntity;
use crate::types::tickets_items_dir;
use crate::utils::validate_identifier;
use std::path::PathBuf;

/// Ticket-specific implementation of the Findable trait
struct TicketFinder;

impl Findable for TicketFinder {
    fn directory() -> PathBuf {
        tickets_items_dir()
    }

    fn cache_find_by_partial_id(
        cache: &cache::TicketCache,
        partial_id: &str,
    ) -> impl std::future::Future<Output = Result<Vec<String>>> + Send {
        cache.find_by_partial_id(partial_id)
    }

    fn not_found_error(partial_id: String) -> JanusError {
        JanusError::TicketNotFound(partial_id)
    }

    fn ambiguous_id_error(partial_id: String, matches: Vec<String>) -> JanusError {
        JanusError::AmbiguousId(partial_id, matches)
    }
}

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
    crate::finder::find_by_partial_id::<TicketFinder>(&partial_id).await
}

/// Type alias for ticket locator using the generic Locator
pub type TicketLocator = Locator<TicketEntity>;

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
}
