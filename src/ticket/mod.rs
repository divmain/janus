mod builder;
mod content;
mod editor;
mod file;
mod locator;
mod manipulator;
mod parser;
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

use crate::entity::Entity;
use crate::error::{JanusError, Result};
use crate::hooks::HookContext;
use crate::storage::{FileStorage, StorageHandle};
use crate::ticket::parser::parse;
use crate::types::EntityType;
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
        let mut metadata = parse(&raw_content)?;
        metadata.file_path = Some(self.file.file_path().to_path_buf());
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

    /// Write a completion summary section to the ticket file
    ///
    /// If a "## Completion Summary" section already exists, it will be updated.
    /// Otherwise, a new section will be appended to the end of the file.
    pub fn write_completion_summary(&self, summary: &str) -> Result<()> {
        self.editor.write_completion_summary(summary)
    }

    /// Build a hook context for this ticket.
    ///
    /// This is a convenience method to avoid repeating the same hook context
    /// construction pattern throughout the codebase.
    pub fn hook_context(&self) -> HookContext {
        HookContext::new()
            .with_item_type(EntityType::Ticket)
            .with_item_id(&self.id)
            .with_file_path(&self.file_path)
    }

    /// Check if the ticket file exists
    pub fn exists(&self) -> bool {
        self.file_path.exists()
    }
}

impl Entity for Ticket {
    type Metadata = TicketMetadata;

    async fn find(partial_id: &str) -> Result<Self> {
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

    fn read(&self) -> Result<TicketMetadata> {
        let raw_content = self.file.read_raw()?;
        let mut metadata = parse(&raw_content)?;
        metadata.file_path = Some(self.file.file_path().to_path_buf());
        Ok(metadata)
    }

    fn write(&self, content: &str) -> Result<()> {
        self.editor.write(content)
    }

    fn delete(&self) -> Result<()> {
        if !self.file_path.exists() {
            return Ok(());
        }

        // Build hook context
        let context = self.hook_context();

        // Run pre-delete hook (can abort)
        crate::hooks::run_pre_hooks(crate::hooks::HookEvent::PreDelete, &context)?;

        // Perform the delete using FileStorage trait
        self.file.delete()?;

        // Run post-delete hooks (fire-and-forget)
        crate::hooks::run_post_hooks(crate::hooks::HookEvent::PostDelete, &context);

        Ok(())
    }

    fn exists(&self) -> bool {
        self.file_path.exists()
    }
}

/// Resolve a partial ID to a full ID using a ticket map
///
/// # Arguments
///
/// * `partial_id` - The partial ID to resolve (e.g., "j-a1")
/// * `map` - A HashMap of ticket IDs to tickets
///
/// # Returns
///
/// Returns the full ID if found uniquely, otherwise an error:
/// - `Other` with "No tickets loaded" if map is empty
/// - `TicketNotFound` if no matches
/// - `AmbiguousId` if multiple matches
pub fn resolve_id_partial<T>(
    partial_id: &str,
    map: &std::collections::HashMap<String, T>,
) -> Result<String> {
    if map.is_empty() {
        return Err(JanusError::Other("No tickets loaded".to_string()));
    }

    if map.contains_key(partial_id) {
        return Ok(partial_id.to_string());
    }

    let matches: Vec<_> = map
        .keys()
        .filter(|k| k.contains(partial_id))
        .cloned()
        .collect();

    match matches.len() {
        0 => Err(JanusError::TicketNotFound(partial_id.to_string())),
        1 => Ok(matches[0].clone()),
        _ => Err(JanusError::AmbiguousId(partial_id.to_string(), matches)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_exact_match() {
        let mut map: std::collections::HashMap<String, ()> = std::collections::HashMap::new();
        map.insert("j-a1b2".to_string(), ());

        let result = resolve_id_partial("j-a1b2", &map).unwrap();
        assert_eq!(result, "j-a1b2");
    }

    #[test]
    fn test_resolve_partial_match_single() {
        let mut map: std::collections::HashMap<String, ()> = std::collections::HashMap::new();
        map.insert("j-a1b2".to_string(), ());
        map.insert("k-c3d4".to_string(), ());

        let result = resolve_id_partial("j-a1", &map).unwrap();
        assert_eq!(result, "j-a1b2");
    }

    #[test]
    fn test_resolve_partial_match_multiple() {
        let mut map: std::collections::HashMap<String, ()> = std::collections::HashMap::new();
        map.insert("j-a1b2".to_string(), ());
        map.insert("j-a1c3".to_string(), ());

        let result = resolve_id_partial("j-a1", &map);
        assert!(matches!(result, Err(JanusError::AmbiguousId(_, _))));
    }

    #[test]
    fn test_resolve_no_match() {
        let map: std::collections::HashMap<String, ()> = std::collections::HashMap::new();

        let result = resolve_id_partial("x-y-z", &map);
        assert!(matches!(result, Err(JanusError::Other(_))));
    }

    #[test]
    fn test_resolve_empty_map() {
        let map: std::collections::HashMap<String, ()> = std::collections::HashMap::new();

        let result = resolve_id_partial("j-a1b2", &map);
        assert!(matches!(result, Err(JanusError::Other(msg)) if msg.contains("No tickets loaded")));
    }
}
