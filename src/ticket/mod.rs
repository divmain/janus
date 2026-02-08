mod builder;
mod content;
mod locator;
mod manipulator;
mod parser;
mod repository;

pub use crate::types::ArrayField;
pub use builder::TicketBuilder;
pub use content::{extract_body, parse as parse_ticket, remove_field, update_field, update_title};

pub use repository::{
    TicketLoadResult, build_ticket_map, find_tickets, get_all_children_counts, get_all_tickets,
    get_all_tickets_from_disk, get_all_tickets_with_map, get_children_count, get_file_mtime,
};

use std::collections::{HashMap, HashSet};

use crate::entity::Entity;
use crate::error::{FileOperation, JanusError, Result, format_file_error};
use crate::hooks::{
    HookContext, HookEvent, run_post_hooks, run_post_hooks_async, run_pre_hooks,
    run_pre_hooks_async,
};
use crate::parser::parse_document;
use crate::ticket::content::validate_field_name;
use crate::ticket::locator::TicketLocator;
use crate::ticket::manipulator::{
    remove_field as remove_field_from_content, update_field as update_field_in_content,
};
use crate::ticket::parser::parse;
use crate::types::EntityType;
use crate::types::TicketMetadata;
use crate::utils::extract_id_from_path;
use serde_json;
use std::path::PathBuf;
use tokio::fs as tokio_fs;

/// A ticket represents a task, bug, feature, or chore stored as a markdown file.
///
/// This struct provides direct file I/O operations for reading and writing ticket files,
/// with built-in support for hooks and field manipulation.
#[derive(Debug, Clone)]
pub struct Ticket {
    pub file_path: PathBuf,
    pub id: String,
}

impl Ticket {
    /// Find a ticket by its partial ID.
    ///
    /// Searches for a ticket matching the given partial ID and returns a Ticket
    /// if found uniquely.
    pub async fn find(partial_id: &str) -> Result<Self> {
        let locator = TicketLocator::find(partial_id).await?;
        Ok(Ticket {
            file_path: locator.file_path,
            id: locator.id,
        })
    }

    /// Find a ticket by partial ID and read its metadata in one operation
    pub async fn find_and_read(partial_id: &str) -> Result<(Self, TicketMetadata)> {
        let ticket = Self::find(partial_id).await?;
        let metadata = ticket.read()?;
        Ok((ticket, metadata))
    }

    /// Create a Ticket from an existing file path.
    ///
    /// Extracts the ticket ID from the file path's stem.
    pub fn new(file_path: PathBuf) -> Result<Self> {
        let id = extract_id_from_path(&file_path, "ticket")?;
        Ok(Ticket { file_path, id })
    }

    /// Read and parse the ticket's metadata.
    pub fn read(&self) -> Result<TicketMetadata> {
        let raw_content = self.read_content()?;
        let mut metadata = parse(&raw_content)?;
        metadata.file_path = Some(self.file_path.clone());
        Ok(metadata)
    }

    /// Read the raw content of the ticket file (async).
    pub async fn read_content_async(&self) -> Result<String> {
        tokio_fs::read_to_string(&self.file_path)
            .await
            .map_err(|e| format_file_error(&self.file_path, FileOperation::Read, "ticket", e))
    }

    /// Read the raw content of the ticket file (blocking - for sync contexts).
    pub fn read_content(&self) -> Result<String> {
        std::fs::read_to_string(&self.file_path)
            .map_err(|e| format_file_error(&self.file_path, FileOperation::Read, "ticket", e))
    }

    /// Write content to the ticket file with hooks.
    pub fn write(&self, content: &str) -> Result<()> {
        crate::fs::with_write_hooks(
            self.hook_context(),
            || self.write_raw(content),
            Some(HookEvent::TicketUpdated),
        )
    }

    /// Write raw content without hooks (blocking - for sync contexts).
    fn write_raw(&self, content: &str) -> Result<()> {
        self.ensure_parent_dir()?;
        std::fs::write(&self.file_path, content)
            .map_err(|e| format_file_error(&self.file_path, FileOperation::Write, "ticket", e))
    }

    /// Ensure the parent directory exists (blocking - for sync contexts).
    fn ensure_parent_dir(&self) -> Result<()> {
        crate::utils::ensure_parent_dir(&self.file_path)
    }

    /// Update a field in the ticket's frontmatter.
    pub fn update_field(&self, field: &str, value: &str) -> Result<()> {
        validate_field_name(field, "update")?;

        let raw_content = self.read_content()?;

        let context = self
            .hook_context()
            .with_field_name(field)
            .with_new_value(value);

        crate::fs::with_write_hooks(
            context,
            || {
                let new_content = update_field_in_content(&raw_content, field, value)?;
                self.write_raw(&new_content)
            },
            Some(HookEvent::TicketUpdated),
        )
    }

    /// Remove a field from the ticket's frontmatter.
    pub fn remove_field(&self, field: &str) -> Result<()> {
        validate_field_name(field, "remove")?;

        let raw_content = self.read_content()?;

        let context = self.hook_context().with_field_name(field);

        crate::fs::with_write_hooks(
            context,
            || {
                let new_content = remove_field_from_content(&raw_content, field)?;
                self.write_raw(&new_content)
            },
            Some(HookEvent::TicketUpdated),
        )
    }

    /// Add a value to an array field (deps or links).
    pub fn add_to_array_field(&self, field: &str, value: &str) -> Result<bool> {
        self.mutate_array_field(
            field,
            value,
            |current| !current.contains(&value.to_string()),
            |current| {
                let mut new_array = current.clone();
                new_array.push(value.to_string());
                new_array
            },
        )
    }

    /// Remove a value from an array field (deps or links).
    pub fn remove_from_array_field(&self, field: &str, value: &str) -> Result<bool> {
        self.mutate_array_field(
            field,
            value,
            |current| current.contains(&value.to_string()),
            |current| {
                current
                    .iter()
                    .filter(|v| v.as_str() != value)
                    .cloned()
                    .collect()
            },
        )
    }

    /// Check if a value exists in an array field (deps or links).
    pub fn has_in_array_field(&self, field: &str, value: &str) -> Result<bool> {
        let field_enum: ArrayField = field.parse()?;
        let raw_content = self.read_content()?;
        let metadata = parse(&raw_content)?;
        let current_array = Self::get_array_field(&metadata, field_enum)?;
        Ok(current_array.contains(&value.to_string()))
    }

    /// Generic helper for mutating array fields (deps, links).
    fn mutate_array_field<F>(
        &self,
        field: &str,
        _value: &str,
        should_mutate: impl Fn(&Vec<String>) -> bool,
        mutate: F,
    ) -> Result<bool>
    where
        F: FnOnce(&Vec<String>) -> Vec<String>,
    {
        let field_enum: ArrayField = field.parse()?;
        let raw_content = self.read_content()?;
        let metadata = parse(&raw_content)?;
        let current_array = Self::get_array_field(&metadata, field_enum)?;

        if !should_mutate(current_array) {
            return Ok(false);
        }

        let new_array = mutate(current_array);
        let json_value = if new_array.is_empty() {
            "[]".to_string()
        } else {
            serde_json::to_string(&new_array)?
        };

        let context = self
            .hook_context()
            .with_field_name(field)
            .with_new_value(&json_value);

        crate::fs::with_write_hooks(
            context,
            || {
                let new_content = update_field_in_content(&raw_content, field, &json_value)?;
                self.write_raw(&new_content)
            },
            Some(HookEvent::TicketUpdated),
        )?;

        Ok(true)
    }

    fn get_array_field(metadata: &TicketMetadata, field: ArrayField) -> Result<&Vec<String>> {
        match field {
            ArrayField::Deps => Ok(&metadata.deps),
            ArrayField::Links => Ok(&metadata.links),
        }
    }

    /// Write a completion summary section to the ticket file.
    ///
    /// If a "## Completion Summary" section already exists, it will be updated.
    /// Otherwise, a new section will be appended to the end of the file.
    pub fn write_completion_summary(&self, summary: &str) -> Result<()> {
        let content = self.read_content()?;

        let doc = parse_document(&content).map_err(|e| {
            JanusError::InvalidFormat(format!(
                "Failed to parse ticket {} at {}: {}",
                self.id,
                crate::utils::format_relative_path(&self.file_path),
                e
            ))
        })?;
        let updated_body = doc.update_section("Completion Summary", summary);

        let new_content = format!("---\n{}\n---\n{}", doc.frontmatter_raw, updated_body);

        self.write(&new_content)
    }

    /// Build a hook context for this ticket.
    pub fn hook_context(&self) -> HookContext {
        HookContext::new()
            .with_item_type(EntityType::Ticket)
            .with_item_id(&self.id)
            .with_file_path(&self.file_path)
    }

    /// Check if the ticket file exists.
    pub fn exists(&self) -> bool {
        self.file_path.exists()
    }

    /// Delete the ticket file (async).
    pub async fn delete_async(&self) -> Result<()> {
        if !self.file_path.exists() {
            return Ok(());
        }

        let context = self.hook_context();

        run_pre_hooks_async(HookEvent::PreDelete, &context).await?;

        tokio_fs::remove_file(&self.file_path)
            .await
            .map_err(|e| format_file_error(&self.file_path, FileOperation::Delete, "ticket", e))?;

        run_post_hooks_async(HookEvent::PostDelete, &context).await;

        Ok(())
    }
}

impl Entity for Ticket {
    type Metadata = TicketMetadata;

    async fn find(partial_id: &str) -> Result<Self> {
        Ticket::find(partial_id).await
    }

    fn read(&self) -> Result<TicketMetadata> {
        self.read()
    }

    fn write(&self, content: &str) -> Result<()> {
        self.write(content)
    }

    fn delete(&self) -> Result<()> {
        if !self.file_path.exists() {
            return Ok(());
        }

        let context = self.hook_context();

        run_pre_hooks(HookEvent::PreDelete, &context)?;

        std::fs::remove_file(&self.file_path)
            .map_err(|e| format_file_error(&self.file_path, FileOperation::Delete, "ticket", e))?;

        run_post_hooks(HookEvent::PostDelete, &context);

        Ok(())
    }

    fn exists(&self) -> bool {
        self.exists()
    }
}

/// Resolve a partial ID to a full ID using an in-memory HashMap
///
/// This function works on an in-memory HashMap (useful for pre-loaded data),
/// while `find_by_partial_id()` in finder.rs works on the filesystem/store.
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
pub fn resolve_id_from_map<T>(
    partial_id: &str,
    map: &std::collections::HashMap<String, T>,
) -> Result<String> {
    if map.is_empty() {
        return Err(JanusError::EmptyTicketMap);
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

/// Check if adding a dependency would create a circular dependency.
///
/// This function performs both direct and transitive circular dependency detection:
/// - Direct: A->B when B already depends on A
/// - Transitive: A->B->C->A (multi-level cycles)
///
/// Returns an error describing the cycle if one is detected.
pub fn check_circular_dependency(
    from_id: &str,
    to_id: &str,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> Result<()> {
    // Direct circular dependency: A->B when B already depends on A
    if let Some(dep_ticket) = ticket_map.get(to_id)
        && dep_ticket.deps.contains(&from_id.to_string())
    {
        return Err(JanusError::CircularDependency(format!(
            "{from_id} -> {to_id} (direct: {to_id} already depends on {from_id})"
        )));
    }

    // Transitive circular dependency: A->B->...->A
    // Use DFS to detect if we can reach from_id starting from to_id
    fn has_path_to(
        current: &str,
        target: &str,
        ticket_map: &HashMap<String, TicketMetadata>,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        if current == target {
            path.push(current.to_string());
            return Some(path.clone());
        }

        if visited.contains(current) {
            return None;
        }

        visited.insert(current.to_string());
        path.push(current.to_string());

        if let Some(ticket) = ticket_map.get(current) {
            for dep in &ticket.deps {
                if let Some(found_path) = has_path_to(dep, target, ticket_map, visited, path) {
                    return Some(found_path);
                }
            }
        }

        path.pop();
        None
    }

    let mut visited = HashSet::new();
    let mut path = Vec::new();

    if let Some(cycle_path) = has_path_to(to_id, from_id, ticket_map, &mut visited, &mut path) {
        // Format the cycle path for the error message
        let cycle_str = cycle_path.join(" -> ");
        return Err(JanusError::CircularDependency(format!(
            "{from_id} -> {to_id} would create cycle: {cycle_str}"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_exact_match() {
        let mut map: std::collections::HashMap<String, ()> = std::collections::HashMap::new();
        map.insert("j-a1b2".to_string(), ());

        let result = resolve_id_from_map("j-a1b2", &map).unwrap();
        assert_eq!(result, "j-a1b2");
    }

    #[test]
    fn test_resolve_partial_match_single() {
        let mut map: std::collections::HashMap<String, ()> = std::collections::HashMap::new();
        map.insert("j-a1b2".to_string(), ());
        map.insert("k-c3d4".to_string(), ());

        let result = resolve_id_from_map("j-a1", &map).unwrap();
        assert_eq!(result, "j-a1b2");
    }

    #[test]
    fn test_resolve_partial_match_multiple() {
        let mut map: std::collections::HashMap<String, ()> = std::collections::HashMap::new();
        map.insert("j-a1b2".to_string(), ());
        map.insert("j-a1c3".to_string(), ());

        let result = resolve_id_from_map("j-a1", &map);
        assert!(matches!(result, Err(JanusError::AmbiguousId(_, _))));
    }

    #[test]
    fn test_resolve_no_match() {
        let map: std::collections::HashMap<String, ()> = std::collections::HashMap::new();

        let result = resolve_id_from_map("x-y-z", &map);
        assert!(matches!(result, Err(JanusError::EmptyTicketMap)));
    }

    #[test]
    fn test_resolve_empty_map() {
        let map: std::collections::HashMap<String, ()> = std::collections::HashMap::new();

        let result = resolve_id_from_map("j-a1b2", &map);
        assert!(matches!(result, Err(JanusError::EmptyTicketMap)));
    }
}
