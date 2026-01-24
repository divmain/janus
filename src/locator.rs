//! Generic entity locator for tickets and plans
//!
//! This module provides a `Locator<T>` that works with any entity type (ticket, plan)
//! using a marker trait pattern to eliminate duplication between `TicketLocator` and
//! `PlanLocator`.

use std::path::PathBuf;

use crate::error::Result;
use crate::utils::extract_id_from_path;

/// Marker trait for entity types that can be located
pub trait LocatableEntity: Send + Sync {
    /// The human-readable entity type name (e.g., "ticket", "plan")
    fn entity_type_name() -> &'static str;

    /// The directory where entity files are stored
    fn directory() -> PathBuf;

    /// Construct the file path for a given entity ID
    fn file_path_for_id(id: &str) -> PathBuf {
        Self::directory().join(format!("{}.md", id))
    }

    /// Find an entity by partial ID
    #[allow(clippy::manual_async_fn)]
    fn find_by_partial_id(
        partial_id: &str,
    ) -> impl std::future::Future<Output = Result<PathBuf>> + Send;
}

/// Marker type for tickets
#[derive(Debug, Clone)]
pub struct TicketEntity;

impl LocatableEntity for TicketEntity {
    fn entity_type_name() -> &'static str {
        "ticket"
    }

    fn directory() -> PathBuf {
        crate::types::tickets_items_dir()
    }

    #[allow(clippy::manual_async_fn)]
    fn find_by_partial_id(
        partial_id: &str,
    ) -> impl std::future::Future<Output = Result<PathBuf>> + Send {
        async move { crate::ticket::find_ticket_by_id(partial_id).await }
    }
}

/// Marker type for plans
#[derive(Debug, Clone)]
pub struct PlanEntity;

impl LocatableEntity for PlanEntity {
    fn entity_type_name() -> &'static str {
        "plan"
    }

    fn directory() -> PathBuf {
        crate::types::plans_dir()
    }

    #[allow(clippy::manual_async_fn)]
    fn find_by_partial_id(
        partial_id: &str,
    ) -> impl std::future::Future<Output = Result<PathBuf>> + Send {
        async move { crate::plan::find_plan_by_id(partial_id).await }
    }
}

/// Generic locator for entity files
///
/// `Locator<T>` encapsulates the relationship between an entity's ID and its
/// file path on disk. The generic parameter `T` is the entity type marker
/// (e.g., `TicketEntity`, `PlanEntity`).
#[derive(Debug, Clone)]
pub struct Locator<T: LocatableEntity> {
    pub file_path: PathBuf,
    pub id: String,
    _marker: std::marker::PhantomData<T>,
}

impl<T: LocatableEntity> Locator<T> {
    /// Create a locator from an existing file path
    ///
    /// Extracts the entity ID from the file path's stem.
    pub fn new(file_path: PathBuf) -> Result<Self> {
        let id = extract_id_from_path(&file_path, T::entity_type_name())?;
        Ok(Locator {
            file_path,
            id,
            _marker: std::marker::PhantomData,
        })
    }

    /// Find an entity by its (partial) ID
    ///
    /// Searches for an entity matching the given partial ID.
    pub async fn find(partial_id: &str) -> Result<Self> {
        let file_path = T::find_by_partial_id(partial_id).await?;
        Locator::new(file_path)
    }

    /// Create a locator for a new entity with the given ID
    ///
    /// This is used when creating new entities. The file does not need to exist.
    pub fn with_id(id: &str) -> Self {
        Locator {
            file_path: T::file_path_for_id(id),
            id: id.to_string(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Get the file path for a given entity ID
    ///
    /// Does not verify that the file exists.
    pub fn file_path_for_id(id: &str) -> PathBuf {
        T::file_path_for_id(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::JanusError;

    #[test]
    fn test_ticket_locator_new_valid_path() {
        let path = PathBuf::from("/path/to/j-a1b2.md");
        let result = Locator::<TicketEntity>::new(path.clone());
        assert!(result.is_ok());
        let locator = result.unwrap();
        assert_eq!(locator.id, "j-a1b2");
        assert_eq!(locator.file_path, path);
    }

    #[test]
    fn test_ticket_locator_new_invalid_empty_path() {
        let path = PathBuf::from("");
        let result = Locator::<TicketEntity>::new(path);
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::InvalidFormat(msg) => {
                assert!(msg.contains("Invalid ticket file path"));
            }
            _ => panic!("Expected InvalidFormat error"),
        }
    }

    #[test]
    fn test_plan_locator_new_valid_path() {
        let path = PathBuf::from("/path/to/plan-a1b2.md");
        let result = Locator::<PlanEntity>::new(path.clone());
        assert!(result.is_ok());
        let locator = result.unwrap();
        assert_eq!(locator.id, "plan-a1b2");
        assert_eq!(locator.file_path, path);
    }

    #[test]
    fn test_plan_locator_new_invalid_empty_path() {
        let path = PathBuf::from("");
        let result = Locator::<PlanEntity>::new(path);
        assert!(result.is_err());
    }

    #[test]
    fn test_locator_with_id() {
        let ticket_locator = Locator::<TicketEntity>::with_id("j-test");
        assert_eq!(ticket_locator.id, "j-test");
        assert!(ticket_locator.file_path.ends_with("j-test.md"));

        let plan_locator = Locator::<PlanEntity>::with_id("plan-test");
        assert_eq!(plan_locator.id, "plan-test");
        assert!(plan_locator.file_path.ends_with("plan-test.md"));
    }

    #[test]
    fn test_locator_file_path_for_id() {
        let path = Locator::<PlanEntity>::file_path_for_id("plan-test");
        assert!(path.ends_with("plan-test.md"));
        assert!(path.to_string_lossy().contains("plans"));
    }
}
