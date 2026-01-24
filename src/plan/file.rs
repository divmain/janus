//! Plan file I/O operations
//!
//! Provides the `PlanFile` struct which implements `StorageHandle` and
//! `FileStorage` traits for standardized file I/O with hook support.

use std::path::PathBuf;

use crate::error::Result;
use crate::plan::locator::PlanLocator;
use crate::storage::{FileStorage, StorageHandle};
use crate::types::EntityType;

/// Handles plan file I/O operations.
///
/// `PlanFile` wraps a `PlanLocator` and implements the `StorageHandle` and
/// `FileStorage` traits, providing standardized read/write operations with
/// proper error handling and hook support.
#[derive(Clone)]
pub struct PlanFile {
    locator: PlanLocator,
}

impl PlanFile {
    /// Create a new `PlanFile` from a locator.
    pub fn new(locator: PlanLocator) -> Self {
        PlanFile { locator }
    }

    /// Create a `PlanFile` directly from a file path.
    ///
    /// # Arguments
    /// * `file_path` - Path to the plan file
    ///
    /// # Returns
    /// A `PlanFile` instance, or an error if the path is invalid.
    pub fn from_path(file_path: PathBuf) -> Result<Self> {
        Ok(PlanFile {
            locator: PlanLocator::new(file_path)?,
        })
    }

    /// Get a reference to the underlying locator.
    pub fn locator(&self) -> &PlanLocator {
        &self.locator
    }

    /// Get a reference to the file path.
    pub fn file_path(&self) -> &PathBuf {
        &self.locator.file_path
    }

    /// Get the plan ID.
    pub fn id(&self) -> &str {
        &self.locator.id
    }

    /// Read raw content from the plan file.
    ///
    /// This is a convenience method that delegates to `FileStorage::read_content`.
    pub fn read_raw(&self) -> Result<String> {
        FileStorage::read_content(self)
    }

    /// Write raw content to the plan file without hooks.
    ///
    /// This is a convenience method that delegates to `FileStorage::write_raw`.
    pub fn write_raw(&self, content: &str) -> Result<()> {
        FileStorage::write_raw(self, content)
    }
}

impl StorageHandle for PlanFile {
    fn file_path(&self) -> &std::path::Path {
        &self.locator.file_path
    }

    fn id(&self) -> &str {
        &self.locator.id
    }

    fn item_type(&self) -> EntityType {
        EntityType::Plan
    }
}

impl FileStorage for PlanFile {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_file_new() {
        let locator = PlanLocator::with_id("plan-test");
        let file = PlanFile::new(locator);
        assert_eq!(file.id(), "plan-test");
    }

    #[test]
    fn test_plan_file_from_path_valid() {
        let path = PathBuf::from("/path/to/plan-abc123.md");
        let result = PlanFile::from_path(path);
        assert!(result.is_ok());
        let file = result.unwrap();
        assert_eq!(file.id(), "plan-abc123");
    }

    #[test]
    fn test_plan_file_from_path_invalid() {
        let path = PathBuf::from("");
        let result = PlanFile::from_path(path);
        assert!(result.is_err());
    }

    #[test]
    fn test_plan_file_storage_handle() {
        let locator = PlanLocator::with_id("plan-test");
        let file = PlanFile::new(locator);

        // Test StorageHandle trait methods
        assert_eq!(file.id(), "plan-test");
        assert!(file.file_path().ends_with("plan-test.md"));
        assert!(matches!(file.item_type(), EntityType::Plan));
    }
}
