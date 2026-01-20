use crate::error::{JanusError, Result};
use crate::hooks::{HookContext, ItemType};
use crate::ticket::locator::TicketLocator;
use std::fs;
use std::path::PathBuf;

#[derive(Clone)]
pub struct TicketFile {
    locator: TicketLocator,
}

impl TicketFile {
    pub fn new(locator: TicketLocator) -> Self {
        TicketFile { locator }
    }

    pub fn from_path(file_path: PathBuf) -> Result<Self> {
        Ok(TicketFile {
            locator: TicketLocator::new(file_path)?,
        })
    }

    pub fn read_raw(&self) -> Result<String> {
        fs::read_to_string(&self.locator.file_path).map_err(|e| {
            JanusError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to read ticket at {}: {}",
                    self.locator.file_path.display(),
                    e
                ),
            ))
        })
    }

    pub fn write_raw(&self, content: &str) -> Result<()> {
        if let Some(parent) = self.locator.file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                JanusError::Io(std::io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to create directory for ticket at {}: {}",
                        parent.display(),
                        e
                    ),
                ))
            })?;
        }
        fs::write(&self.locator.file_path, content).map_err(|e| {
            JanusError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to write ticket at {}: {}",
                    self.locator.file_path.display(),
                    e
                ),
            ))
        })?;
        Ok(())
    }

    pub fn file_path(&self) -> &PathBuf {
        &self.locator.file_path
    }

    pub fn id(&self) -> &str {
        &self.locator.id
    }

    pub fn locator(&self) -> &TicketLocator {
        &self.locator
    }

    /// Build a hook context for this ticket file.
    ///
    /// This is a convenience method to avoid repeating the same hook context
    /// construction pattern throughout the codebase.
    pub fn hook_context(&self) -> HookContext {
        HookContext::new()
            .with_item_type(ItemType::Ticket)
            .with_item_id(&self.locator.id)
            .with_file_path(&self.locator.file_path)
    }
}
