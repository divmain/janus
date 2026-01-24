//! Generic storage abstractions for file-based entities

use crate::error::{JanusError, Result};
use crate::hooks::{HookContext, HookEvent, run_post_hooks, run_pre_hooks};
use crate::types::EntityType;
use std::path::Path;

/// Generic trait for file-based storage with hook support
pub trait StorageHandle {
    /// Get the file path for this storage item
    fn file_path(&self) -> &Path;

    /// Get the item ID
    fn id(&self) -> &str;

    /// Get the item type for hooks
    fn item_type(&self) -> EntityType;
}

/// Common file I/O operations applicable to both tickets and plans
pub trait FileStorage: StorageHandle {
    /// Read raw content with context-aware error handling
    fn read_content(&self) -> Result<String> {
        std::fs::read_to_string(self.file_path()).map_err(|e| {
            JanusError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to read {} at {}: {}",
                    match self.item_type() {
                        EntityType::Ticket => "ticket",
                        EntityType::Plan => "plan",
                    },
                    self.file_path().display(),
                    e
                ),
            ))
        })
    }

    /// Write content with hooks, directory creation, and error handling
    fn write_with_hooks(&self, content: &str, with_hooks: bool) -> Result<()> {
        if with_hooks {
            let context = self.hook_context();
            run_pre_hooks(HookEvent::PreWrite, &context)?;
        }

        self.write_raw(content)?;

        if with_hooks {
            let context = self.hook_context();
            run_post_hooks(HookEvent::PostWrite, &context);
        }

        Ok(())
    }

    /// Write raw content without hooks
    fn write_raw(&self, content: &str) -> Result<()> {
        self.ensure_parent_dir()?;
        std::fs::write(self.file_path(), content).map_err(|e| {
            JanusError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to write {} at {}: {}",
                    match self.item_type() {
                        EntityType::Ticket => "ticket",
                        EntityType::Plan => "plan",
                    },
                    self.file_path().display(),
                    e
                ),
            ))
        })
    }

    /// Ensure parent directory exists
    fn ensure_parent_dir(&self) -> Result<()> {
        if let Some(parent) = self.file_path().parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent).map_err(|e| {
                JanusError::Io(std::io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to create directory for {} at {}: {}",
                        match self.item_type() {
                            EntityType::Ticket => "ticket",
                            EntityType::Plan => "plan",
                        },
                        parent.display(),
                        e
                    ),
                ))
            })?;
        }
        Ok(())
    }

    /// Build a hook context for this item
    fn hook_context(&self) -> HookContext {
        HookContext::new()
            .with_item_type(self.item_type())
            .with_item_id(self.id())
            .with_file_path(self.file_path())
    }
}

/// Execute an operation with standard write hooks.
///
/// Runs PreWrite hook before the operation, then PostWrite hook,
/// and optionally an additional post-hook event after successful completion.
pub fn with_write_hooks<F>(
    context: HookContext,
    operation: F,
    post_hook_event: Option<HookEvent>,
) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    run_pre_hooks(HookEvent::PreWrite, &context)?;
    operation()?;
    run_post_hooks(HookEvent::PostWrite, &context);
    if let Some(event) = post_hook_event {
        run_post_hooks(event, &context);
    }
    Ok(())
}
