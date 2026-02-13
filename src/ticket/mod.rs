mod builder;
mod locator;
mod manipulator;
mod parser;
mod repository;
mod validate;

pub use crate::types::ArrayField;
pub use crate::types::validate_field_name;
pub use builder::TicketBuilder;
pub use manipulator::{extract_body, remove_field, update_field, update_title};
pub use parser::parse as parse_ticket;

pub use repository::{
    TicketLoadResult, build_ticket_map, find_tickets, get_all_children_counts, get_all_tickets,
    get_all_tickets_from_disk, get_all_tickets_with_map, get_children_count,
};

pub use self::validate::enforce_filename_authority;

use crate::entity::Entity;
use crate::error::{JanusError, Result};
use crate::hooks::{
    HookContext, HookEvent, run_post_hooks, run_post_hooks_async, run_pre_hooks,
    run_pre_hooks_async,
};
use crate::parser::{
    extract_section_from_body, parse_document_raw, remove_section_from_body, split_frontmatter,
    update_section_in_body,
};
use crate::ticket::locator::TicketLocator;
use crate::ticket::manipulator::{
    remove_field as remove_field_from_content, update_field as update_field_in_content,
};
use crate::ticket::parser::parse;
use crate::types::EntityType;
use crate::types::TicketMetadata;
use crate::utils::extract_id_from_path;
use serde_json;
use serde_yaml_ng;
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
        let ticket_clone = ticket.clone();
        let metadata = tokio::task::spawn_blocking(move || ticket_clone.read())
            .await
            .map_err(|e| JanusError::BlockingTaskFailed(e.to_string()))??;
        Ok((ticket, metadata))
    }

    /// Resolve a partial ticket ID to a full ID.
    ///
    /// This is a convenience method that finds a ticket by partial ID and returns
    /// just the full ID. Errors (including `TicketNotFound`) are propagated.
    pub async fn resolve_partial_id(partial_id: &str) -> Result<String> {
        let ticket = Self::find(partial_id).await?;
        Ok(ticket.id)
    }

    /// Create a Ticket from an existing file path.
    ///
    /// Extracts the ticket ID from the file path's stem.
    pub fn new(file_path: PathBuf) -> Result<Self> {
        let id = extract_id_from_path(&file_path, "ticket")?;
        Ok(Ticket { file_path, id })
    }

    /// Read and parse the ticket's metadata.
    ///
    /// Enforces the filename-stem-is-authoritative policy: if the frontmatter
    /// `id` differs from the filename stem, a warning is emitted and the
    /// filename stem is used.
    pub fn read(&self) -> Result<TicketMetadata> {
        let raw_content = self.read_content()?;
        let mut metadata = parse(&raw_content)?;
        enforce_filename_authority(&mut metadata, &self.id);
        metadata.file_path = Some(self.file_path.clone());
        Ok(metadata)
    }

    /// Read the raw content of the ticket file (async).
    pub async fn read_content_async(&self) -> Result<String> {
        tokio_fs::read_to_string(&self.file_path)
            .await
            .map_err(|e| JanusError::StorageError {
                operation: "read",
                item_type: "ticket",
                path: self.file_path.clone(),
                source: e,
            })
    }

    /// Read the raw content of the ticket file (blocking - for sync contexts).
    pub fn read_content(&self) -> Result<String> {
        std::fs::read_to_string(&self.file_path).map_err(|e| JanusError::StorageError {
            operation: "read",
            item_type: "ticket",
            path: self.file_path.clone(),
            source: e,
        })
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
        crate::fs::write_file_atomic(&self.file_path, content)
    }

    /// Ensure the parent directory exists (blocking - for sync contexts).
    fn ensure_parent_dir(&self) -> Result<()> {
        crate::fs::ensure_parent_dir(&self.file_path)
    }

    /// Update a field in the ticket's frontmatter.
    ///
    /// Concurrent read-modify-write cycles follow last-writer-wins semantics.
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
    ///
    /// Concurrent read-modify-write cycles follow last-writer-wins semantics.
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
    pub fn add_to_array_field(&self, field: ArrayField, value: &str) -> Result<bool> {
        let field_str = field.as_str();
        self.mutate_array_field(
            field_str,
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
    pub fn remove_from_array_field(&self, field: ArrayField, value: &str) -> Result<bool> {
        let field_str = field.as_str();
        self.mutate_array_field(
            field_str,
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

    /// Extract an array field from raw content with fallback to tolerant parsing.
    ///
    /// Attempts strict YAML parsing first. If that fails (e.g., due to unknown fields),
    /// prints a warning and falls back to tolerant extraction that only parses
    /// the requested field.
    fn extract_array_field_with_fallback(
        &self,
        raw_content: &str,
        field: &str,
        operation: &str,
    ) -> Result<Vec<String>> {
        // Try strict parse first, fall back to tolerant path
        match parse(raw_content) {
            Ok(metadata) => {
                let field_enum: ArrayField = field.parse()?;
                Ok(Self::get_array_field(&metadata, field_enum)?.clone())
            }
            Err(e) => {
                eprintln!(
                    "Warning: ticket '{}' has validation issues ({}); using tolerant {} for field '{}'",
                    self.id, e, operation, field
                );
                Self::extract_array_field_tolerant(raw_content, field)
            }
        }
    }

    /// Check if a value exists in an array field (deps or links).
    pub fn has_in_array_field(&self, field: ArrayField, value: &str) -> Result<bool> {
        let raw_content = self.read_content()?;
        let current_array =
            self.extract_array_field_with_fallback(&raw_content, field.as_str(), "read")?;
        Ok(current_array.contains(&value.to_string()))
    }

    /// Generic helper for mutating array fields (deps, links).
    ///
    /// Concurrent read-modify-write cycles follow last-writer-wins semantics.
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
        let raw_content = self.read_content()?;
        let current_array = self.extract_array_field_with_fallback(&raw_content, field, "edit")?;

        if !should_mutate(&current_array) {
            return Ok(false);
        }

        let new_array = mutate(&current_array);
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

    /// Tolerant extraction of an array field from raw ticket content.
    ///
    /// When the strict ticket parser fails (e.g., due to unknown fields, missing
    /// required fields, or invalid values in other fields), this function falls back
    /// to splitting the file into frontmatter and body, parsing only the YAML as a
    /// generic mapping, and extracting the targeted array field.
    ///
    /// This allows array field operations (add/remove deps, links) to succeed even
    /// when the ticket file has validation issues in unrelated fields.
    fn extract_array_field_tolerant(raw_content: &str, field: &str) -> Result<Vec<String>> {
        let (frontmatter_str, _body) = split_frontmatter(raw_content)?;
        let mapping: serde_yaml_ng::Mapping =
            serde_yaml_ng::from_str(&frontmatter_str).map_err(|e| {
                JanusError::InvalidFormat(format!(
                    "Failed to parse frontmatter YAML in tolerant mode: {e}"
                ))
            })?;

        let key = serde_yaml_ng::Value::String(field.to_string());
        match mapping.get(&key) {
            Some(serde_yaml_ng::Value::Sequence(seq)) => {
                let mut result = Vec::new();
                for item in seq {
                    match item {
                        serde_yaml_ng::Value::String(s) => result.push(s.clone()),
                        other => result.push(format!("{other:?}")),
                    }
                }
                Ok(result)
            }
            Some(serde_yaml_ng::Value::Null) | None => Ok(Vec::new()),
            Some(other) => Err(JanusError::InvalidFormat(format!(
                "field '{field}' is not an array, found: {other:?}"
            ))),
        }
    }

    /// Add a timestamped note to the ticket.
    ///
    /// Adds the note text under a "## Notes" section. If the section doesn't exist,
    /// it will be created. The note is prefixed with a timestamp.
    ///
    /// Concurrent read-modify-write cycles follow last-writer-wins semantics.
    ///
    /// # Errors
    ///
    /// Returns `JanusError::EmptyNote` if the note text is empty or only whitespace.
    pub fn add_note(&self, note_text: &str) -> Result<()> {
        // Validate that note is not empty or only whitespace
        if note_text.trim().is_empty() {
            return Err(JanusError::EmptyNote);
        }

        let timestamp = crate::utils::iso_date();

        let content = self.read_content()?;
        let mut new_content = content;
        if !new_content.contains("## Notes") {
            new_content.push_str("\n## Notes");
        }
        new_content.push_str(&format!("\n\n**{timestamp}**\n\n{note_text}"));
        self.write(&new_content)?;

        crate::events::log_note_added(&self.id, note_text);

        Ok(())
    }

    /// Write a completion summary section to the ticket file.
    ///
    /// If a "## Completion Summary" section already exists, it will be updated.
    /// Otherwise, a new section will be appended to the end of the file.
    pub fn write_completion_summary(&self, summary: &str) -> Result<()> {
        self.update_section("Completion Summary", Some(summary))
    }

    /// Extract current value of a body section from ticket content.
    ///
    /// Returns `Ok(Some(content))` if the section exists,
    /// `Ok(None)` if it doesn't exist,
    /// or an error if parsing fails.
    pub fn extract_section(&self, section_name: &str) -> Result<Option<String>> {
        let content = self.read_content()?;
        let (_frontmatter_raw, body) = parse_document_raw(&content).map_err(|e| {
            JanusError::InvalidFormat(format!("Failed to parse ticket {}: {}", self.id, e))
        })?;
        extract_section_from_body(&body, section_name)
    }

    /// Extract the description (content between title and first H2).
    ///
    /// Returns `Ok(Some(desc))` if there is description content,
    /// `Ok(None)` if the description is empty,
    /// or an error if parsing fails.
    pub fn extract_description(&self) -> Result<Option<String>> {
        let content = self.read_content()?;
        let (_frontmatter_raw, body) = parse_document_raw(&content).map_err(|e| {
            JanusError::InvalidFormat(format!("Failed to parse ticket {}: {}", self.id, e))
        })?;

        // Get content after the title (first line)
        let after_title = body
            .split_once('\n')
            .map(|(_, rest)| rest.trim_start())
            .unwrap_or("");

        // Find first H2 or use all remaining content
        let desc = after_title
            .split_once("\n## ")
            .map(|(before, _)| before.trim())
            .unwrap_or_else(|| after_title.trim());

        if desc.is_empty() {
            Ok(None)
        } else {
            Ok(Some(desc.to_string()))
        }
    }

    /// Update a body section in the ticket.
    ///
    /// If `content` is `Some(value)`, the section will be created or updated.
    /// If `content` is `None`, the section will be removed if it exists.
    pub fn update_section(&self, section_name: &str, content: Option<&str>) -> Result<()> {
        let raw_content = self.read_content()?;
        let (frontmatter_raw, body) = parse_document_raw(&raw_content).map_err(|e| {
            JanusError::InvalidFormat(format!(
                "Failed to parse ticket {} at {}: {}",
                self.id,
                crate::utils::format_relative_path(&self.file_path),
                e
            ))
        })?;

        let updated_body = if let Some(new_content) = content {
            update_section_in_body(&body, section_name, new_content)?
        } else {
            // Remove the section if content is None
            remove_section_from_body(&body, section_name)
        };

        let new_content = format!("---\n{frontmatter_raw}\n---\n{updated_body}");
        self.write(&new_content)
    }

    /// Update the description (content between title and first H2).
    ///
    /// If `description` is `Some(value)`, the description will be created or updated.
    /// If `description` is `None`, the description will be removed.
    pub fn update_description(&self, description: Option<&str>) -> Result<()> {
        let raw_content = self.read_content()?;
        let (frontmatter_raw, body) = parse_document_raw(&raw_content).map_err(|e| {
            JanusError::InvalidFormat(format!(
                "Failed to parse ticket {} at {}: {}",
                self.id,
                crate::utils::format_relative_path(&self.file_path),
                e
            ))
        })?;

        // Get body without title
        let title_end = body.find('\n').unwrap_or(body.len());
        let title = &body[..title_end];
        let after_title = &body[title_end..];

        // Find first H2 or end of document
        let h2_pos = after_title.find("\n## ");

        let new_body = if let Some(pos) = h2_pos {
            let from_h2 = &after_title[pos..];
            if let Some(desc) = description {
                format!("{title}\n\n{desc}{from_h2}")
            } else {
                format!("{title}{from_h2}")
            }
        } else {
            // No H2 sections
            if let Some(desc) = description {
                format!("{title}\n\n{desc}")
            } else {
                title.to_string()
            }
        };

        let new_content = format!("---\n{frontmatter_raw}\n---\n{new_body}");
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
            .map_err(|e| JanusError::StorageError {
                operation: "delete",
                item_type: "ticket",
                path: self.file_path.clone(),
                source: e,
            })?;

        run_post_hooks_async(HookEvent::PostDelete, &context).await;

        Ok(())
    }

    /// Find a ticket and update a field in one operation.
    ///
    /// This is a convenience method for the common pattern of finding a ticket
    /// by ID and immediately updating one of its fields.
    ///
    /// # Arguments
    /// * `partial_id` - The partial ticket ID to find
    /// * `field` - The field name to update
    /// * `value` - The new value for the field
    ///
    /// # Returns
    /// `Ok(())` if the operation succeeded
    pub async fn find_and_update_field(partial_id: &str, field: &str, value: &str) -> Result<()> {
        let ticket = Self::find(partial_id).await?;
        ticket.update_field(field, value)?;
        Ok(())
    }

    /// Find a ticket and apply a modification function to its content.
    ///
    /// This is a more flexible version that allows arbitrary modifications
    /// to the ticket content while handling the find/modify/write cycle.
    ///
    /// # Arguments
    /// * `partial_id` - The partial ticket ID to find
    /// * `modify` - A function that takes the current content and returns the modified content
    ///
    /// # Returns
    /// `Ok(())` if the operation succeeded
    pub async fn find_and_modify<F>(partial_id: &str, modify: F) -> Result<()>
    where
        F: FnOnce(String) -> Result<String>,
    {
        let ticket = Self::find(partial_id).await?;
        let content = ticket.read_content()?;
        let new_content = modify(content)?;
        ticket.write(&new_content)?;
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

        std::fs::remove_file(&self.file_path).map_err(|e| JanusError::StorageError {
            operation: "delete",
            item_type: "ticket",
            path: self.file_path.clone(),
            source: e,
        })?;

        run_post_hooks(HookEvent::PostDelete, &context);

        Ok(())
    }

    fn exists(&self) -> bool {
        self.exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Tolerant Edit Path Tests ====================

    #[test]
    fn test_tolerant_extract_array_field_with_unknown_fields() {
        // Ticket with an unknown field that would fail strict parsing (deny_unknown_fields)
        let content = r#"---
id: test-1234
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: ["dep-1", "dep-2"]
links: []
unknown_field: should_cause_strict_parse_failure
---
# Test Ticket

Description.
"#;

        // Strict parse should fail
        assert!(parse(content).is_err());

        // Tolerant extraction should succeed
        let deps = Ticket::extract_array_field_tolerant(content, "deps").unwrap();
        assert_eq!(deps, vec!["dep-1", "dep-2"]);

        let links = Ticket::extract_array_field_tolerant(content, "links").unwrap();
        assert!(links.is_empty());
    }

    #[test]
    fn test_tolerant_extract_array_field_missing_required_fields() {
        // Ticket missing required `uuid` field
        let content = r#"---
id: test-1234
status: new
deps: ["existing-dep"]
links: ["link-1"]
---
# Test Ticket
"#;

        // Strict parse should fail (missing uuid)
        assert!(parse(content).is_err());

        // Tolerant extraction should succeed
        let deps = Ticket::extract_array_field_tolerant(content, "deps").unwrap();
        assert_eq!(deps, vec!["existing-dep"]);

        let links = Ticket::extract_array_field_tolerant(content, "links").unwrap();
        assert_eq!(links, vec!["link-1"]);
    }

    #[test]
    fn test_tolerant_extract_array_field_invalid_enum_value() {
        // Ticket with an invalid status value
        let content = r#"---
id: test-1234
uuid: 550e8400-e29b-41d4-a716-446655440000
status: invalid_status_value
deps: []
links: ["link-a"]
---
# Test Ticket
"#;

        // Strict parse should fail (invalid status)
        assert!(parse(content).is_err());

        // Tolerant extraction should succeed
        let deps = Ticket::extract_array_field_tolerant(content, "deps").unwrap();
        assert!(deps.is_empty());

        let links = Ticket::extract_array_field_tolerant(content, "links").unwrap();
        assert_eq!(links, vec!["link-a"]);
    }

    #[test]
    fn test_tolerant_extract_array_field_null_value() {
        // Field exists but is null (e.g., `deps:` with no value)
        let content = r#"---
id: test-1234
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps:
links:
---
# Test Ticket
"#;

        // This may or may not pass strict parsing depending on serde defaults,
        // but tolerant extraction should handle null gracefully
        let deps = Ticket::extract_array_field_tolerant(content, "deps").unwrap();
        assert!(deps.is_empty());

        let links = Ticket::extract_array_field_tolerant(content, "links").unwrap();
        assert!(links.is_empty());
    }

    #[test]
    fn test_tolerant_extract_array_field_missing_field() {
        // The array field itself doesn't exist in the frontmatter
        let content = r#"---
id: test-1234
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
---
# Test Ticket
"#;

        let deps = Ticket::extract_array_field_tolerant(content, "deps").unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn test_tolerant_extract_array_field_non_array_errors() {
        // Field exists but is not an array
        let content = r#"---
id: test-1234
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: "not-an-array"
---
# Test Ticket
"#;

        let result = Ticket::extract_array_field_tolerant(content, "deps");
        assert!(result.is_err());
    }

    #[test]
    fn test_tolerant_extract_preserves_body_verbatim() {
        // Ensure the tolerant path doesn't corrupt the body when used with update_field_in_content
        let content = r#"---
id: test-1234
status: new
deps: ["old-dep"]
unknown_extra: true
---
# My Special Title

Body with **markdown** and special --- chars.

## Notes

Some notes here.
"#;

        // Strict parse should fail (missing uuid, unknown field)
        assert!(parse(content).is_err());

        // But FrontmatterEditor (used by update_field_in_content) should work
        let updated = update_field_in_content(content, "deps", r#"["old-dep","new-dep"]"#).unwrap();

        // Body should be preserved verbatim
        assert!(updated.contains("# My Special Title"));
        assert!(updated.contains("Body with **markdown** and special --- chars."));
        assert!(updated.contains("## Notes"));
        assert!(updated.contains("Some notes here."));
    }

    #[test]
    fn test_tolerant_file_based_update_field() {
        // Test that update_field works on a ticket with validation issues
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test-1234.md");

        let content = r#"---
id: test-1234
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: []
links: []
unknown_field: causes_strict_failure
---
# Test Ticket

Description.
"#;
        std::fs::write(&file_path, content).unwrap();

        let ticket = Ticket {
            file_path: file_path.clone(),
            id: "test-1234".to_string(),
        };

        // Strict read should fail
        assert!(ticket.read().is_err());

        // update_field should still work (it uses FrontmatterEditor, not strict parse)
        ticket.update_field("status", "complete").unwrap();

        let updated = std::fs::read_to_string(&file_path).unwrap();
        assert!(updated.contains("status: complete"));
        assert!(updated.contains("id: test-1234"));
        assert!(updated.contains("# Test Ticket"));
    }

    #[test]
    fn test_tolerant_file_based_add_to_array_field() {
        // Test that add_to_array_field works on a ticket with validation issues
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test-1234.md");

        let content = r#"---
id: test-1234
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: ["existing-dep"]
links: []
unknown_field: causes_strict_failure
---
# Test Ticket

Description.
"#;
        std::fs::write(&file_path, content).unwrap();

        let ticket = Ticket {
            file_path: file_path.clone(),
            id: "test-1234".to_string(),
        };

        // Strict read should fail
        assert!(ticket.read().is_err());

        // add_to_array_field should succeed via tolerant path
        let added = ticket
            .add_to_array_field(ArrayField::Deps, "new-dep")
            .unwrap();
        assert!(added);

        let updated = std::fs::read_to_string(&file_path).unwrap();
        assert!(updated.contains("existing-dep"));
        assert!(updated.contains("new-dep"));
        assert!(updated.contains("# Test Ticket"));
    }

    #[test]
    fn test_tolerant_file_based_remove_from_array_field() {
        // Test that remove_from_array_field works on a ticket with validation issues
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test-1234.md");

        let content = r#"---
id: test-1234
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: ["dep-to-remove", "dep-to-keep"]
links: []
unknown_field: causes_strict_failure
---
# Test Ticket

Description.
"#;
        std::fs::write(&file_path, content).unwrap();

        let ticket = Ticket {
            file_path: file_path.clone(),
            id: "test-1234".to_string(),
        };

        // Strict read should fail
        assert!(ticket.read().is_err());

        // remove_from_array_field should succeed via tolerant path
        let removed = ticket
            .remove_from_array_field(ArrayField::Deps, "dep-to-remove")
            .unwrap();
        assert!(removed);

        let updated = std::fs::read_to_string(&file_path).unwrap();
        assert!(!updated.contains("dep-to-remove"));
        assert!(updated.contains("dep-to-keep"));
    }

    #[test]
    fn test_tolerant_file_based_has_in_array_field() {
        // Test that has_in_array_field works on a ticket with validation issues
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test-1234.md");

        let content = r#"---
id: test-1234
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: ["existing-dep"]
links: []
unknown_field: causes_strict_failure
---
# Test Ticket
"#;
        std::fs::write(&file_path, content).unwrap();

        let ticket = Ticket {
            file_path: file_path.clone(),
            id: "test-1234".to_string(),
        };

        // Strict read should fail
        assert!(ticket.read().is_err());

        // has_in_array_field should succeed via tolerant path
        assert!(
            ticket
                .has_in_array_field(ArrayField::Deps, "existing-dep")
                .unwrap()
        );
        assert!(
            !ticket
                .has_in_array_field(ArrayField::Deps, "nonexistent")
                .unwrap()
        );
    }

    #[test]
    fn test_tolerant_add_does_not_duplicate() {
        // When value already exists, add should return false even in tolerant mode
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test-1234.md");

        let content = r#"---
id: test-1234
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: ["already-there"]
links: []
unknown_field: causes_strict_failure
---
# Test Ticket
"#;
        std::fs::write(&file_path, content).unwrap();

        let ticket = Ticket {
            file_path: file_path.clone(),
            id: "test-1234".to_string(),
        };

        let added = ticket
            .add_to_array_field(ArrayField::Deps, "already-there")
            .unwrap();
        assert!(!added);
    }

    #[test]
    fn test_tolerant_remove_nonexistent_returns_false() {
        // When value doesn't exist, remove should return false even in tolerant mode
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test-1234.md");

        let content = r#"---
id: test-1234
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: ["some-dep"]
links: []
unknown_field: causes_strict_failure
---
# Test Ticket
"#;
        std::fs::write(&file_path, content).unwrap();

        let ticket = Ticket {
            file_path: file_path.clone(),
            id: "test-1234".to_string(),
        };

        let removed = ticket
            .remove_from_array_field(ArrayField::Deps, "nonexistent")
            .unwrap();
        assert!(!removed);
    }

    #[test]
    fn test_strict_path_still_used_when_valid() {
        // Ensure that when strict parsing succeeds, we use the strict path (no warning)
        let content = r#"---
id: test-1234
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: ["dep-1"]
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Valid Ticket

Description.
"#;

        // Strict parse should succeed
        let metadata = parse(content).unwrap();
        assert_eq!(metadata.deps, vec!["dep-1"]);

        // Tolerant extraction should also work (same result)
        let deps = Ticket::extract_array_field_tolerant(content, "deps").unwrap();
        assert_eq!(deps, vec!["dep-1"]);
    }
}
