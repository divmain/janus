use crate::error::Result;
use crate::storage::{FileStorage, StorageHandle};
use crate::ticket::locator::TicketLocator;
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

    pub fn locator(&self) -> &TicketLocator {
        &self.locator
    }

    pub fn file_path(&self) -> &PathBuf {
        &self.locator.file_path
    }

    pub fn id(&self) -> &str {
        &self.locator.id
    }
}

impl StorageHandle for TicketFile {
    fn file_path(&self) -> &std::path::Path {
        &self.locator.file_path
    }

    fn id(&self) -> &str {
        &self.locator.id
    }

    fn item_type(&self) -> crate::types::EntityType {
        crate::types::EntityType::Ticket
    }
}

impl FileStorage for TicketFile {}

/// Read raw content (alias for read_content from FileStorage trait)
impl TicketFile {
    pub fn read_raw(&self) -> Result<String> {
        FileStorage::read_content(self)
    }

    /// Write raw content (alias for write_raw from FileStorage trait)
    pub fn write_raw(&self, content: &str) -> Result<()> {
        FileStorage::write_raw(self, content)
    }
}
