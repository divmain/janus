//! Entity trait for consistent interfaces between Ticket and Plan facades
//!
//! This trait provides a common interface for entity operations like finding,
//! reading, writing, and deleting tickets and plans.

use crate::error::Result;

/// Trait for entity operations shared between Ticket and Plan
///
/// This trait provides a consistent interface for entity operations,
/// allowing generic code to work with both tickets and plans.
pub trait Entity {
    /// The type of metadata returned by `read()`
    type Metadata;

    /// Find an entity by its partial ID
    ///
    /// # Arguments
    /// * `partial_id` - A partial or full entity ID (e.g., "j-a1" or "plan-abc")
    ///
    /// # Returns
    /// Returns the entity if found uniquely, otherwise an error
    fn find(partial_id: &str) -> impl std::future::Future<Output = Result<Self>> + Send
    where
        Self: Sized;

    /// Read and parse the entity's metadata
    ///
    /// # Returns
    /// Returns the parsed metadata for this entity
    fn read(&self) -> Result<Self::Metadata>;

    /// Write content to the entity file
    ///
    /// # Arguments
    /// * `content` - The full content to write to the file
    ///
    /// # Returns
    /// Returns Ok(()) on success, or an error if the write fails
    fn write(&self, content: &str) -> Result<()>;

    /// Delete the entity file
    ///
    /// # Returns
    /// Returns Ok(()) on success, or an error if the delete fails
    fn delete(&self) -> Result<()>;

    /// Check if the entity file exists
    ///
    /// # Returns
    /// Returns true if the file exists, false otherwise
    fn exists(&self) -> bool;
}
