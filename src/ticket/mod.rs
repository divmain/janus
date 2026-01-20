mod builder;
mod content;
mod editor;
mod file;
mod locator;
mod repository;

pub use builder::TicketBuilder;
pub use content::{extract_body, parse as parse_ticket, remove_field, update_field, update_title};
pub use editor::TicketEditor;
pub use file::TicketFile;
pub use locator::{TicketLocator, find_ticket_by_id};
pub use repository::{
    TicketRepository, build_ticket_map, find_tickets, get_all_tickets, get_all_tickets_from_disk,
    get_all_tickets_with_map, get_children_count, get_file_mtime,
};

use crate::error::Result;
use crate::types::TicketMetadata;
use std::path::PathBuf;

pub struct Ticket {
    pub file_path: PathBuf,
    pub id: String,
    file: TicketFile,
    editor: TicketEditor,
}

impl Ticket {
    pub async fn find(partial_id: &str) -> Result<Self> {
        let locator = TicketLocator::find(partial_id).await?;
        let file = TicketFile::new(locator.clone());
        let editor = TicketEditor::new(file.clone());
        Ok(Ticket {
            file_path: locator.file_path.clone(),
            id: locator.id.clone(),
            file,
            editor,
        })
    }

    pub fn new(file_path: PathBuf) -> Result<Self> {
        let locator = TicketLocator::new(file_path.clone())?;
        let file = TicketFile::new(locator.clone());
        let editor = TicketEditor::new(file.clone());
        Ok(Ticket {
            file_path: locator.file_path.clone(),
            id: locator.id.clone(),
            file,
            editor,
        })
    }

    pub fn read(&self) -> Result<TicketMetadata> {
        let raw_content = self.file.read_raw()?;
        let mut metadata = content::parse(&raw_content)?;
        metadata.file_path = Some(self.file.file_path().clone());
        Ok(metadata)
    }

    pub fn read_content(&self) -> Result<String> {
        self.file.read_raw()
    }

    pub fn write(&self, content: &str) -> Result<()> {
        self.editor.write(content)
    }

    pub fn update_field(&self, field: &str, value: &str) -> Result<()> {
        self.editor.update_field(field, value)
    }

    pub fn remove_field(&self, field: &str) -> Result<()> {
        self.editor.remove_field(field)
    }

    pub fn add_to_array_field(&self, field: &str, value: &str) -> Result<bool> {
        self.editor.add_to_array_field(field, value)
    }

    pub fn remove_from_array_field(&self, field: &str, value: &str) -> Result<bool> {
        self.editor.remove_from_array_field(field, value)
    }
}
