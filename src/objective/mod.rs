//! Objective module for project objectives.
//!
//! This module provides the `Objective` type which handles file I/O for
//! objectives stored in `.janus/objectives/`. Objectives are markdown files
//! with YAML frontmatter containing metadata like id, uuid, created, and
//! satisfied-by references.
//!
//! Objective status (`Unrealized` / `Achieved`) is auto-computed at read time
//! from the `satisfied_by` references and never stored in frontmatter.

pub mod builder;
pub mod parser;
pub mod serialize;
pub mod status;
pub mod types;

pub use builder::ObjectiveBuilder;
pub use parser::parse_objective_content;
pub use serialize::serialize_objective;
pub use status::compute_objective_status;
pub use types::{ObjectiveLoadResult, ObjectiveMetadata};

use std::fs;
use std::path::PathBuf;

use crate::entity::Entity;
use crate::error::{JanusError, Result};
use crate::hooks::{HookContext, HookEvent, run_post_hooks, run_pre_hooks};
use crate::types::{EntityType, ObjectiveId, objectives_dir};
use crate::utils::{extract_id_from_path, find_markdown_files, find_markdown_files_from_path};

/// A handle for reading and writing objective files.
///
/// Objectives are stored as Markdown files with YAML frontmatter in `.janus/objectives/`.
/// This struct provides direct file I/O operations for reading and writing objective files,
/// with built-in support for hooks.
#[derive(Debug, Clone)]
pub struct Objective {
    /// Path to the objective file
    pub file_path: PathBuf,
    /// Objective ID
    pub id: String,
}

impl Objective {
    /// Find an objective by its (partial) ID.
    ///
    /// Uses filesystem search for now. Store integration will be added in a later phase.
    pub async fn find(partial_id: &str) -> Result<Self> {
        let file_path = find_objective_by_id(partial_id).await?;
        let id = extract_id_from_path(&file_path, "objective")?;
        Ok(Objective { file_path, id })
    }

    /// Create an Objective handle from an existing file path.
    pub fn new(file_path: PathBuf) -> Result<Self> {
        let id = extract_id_from_path(&file_path, "objective")?;
        Ok(Objective { file_path, id })
    }

    /// Create an Objective handle for a given ID.
    ///
    /// The ID must start with `objv-` and contain only alphanumeric characters and hyphens.
    pub fn with_id(id: &str) -> Result<Self> {
        ObjectiveId::validate(id)
            .map_err(|_| JanusError::InvalidObjectiveIdFormat(id.to_string()))?;
        let file_path = objectives_dir().join(format!("{id}.md"));
        Ok(Objective {
            file_path,
            id: id.to_string(),
        })
    }

    /// Check if the objective file exists.
    pub fn exists(&self) -> bool {
        self.file_path.exists()
    }

    /// Read and parse the objective's metadata.
    pub fn read(&self) -> Result<ObjectiveMetadata> {
        let content = self.read_content()?;
        let mut metadata = parse_objective_content(&content)?;
        metadata.file_path = Some(self.file_path.clone());
        Ok(metadata)
    }

    /// Read the raw content of the objective file.
    pub fn read_content(&self) -> Result<String> {
        fs::read_to_string(&self.file_path).map_err(|e| JanusError::StorageError {
            operation: "read",
            item_type: "objective",
            path: self.file_path.clone(),
            source: e,
        })
    }

    /// Write content to the objective file.
    ///
    /// This method triggers `PreWrite` hook before writing, and `PostWrite`
    /// + `ObjectiveUpdated` hooks after successful write.
    pub fn write(&self, content: &str) -> Result<()> {
        crate::fs::with_write_hooks(
            self.hook_context(),
            || self.write_raw(content),
            Some(HookEvent::ObjectiveUpdated),
        )
    }

    /// Write metadata to the objective file.
    pub fn write_metadata(&self, metadata: &ObjectiveMetadata) -> Result<()> {
        let content = serialize_objective(metadata)?;
        self.write(&content)
    }

    /// Write raw content without hooks.
    fn write_raw(&self, content: &str) -> Result<()> {
        self.ensure_parent_dir()?;
        crate::fs::write_file_atomic(&self.file_path, content)
    }

    /// Ensure the parent directory exists.
    fn ensure_parent_dir(&self) -> Result<()> {
        crate::fs::ensure_parent_dir(&self.file_path)
    }

    /// Delete the objective file.
    ///
    /// This method triggers `PreDelete` hook before deletion, and `PostDelete`
    /// + `ObjectiveDeleted` hooks after successful deletion.
    pub fn delete(&self) -> Result<()> {
        let context = self.hook_context();

        run_pre_hooks(HookEvent::PreDelete, &context)?;

        if let Err(e) = fs::remove_file(&self.file_path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            return Err(JanusError::StorageError {
                operation: "delete",
                item_type: "objective",
                path: self.file_path.clone(),
                source: e,
            });
        }

        run_post_hooks(HookEvent::PostDelete, &context);
        run_post_hooks(HookEvent::ObjectiveDeleted, &context);

        crate::events::log_objective_deleted(&self.id, None);

        Ok(())
    }

    /// Sanitize a criterion string for safe insertion as a markdown bullet.
    ///
    /// Removes markdown headings, collapses newlines, strips leading bullet markers,
    /// and trims excessive whitespace so the result can be inserted as a `- ` item
    /// without breaking the document structure.
    fn sanitize_criterion(input: &str) -> String {
        let mut result = String::with_capacity(input.len());
        for line in input.lines() {
            let trimmed = line.trim();
            // Strip markdown heading prefixes
            let trimmed = trimmed.trim_start_matches('#').trim_start();
            // Strip leading bullet markers (-, *, +) so we don't double-bullet
            let trimmed = if trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
                || trimmed.starts_with("+ ")
            {
                trimmed[2..].trim_start()
            } else if trimmed == "-" || trimmed == "*" || trimmed == "+" {
                ""
            } else {
                trimmed
            };
            if trimmed.is_empty() {
                continue;
            }
            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(trimmed);
        }
        result
    }

    /// Add an acceptance criterion to the objective.
    ///
    /// Appends a bullet item under the "## Acceptance Criteria" section.
    /// If the section doesn't exist, it will be created (inserted before
    /// "## Notes" if present, otherwise appended at the end).
    pub fn add_criterion(&self, criterion_text: &str) -> Result<()> {
        let sanitized = Self::sanitize_criterion(criterion_text);
        if sanitized.is_empty() {
            return Err(JanusError::ValidationEmpty("criterion text".to_string()));
        }

        let content = self.read_content()?;

        let new_content = if let Some(pos) = content.find("## Acceptance Criteria") {
            // Find the end of this section: next H2 or end of file
            let section_start = pos + "## Acceptance Criteria".len();
            let rest = &content[section_start..];
            let section_end = rest
                .find("\n## ")
                .map(|p| section_start + p)
                .unwrap_or(content.len());

            // Insert the new bullet at the end of the section
            let mut new = String::with_capacity(content.len() + sanitized.len() + 4);
            new.push_str(&content[..section_end]);
            // Ensure there's a newline before our bullet
            if !new.ends_with('\n') {
                new.push('\n');
            }
            new.push_str(&format!("- {sanitized}\n"));
            new.push_str(&content[section_end..]);
            new
        } else {
            // Section doesn't exist — insert before ## Notes if present, else append
            let mut new = content.clone();
            let insert_section = format!("\n\n## Acceptance Criteria\n\n- {sanitized}\n");
            if let Some(notes_pos) = new.find("\n## Notes") {
                new.insert_str(notes_pos, &insert_section);
            } else {
                new.push_str(&insert_section);
            }
            new
        };

        self.write(&new_content)?;

        crate::events::log_objective_field_updated(
            &self.id,
            "acceptance-criteria",
            None,
            Some(&sanitized),
            None,
        );

        Ok(())
    }

    /// Add a timestamped note to the objective.
    ///
    /// Adds the note text under a "## Notes" section. If the section doesn't exist,
    /// it will be created. Each note gets an H3 timestamp heading.
    pub fn add_note(&self, note_text: &str) -> Result<()> {
        if note_text.trim().is_empty() {
            return Err(JanusError::EmptyNote);
        }

        let timestamp = crate::utils::iso_date();

        let content = self.read_content()?;
        let mut new_content = content;
        if !new_content.contains("## Notes") {
            new_content.push_str("\n\n## Notes");
        }
        new_content.push_str(&format!("\n\n### {timestamp}\n\n{note_text}"));
        self.write(&new_content)?;

        crate::events::log_objective_note_added(&self.id, None);

        Ok(())
    }

    /// Update a field in the objective's frontmatter.
    ///
    /// This reads the file, updates the YAML field, and writes back.
    /// Logs an `ObjectiveFieldUpdated` event after the update.
    pub fn update_field(&self, field: &str, value: &str) -> Result<()> {
        // Read current value for event logging
        let raw_content = self.read_content()?;
        let old_value = self.extract_field_value_for_logging(&raw_content, field);
        let new_content = crate::ticket::update_field(&raw_content, field, value)?;
        self.write(&new_content)?;

        crate::events::log_objective_field_updated(
            &self.id,
            field,
            old_value.as_deref(),
            Some(value),
            None,
        );

        Ok(())
    }

    /// Extract the current value of a field from raw content for event logging.
    fn extract_field_value_for_logging(&self, raw_content: &str, field: &str) -> Option<String> {
        if let Ok(metadata) = parse_objective_content(raw_content) {
            match field {
                "satisfied-by" => {
                    if metadata.satisfied_by.is_empty() {
                        None
                    } else {
                        Some(metadata.satisfied_by.join(", "))
                    }
                }
                _ => None,
            }
        } else {
            None
        }
    }

    fn format_refs_for_logging(refs: &[String]) -> String {
        if refs.is_empty() {
            String::new()
        } else {
            refs.join(", ")
        }
    }

    /// Add a ticket or plan reference to the satisfied-by list.
    ///
    /// Returns an error if the reference is already present (no duplicates).
    pub fn add_ref(&self, ref_id: &str) -> Result<()> {
        self.add_ref_with_actor(ref_id, None)
    }

    /// Add a ticket or plan reference with a specific event actor.
    pub fn add_ref_with_actor(
        &self,
        ref_id: &str,
        actor: Option<crate::events::Actor>,
    ) -> Result<()> {
        let mut metadata = self.read()?;
        if metadata.satisfied_by.contains(&ref_id.to_string()) {
            return Err(JanusError::DuplicateObjectiveRef(
                ref_id.to_string(),
                self.id.clone(),
            ));
        }
        let old_value = Self::format_refs_for_logging(&metadata.satisfied_by);
        metadata.satisfied_by.push(ref_id.to_string());
        let new_value = Self::format_refs_for_logging(&metadata.satisfied_by);
        let content = crate::objective::serialize::serialize_objective(&metadata)?;
        self.write(&content)?;
        crate::events::log_objective_field_updated(
            &self.id,
            "satisfied-by",
            Some(&old_value),
            Some(&new_value),
            actor,
        );
        Ok(())
    }

    /// Remove a ticket or plan reference from the satisfied-by list.
    ///
    /// Returns an error if the reference is not present.
    pub fn remove_ref(&self, ref_id: &str) -> Result<()> {
        self.remove_ref_with_actor(ref_id, None)
    }

    /// Remove a ticket or plan reference with a specific event actor.
    pub fn remove_ref_with_actor(
        &self,
        ref_id: &str,
        actor: Option<crate::events::Actor>,
    ) -> Result<()> {
        let mut metadata = self.read()?;
        let old_value = Self::format_refs_for_logging(&metadata.satisfied_by);
        let initial_len = metadata.satisfied_by.len();
        metadata.satisfied_by.retain(|r| r != ref_id);
        if metadata.satisfied_by.len() == initial_len {
            return Err(JanusError::ObjectiveRefNotFound(
                ref_id.to_string(),
                self.id.clone(),
            ));
        }
        let new_value = Self::format_refs_for_logging(&metadata.satisfied_by);
        let content = crate::objective::serialize::serialize_objective(&metadata)?;
        self.write(&content)?;
        crate::events::log_objective_field_updated(
            &self.id,
            "satisfied-by",
            Some(&old_value),
            Some(&new_value),
            actor,
        );
        Ok(())
    }

    /// Remove all references from the satisfied-by list.
    pub fn reset_refs(&self) -> Result<()> {
        self.reset_refs_with_actor(None)
    }

    /// Remove all references with a specific event actor.
    pub fn reset_refs_with_actor(&self, actor: Option<crate::events::Actor>) -> Result<()> {
        let mut metadata = self.read()?;
        let old_value = Self::format_refs_for_logging(&metadata.satisfied_by);
        metadata.satisfied_by.clear();
        let content = crate::objective::serialize::serialize_objective(&metadata)?;
        self.write(&content)?;
        crate::events::log_objective_field_updated(
            &self.id,
            "satisfied-by",
            Some(&old_value),
            None,
            actor,
        );
        Ok(())
    }

    /// Build a hook context for this objective.
    pub fn hook_context(&self) -> HookContext {
        HookContext::new()
            .with_item_type(EntityType::Objective)
            .with_item_id(&self.id)
            .with_file_path(&self.file_path)
    }
}

impl Entity for Objective {
    type Metadata = ObjectiveMetadata;

    async fn find(partial_id: &str) -> Result<Self> {
        Objective::find(partial_id).await
    }

    fn read(&self) -> Result<ObjectiveMetadata> {
        self.read()
    }

    fn write(&self, content: &str) -> Result<()> {
        self.write(content)
    }

    fn delete(&self) -> Result<()> {
        self.delete()
    }

    fn exists(&self) -> bool {
        self.exists()
    }
}

/// Find an objective by partial ID.
///
/// Uses the in-memory store when available, falling back to filesystem search.
async fn find_objective_by_id(partial_id: &str) -> Result<PathBuf> {
    let dir = objectives_dir();

    let trimmed = partial_id.trim();
    if trimmed.is_empty() {
        return Err(JanusError::InvalidObjectiveIdFormat(partial_id.to_string()));
    }

    // Check for invalid characters
    if !trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(JanusError::InvalidObjectiveIdFormat(partial_id.to_string()));
    }

    // Try store-based lookup first
    if let Ok(store) = crate::store::get_or_init_store().await {
        let matches = store.find_objective_by_partial_id(trimmed);
        match matches.len() {
            0 => {
                // Not found in store — fall back to filesystem
                return find_objective_by_id_filesystem(trimmed, &dir);
            }
            1 => {
                return Ok(dir.join(format!("{}.md", matches[0])));
            }
            _ => {
                // Check for exact match among multiple results
                if let Some(exact) = matches.iter().find(|id| id.as_str() == trimmed) {
                    return Ok(dir.join(format!("{exact}.md")));
                }
                return Err(JanusError::AmbiguousObjectiveId(
                    partial_id.to_string(),
                    matches,
                ));
            }
        }
    }

    // Fall back to filesystem-only search
    find_objective_by_id_filesystem(trimmed, &dir)
}

/// Filesystem-based find implementation for objectives.
fn find_objective_by_id_filesystem(partial_id: &str, dir: &std::path::Path) -> Result<PathBuf> {
    let files = match find_markdown_files_from_path(dir) {
        Ok(files) => files,
        Err(_) => {
            // Directory doesn't exist yet — treat as not found
            return Err(JanusError::ObjectiveNotFound(ObjectiveId::new_unchecked(
                partial_id,
            )));
        }
    };

    // Check for exact match first
    let exact_name = format!("{partial_id}.md");
    if files.iter().any(|f| f == &exact_name) {
        return Ok(dir.join(&exact_name));
    }

    // Then check for partial matches
    let matches: Vec<_> = files.iter().filter(|f| f.contains(partial_id)).collect();

    match matches.len() {
        0 => Err(JanusError::ObjectiveNotFound(ObjectiveId::new_unchecked(
            partial_id,
        ))),
        1 => Ok(dir.join(matches[0])),
        _ => Err(JanusError::AmbiguousObjectiveId(
            partial_id.to_string(),
            matches.iter().map(|m| m.replace(".md", "")).collect(),
        )),
    }
}

/// Get all objectives from disk.
pub fn get_all_objectives_from_disk() -> ObjectiveLoadResult {
    let mut result = ObjectiveLoadResult::new();
    let o_dir = objectives_dir();

    // If objectives directory doesn't exist, return empty result
    if !o_dir.exists() {
        return result;
    }

    let files = match find_markdown_files(&o_dir) {
        Ok(files) => files,
        Err(e) => {
            result.add_failure(
                "<objectives directory>",
                format!("failed to read directory: {e}"),
            );
            return result;
        }
    };

    for file in files {
        let file_path = o_dir.join(&file);
        match fs::read_to_string(&file_path) {
            Ok(content) => match parse_objective_content(&content) {
                Ok(mut metadata) => {
                    if metadata.id.is_none() {
                        metadata.id = Some(ObjectiveId::new_unchecked(
                            file.strip_suffix(".md").unwrap_or(&file),
                        ));
                    }
                    metadata.file_path = Some(file_path);
                    result.add_objective(metadata);
                }
                Err(e) => {
                    result.add_failure(&file, format!("parse error: {e}"));
                }
            },
            Err(e) => {
                result.add_failure(&file, format!("read error: {e}"));
            }
        }
    }

    result
}

/// Get all objectives, preferring the in-memory store when available.
pub async fn get_all_objectives() -> Result<ObjectiveLoadResult> {
    if let Ok(store) = crate::store::get_or_init_store().await {
        let objectives = store.get_all_objective_metadata();
        let mut result = ObjectiveLoadResult::new();
        for objective in objectives {
            result.add_objective(objective);
        }
        return Ok(result);
    }

    // Fall back to disk
    Ok(get_all_objectives_from_disk())
}

/// Ensure the objectives directory exists.
pub fn ensure_objectives_dir() -> Result<()> {
    let o_dir = objectives_dir();
    fs::create_dir_all(&o_dir).map_err(|e| JanusError::StorageError {
        operation: "create",
        item_type: "directory",
        path: o_dir.clone(),
        source: e,
    })?;
    crate::utils::ensure_gitignore();
    Ok(())
}

/// Generate a unique objective ID with collision checking.
pub fn generate_objective_id() -> Result<String> {
    use crate::utils::generate_hash;

    const RETRIES_PER_LENGTH: u32 = 40;
    let o_dir = objectives_dir();

    for length in 4..=8 {
        for _ in 0..RETRIES_PER_LENGTH {
            let hash = generate_hash(length);
            let candidate = format!("objv-{hash}");
            let filename = format!("{candidate}.md");

            if !o_dir.join(&filename).exists() {
                return Ok(candidate);
            }
        }
    }

    Err(JanusError::IdGenerationFailed(format!(
        "Failed to generate unique objective ID after trying hash lengths 4-8 with {RETRIES_PER_LENGTH} retries each"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JanusRootGuard;

    #[test]
    fn test_generate_objective_id_format() {
        let temp = tempfile::TempDir::new().unwrap();
        let _guard = JanusRootGuard::new(temp.path().join(".janus"));

        let id = generate_objective_id().unwrap();
        assert!(id.starts_with("objv-"));
        let parts: Vec<&str> = id.splitn(2, '-').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "objv");
        assert_eq!(parts[1].len(), 4);
        assert!(parts[1].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_objective_with_id() {
        let obj = Objective::with_id("objv-test").unwrap();
        assert_eq!(obj.id, "objv-test");
        assert!(obj.file_path.to_string_lossy().contains("objectives"));
        assert!(obj.file_path.to_string_lossy().contains("objv-test.md"));
    }

    #[test]
    fn test_objective_with_id_invalid_prefix() {
        let result = Objective::with_id("plan-test");
        assert!(result.is_err());
    }

    #[test]
    fn test_objective_with_id_empty() {
        let result = Objective::with_id("");
        assert!(result.is_err());
    }

    #[test]
    fn test_objective_with_id_invalid_characters() {
        let result = Objective::with_id("objv-../../../etc");
        assert!(result.is_err());
    }

    #[test]
    fn test_objective_new() {
        let path = PathBuf::from(".janus/objectives/objv-abc123.md");
        let obj = Objective::new(path).unwrap();
        assert_eq!(obj.id, "objv-abc123");
    }

    #[test]
    fn test_ensure_objectives_dir() {
        let temp = tempfile::TempDir::new().unwrap();
        let _guard = JanusRootGuard::new(temp.path().join(".janus"));

        ensure_objectives_dir().unwrap();

        let o_dir = objectives_dir();
        assert!(o_dir.exists());
    }

    #[test]
    fn test_get_all_objectives_empty() {
        let temp = tempfile::TempDir::new().unwrap();
        let _guard = JanusRootGuard::new(temp.path().join(".janus"));

        let result = get_all_objectives_from_disk();
        assert_eq!(result.success_count(), 0);
        assert!(!result.has_failures());
    }

    #[test]
    fn test_get_all_objectives_with_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let janus_dir = temp.path().join(".janus");
        let _guard = JanusRootGuard::new(&janus_dir);

        ensure_objectives_dir().unwrap();

        let content = r#"---
id: objv-test
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# Test Objective

## Description

A test.

## Acceptance Criteria

- Do something
"#;
        let file_path = objectives_dir().join("objv-test.md");
        fs::write(&file_path, content).unwrap();

        let result = get_all_objectives_from_disk();
        assert_eq!(result.success_count(), 1);
        assert!(!result.has_failures());

        let objectives = result.into_objectives();
        assert_eq!(objectives[0].id.as_deref(), Some("objv-test"));
        assert_eq!(objectives[0].title, Some("Test Objective".to_string()));
    }

    #[test]
    fn test_objective_read_write_roundtrip() {
        let temp = tempfile::TempDir::new().unwrap();
        let janus_dir = temp.path().join(".janus");
        let _guard = JanusRootGuard::new(&janus_dir);

        ensure_objectives_dir().unwrap();

        let content = r#"---
id: objv-rwrt
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
satisfied-by:
  - plan-x1y2
---
# RW Roundtrip

## Description

Description here.

## Acceptance Criteria

- Criterion A
- Criterion B
"#;

        let obj = Objective::with_id("objv-rwrt").unwrap();
        obj.write(content).unwrap();

        assert!(obj.exists());

        let metadata = obj.read().unwrap();
        assert_eq!(metadata.id.as_deref(), Some("objv-rwrt"));
        assert_eq!(metadata.title, Some("RW Roundtrip".to_string()));
        assert_eq!(metadata.satisfied_by, vec!["plan-x1y2".to_string()]);
        assert_eq!(metadata.acceptance_criteria.len(), 2);
    }

    #[test]
    fn test_objective_add_note() {
        let temp = tempfile::TempDir::new().unwrap();
        let janus_dir = temp.path().join(".janus");
        let _guard = JanusRootGuard::new(&janus_dir);

        ensure_objectives_dir().unwrap();

        let content = r#"---
id: objv-note
created: 2024-01-01T00:00:00Z
---
# Note Test

## Description

A test.
"#;

        let obj = Objective::with_id("objv-note").unwrap();
        obj.write(content).unwrap();

        obj.add_note("This is a test note.").unwrap();

        let updated = obj.read_content().unwrap();
        assert!(updated.contains("## Notes"));
        assert!(updated.contains("This is a test note."));
        // Should have a timestamp heading (H3)
        assert!(updated.contains("### "));
    }

    #[test]
    fn test_objective_add_note_empty_fails() {
        let temp = tempfile::TempDir::new().unwrap();
        let janus_dir = temp.path().join(".janus");
        let _guard = JanusRootGuard::new(&janus_dir);

        ensure_objectives_dir().unwrap();

        let content = r#"---
id: objv-note2
created: 2024-01-01T00:00:00Z
---
# Note Test 2
"#;

        let obj = Objective::with_id("objv-note2").unwrap();
        obj.write(content).unwrap();

        let result = obj.add_note("");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::EmptyNote => {}
            other => panic!("Expected EmptyNote, got: {other:?}"),
        }
    }

    #[test]
    fn test_sanitize_criterion_plain_text() {
        assert_eq!(
            Objective::sanitize_criterion("Simple criterion"),
            "Simple criterion"
        );
    }

    #[test]
    fn test_sanitize_criterion_strips_headings() {
        assert_eq!(
            Objective::sanitize_criterion("## Heading content"),
            "Heading content"
        );
        assert_eq!(
            Objective::sanitize_criterion("### Deep heading"),
            "Deep heading"
        );
    }

    #[test]
    fn test_sanitize_criterion_collapses_newlines() {
        assert_eq!(
            Objective::sanitize_criterion("line one\nline two\nline three"),
            "line one line two line three"
        );
    }

    #[test]
    fn test_sanitize_criterion_strips_bullets() {
        assert_eq!(
            Objective::sanitize_criterion("- already a bullet"),
            "already a bullet"
        );
        assert_eq!(
            Objective::sanitize_criterion("* star bullet"),
            "star bullet"
        );
        assert_eq!(
            Objective::sanitize_criterion("+ plus bullet"),
            "plus bullet"
        );
    }

    #[test]
    fn test_sanitize_criterion_complex_input() {
        let input = "## Acceptance Criteria\n\n- The system should handle errors\n- And also this";
        let result = Objective::sanitize_criterion(input);
        assert_eq!(
            result,
            "Acceptance Criteria The system should handle errors And also this"
        );
        assert!(!result.contains('#'));
        assert!(!result.contains('\n'));
    }

    #[test]
    fn test_sanitize_criterion_empty_after_stripping() {
        assert_eq!(Objective::sanitize_criterion("##   \n\n  "), "");
        assert_eq!(Objective::sanitize_criterion("- "), "");
    }

    #[test]
    fn test_add_criterion_to_existing_section() {
        let temp = tempfile::TempDir::new().unwrap();
        let janus_dir = temp.path().join(".janus");
        let _guard = JanusRootGuard::new(&janus_dir);

        ensure_objectives_dir().unwrap();

        let content = r#"---
id: objv-crit1
created: 2024-01-01T00:00:00Z
---
# Criterion Test

## Description

A test.

## Acceptance Criteria

- Existing criterion
"#;

        let obj = Objective::with_id("objv-crit1").unwrap();
        obj.write(content).unwrap();

        obj.add_criterion("New criterion").unwrap();

        let updated = obj.read_content().unwrap();
        assert!(updated.contains("- Existing criterion"));
        assert!(updated.contains("- New criterion"));
    }

    #[test]
    fn test_add_criterion_creates_section() {
        let temp = tempfile::TempDir::new().unwrap();
        let janus_dir = temp.path().join(".janus");
        let _guard = JanusRootGuard::new(&janus_dir);

        ensure_objectives_dir().unwrap();

        let content = r#"---
id: objv-crit2
created: 2024-01-01T00:00:00Z
---
# No Criteria Yet

## Description

Just a description.
"#;

        let obj = Objective::with_id("objv-crit2").unwrap();
        obj.write(content).unwrap();

        obj.add_criterion("Brand new criterion").unwrap();

        let updated = obj.read_content().unwrap();
        assert!(updated.contains("## Acceptance Criteria"));
        assert!(updated.contains("- Brand new criterion"));
    }

    #[test]
    fn test_add_criterion_creates_section_before_notes() {
        let temp = tempfile::TempDir::new().unwrap();
        let janus_dir = temp.path().join(".janus");
        let _guard = JanusRootGuard::new(&janus_dir);

        ensure_objectives_dir().unwrap();

        let content = r#"---
id: objv-crit3
created: 2024-01-01T00:00:00Z
---
# Before Notes Test

## Description

Description.

## Notes

### 2024-01-01T00:00:00Z

A note.
"#;

        let obj = Objective::with_id("objv-crit3").unwrap();
        obj.write(content).unwrap();

        obj.add_criterion("Criterion before notes").unwrap();

        let updated = obj.read_content().unwrap();
        let criteria_pos = updated.find("## Acceptance Criteria").unwrap();
        let notes_pos = updated.find("## Notes").unwrap();
        assert!(
            criteria_pos < notes_pos,
            "Acceptance Criteria should appear before Notes"
        );
        assert!(updated.contains("- Criterion before notes"));
    }

    #[test]
    fn test_add_criterion_sanitizes_input() {
        let temp = tempfile::TempDir::new().unwrap();
        let janus_dir = temp.path().join(".janus");
        let _guard = JanusRootGuard::new(&janus_dir);

        ensure_objectives_dir().unwrap();

        let content = r#"---
id: objv-crit4
created: 2024-01-01T00:00:00Z
---
# Sanitize Test

## Acceptance Criteria

- First
"#;

        let obj = Objective::with_id("objv-crit4").unwrap();
        obj.write(content).unwrap();

        obj.add_criterion("## Heading\n\n- bullet content\n- more stuff")
            .unwrap();

        let updated = obj.read_content().unwrap();
        // Should be a single sanitized bullet, no headings or nested bullets
        assert!(updated.contains("- Heading bullet content more stuff"));
        assert!(!updated.contains("## Heading"));
    }

    #[test]
    fn test_add_criterion_empty_fails() {
        let temp = tempfile::TempDir::new().unwrap();
        let janus_dir = temp.path().join(".janus");
        let _guard = JanusRootGuard::new(&janus_dir);

        ensure_objectives_dir().unwrap();

        let content = r#"---
id: objv-crit5
created: 2024-01-01T00:00:00Z
---
# Empty Test
"#;

        let obj = Objective::with_id("objv-crit5").unwrap();
        obj.write(content).unwrap();

        let result = obj.add_criterion("");
        assert!(result.is_err());
    }

    #[test]
    fn test_objective_delete() {
        let temp = tempfile::TempDir::new().unwrap();
        let janus_dir = temp.path().join(".janus");
        let _guard = JanusRootGuard::new(&janus_dir);

        ensure_objectives_dir().unwrap();

        let content = r#"---
id: objv-del
created: 2024-01-01T00:00:00Z
---
# Delete Test
"#;

        let obj = Objective::with_id("objv-del").unwrap();
        obj.write(content).unwrap();
        assert!(obj.exists());

        obj.delete().unwrap();
        assert!(!obj.exists());
    }
}
