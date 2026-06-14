//! Ticket locator module - handles finding and path resolution for tickets
//!
//! This module provides the `TicketLocator` type which encapsulates the relationship
//! between a ticket's ID and its file path on disk. It handles both finding existing
//! tickets by partial ID and creating locators for new tickets.

use std::path::{Path, PathBuf};

use crate::error::{JanusError, Result};
use crate::store::get_or_init_store_for;
use crate::types::{TicketId, janus_root, tickets_items_dir_in};
use crate::utils::extract_id_from_path;

fn validate_partial_id(id: &str) -> Result<String> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(JanusError::EmptyTicketId);
    }
    // Check for invalid characters (alphanumeric, hyphens, and underscores only)
    if !trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(JanusError::InvalidTicketIdCharacters);
    }
    Ok(trimmed.to_string())
}

/// Find a ticket by partial ID.
///
/// Searches for a ticket file matching the given partial ID in the tickets directory.
/// Returns the full path to the ticket file if found, or an error if not found
/// or if multiple tickets match (ambiguous).
/// Find a ticket file by partial ID within an explicit Janus root.
///
/// Resolves the items dir and the authoritative store against `root`, so lookups
/// target the right workspace when one process serves several. The ambient-root
/// lookup is [`TicketLocator::find`], which calls this with [`janus_root`].
async fn find_ticket_by_id_impl_in(partial_id: &str, root: &Path) -> Result<PathBuf> {
    let dir = tickets_items_dir_in(root);

    // Validate ID before any path construction using shared validation
    let _trimmed = validate_partial_id(partial_id)?;

    // Use store as authoritative source when available; filesystem fallback only when store fails
    let store = get_or_init_store_for(root).await?;

    // Exact match check - does file exist on disk?
    let exact_match_path = dir.join(format!("{partial_id}.md"));
    if exact_match_path.exists() {
        return Ok(exact_match_path);
    }

    // Partial match via store (store is authoritative)
    let matches = store.find_by_partial_id(partial_id);
    match matches.len() {
        0 => Err(JanusError::TicketNotFound(TicketId::new_unchecked(
            partial_id,
        ))),
        1 => Ok(dir.join(format!("{}.md", &matches[0]))),
        _ => Err(JanusError::AmbiguousTicketId(
            partial_id.to_string(),
            matches,
        )),
    }
}

/// Filesystem-based find implementation for tickets (fallback when store unavailable).
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
        Self::find_in(partial_id, &janus_root()).await
    }

    /// Find a ticket by its (partial) ID within an explicit Janus root.
    ///
    /// Same as [`find`](Self::find) but searches `root`'s tickets instead of the
    /// ambient [`janus_root`]'s, for serving multiple workspaces from one
    /// process.
    pub async fn find_in(partial_id: &str, root: &Path) -> Result<Self> {
        let partial_id = validate_partial_id(partial_id)?;
        let file_path = find_ticket_by_id_impl_in(&partial_id, root).await?;
        TicketLocator::new(file_path)
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
}
