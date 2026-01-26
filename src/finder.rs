//! Generic find-by-partial-ID implementation for Tickets and Plans.
//!
//! This module provides a generic async function for finding entities (tickets, plans)
//! by partial ID, eliminating code duplication between ticket and plan modules.
//!
//! The implementation follows this algorithm:
//! 1. Check cache for exact match (file exists)
//! 2. Check cache for partial matches
//! 3. Fall back to filesystem-based search if cache unavailable
//! 4. Handle exact vs partial matches
//! 5. Return ambiguous ID errors for multiple matches

use std::path::PathBuf;

use crate::cache;
use crate::error::Result;
use crate::utils::DirScanner;

/// Trait for types that can be found by partial ID.
///
/// This trait provides entity-specific information needed for the generic
/// find-by-partial-ID implementation.
pub trait Findable {
    /// The directory where entity files are stored (e.g., ".janus/items", ".janus/plans")
    /// Returns a PathBuf to support dynamic paths via JANUS_ROOT environment variable.
    fn directory() -> PathBuf;

    /// Find all entity files in the directory (returns filenames like "id.md")
    fn find_files() -> Vec<String> {
        let dir = Self::directory();
        DirScanner::find_markdown_files_from_path(&dir).unwrap_or_else(|e| {
            eprintln!("Warning: failed to read {} directory: {}", dir.display(), e);
            Vec::new()
        })
    }

    /// Find entity by partial ID using cache
    fn cache_find_by_partial_id(
        cache: &cache::TicketCache,
        partial_id: &str,
    ) -> impl std::future::Future<Output = Result<Vec<String>>> + Send;

    /// Create "not found" error for the entity type
    fn not_found_error(partial_id: String) -> crate::error::JanusError;

    /// Create "ambiguous ID" error for the entity type
    fn ambiguous_id_error(partial_id: String, matches: Vec<String>) -> crate::error::JanusError;
}

/// Generic find-by-partial-ID implementation.
///
/// This function implements the common algorithm used by both tickets and plans:
/// 1. Try cache for exact match (file exists)
/// 2. Try cache for partial matches
/// 3. Fall back to filesystem search
/// 4. Handle exact vs partial matches
/// 5. Return appropriate errors
pub async fn find_by_partial_id<T: Findable>(partial_id: &str) -> Result<PathBuf> {
    let dir = T::directory();

    // Try cache first
    if let Some(cache) = cache::get_or_init_cache().await {
        // Exact match check - does file exist?
        let exact_match_path = dir.join(format!("{}.md", partial_id));
        if exact_match_path.exists() {
            return Ok(exact_match_path);
        }

        // Partial match via cache
        if let Ok(matches) = T::cache_find_by_partial_id(cache, partial_id).await {
            match matches.len() {
                0 => {}
                1 => {
                    let filename = format!("{}.md", &matches[0]);
                    return Ok(dir.join(filename));
                }
                _ => {
                    return Err(T::ambiguous_id_error(partial_id.to_string(), matches));
                }
            }
        }
    }

    // FALLBACK: File-based implementation
    find_by_partial_id_impl::<T>(partial_id)
}

/// Filesystem-based find implementation (fallback when cache unavailable).
fn find_by_partial_id_impl<T: Findable>(partial_id: &str) -> Result<PathBuf> {
    let dir = T::directory();
    let files = T::find_files();

    // Check for exact match first
    let exact_name = format!("{}.md", partial_id);
    if files.iter().any(|f| f == &exact_name) {
        return Ok(dir.join(&exact_name));
    }

    // Then check for partial matches
    let matches: Vec<_> = files.iter().filter(|f| f.contains(partial_id)).collect();

    match matches.len() {
        0 => Err(T::not_found_error(partial_id.to_string())),
        1 => Ok(dir.join(matches[0])),
        _ => Err(T::ambiguous_id_error(
            partial_id.to_string(),
            matches.iter().map(|m| m.replace(".md", "")).collect(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock implementation for testing
    struct MockEntity;

    impl Findable for MockEntity {
        fn directory() -> PathBuf {
            PathBuf::from(".janus/test")
        }

        async fn cache_find_by_partial_id(
            _cache: &cache::TicketCache,
            _partial_id: &str,
        ) -> Result<Vec<String>> {
            Ok(vec![])
        }

        fn not_found_error(partial_id: String) -> crate::error::JanusError {
            crate::error::JanusError::Other(format!("mock not found: {}", partial_id))
        }

        fn ambiguous_id_error(
            partial_id: String,
            matches: Vec<String>,
        ) -> crate::error::JanusError {
            crate::error::JanusError::Other(format!(
                "mock ambiguous: {} matches {:?}",
                partial_id, matches
            ))
        }
    }

    #[test]
    fn test_findable_trait_methods() {
        assert_eq!(MockEntity::directory(), PathBuf::from(".janus/test"));
        let files = MockEntity::find_files();
        // Directory doesn't exist, should return empty vec
        assert_eq!(files.len(), 0);
    }
}
