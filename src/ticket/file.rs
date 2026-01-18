use crate::error::Result;
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

    pub fn from_path(file_path: PathBuf) -> Self {
        TicketFile {
            locator: TicketLocator::new(file_path),
        }
    }

    pub fn read_raw(&self) -> Result<String> {
        Ok(fs::read_to_string(&self.locator.file_path)?)
    }

    pub fn write_raw(&self, content: &str) -> Result<()> {
        if let Some(parent) = self.locator.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.locator.file_path, content)?;
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
}
