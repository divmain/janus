//! Objective serializer for writing objective files.
//!
//! Serializes `ObjectiveMetadata` back to markdown with YAML frontmatter,
//! preserving raw content for round-trip fidelity where available.

use serde_yaml_ng as yaml;

use crate::error::{JanusError, Result};
use crate::objective::types::ObjectiveMetadata;

/// Serialize `ObjectiveMetadata` to full markdown file content.
///
/// The output format is:
/// ```text
/// ---
/// id: objv-xxxx
/// uuid: ...
/// created: ...
/// satisfied-by: ...    # if present
/// ---
/// # Title
///
/// ## Description
///
/// Description content...
///
/// ## Acceptance Criteria
///
/// - Criterion 1
/// - Criterion 2
///
/// ## Notes
///
/// Notes content...
/// ```
///
/// Uses raw fields (`description_raw`, `acceptance_criteria_raw`, `notes_raw`)
/// when available for round-trip fidelity, falling back to generating content
/// from parsed fields.
pub fn serialize_objective(metadata: &ObjectiveMetadata) -> Result<String> {
    // Build frontmatter using ordered Mapping to control field order
    let mut frontmatter = yaml::Mapping::new();

    // Add id
    if let Some(id) = &metadata.id {
        frontmatter.insert(
            yaml::Value::String("id".to_string()),
            yaml::Value::String(id.to_string()),
        );
    }

    // Add uuid
    if let Some(uuid) = &metadata.uuid {
        frontmatter.insert(
            yaml::Value::String("uuid".to_string()),
            yaml::Value::String(uuid.clone()),
        );
    }

    // Add created
    if let Some(created) = &metadata.created {
        frontmatter.insert(
            yaml::Value::String("created".to_string()),
            yaml::Value::String(created.to_string()),
        );
    }

    // Add satisfied-by
    if let Some(satisfied_by) = &metadata.satisfied_by {
        frontmatter.insert(
            yaml::Value::String("satisfied-by".to_string()),
            yaml::Value::String(satisfied_by.clone()),
        );
    }

    // Serialize frontmatter YAML
    let frontmatter_yaml = yaml::to_string(&frontmatter)
        .map_err(|e| JanusError::InvalidFormat(format!("Failed to serialize frontmatter: {e}")))?;

    // Build body sections
    let mut body_parts = Vec::new();

    // H1 title
    if let Some(title) = &metadata.title {
        body_parts.push(format!("# {title}"));
    }

    // Description section
    let has_description = metadata.description_raw.is_some() || metadata.description.is_some();
    if has_description {
        body_parts.push(String::new());
        body_parts.push("## Description".to_string());
        if let Some(raw) = &metadata.description_raw {
            body_parts.push(raw.clone());
        } else if let Some(desc) = &metadata.description {
            body_parts.push(String::new());
            body_parts.push(desc.clone());
        }
    }

    // Acceptance Criteria section
    let has_criteria =
        metadata.acceptance_criteria_raw.is_some() || !metadata.acceptance_criteria.is_empty();
    if has_criteria {
        body_parts.push(String::new());
        body_parts.push("## Acceptance Criteria".to_string());
        if let Some(raw) = &metadata.acceptance_criteria_raw {
            body_parts.push(raw.clone());
        } else {
            body_parts.push(String::new());
            for criterion in &metadata.acceptance_criteria {
                body_parts.push(format!("- {criterion}"));
            }
        }
    }

    // Notes section
    if let Some(notes_raw) = &metadata.notes_raw {
        body_parts.push(String::new());
        body_parts.push("## Notes".to_string());
        body_parts.push(notes_raw.clone());
    }

    let body = if body_parts.is_empty() {
        "\n".to_string()
    } else {
        format!("\n{}\n", body_parts.join("\n"))
    };

    Ok(format!("---\n{frontmatter_yaml}---{body}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objective::parser::parse_objective_content;
    use crate::types::{CreatedAt, ObjectiveId};

    #[test]
    fn test_serialize_basic_objective() {
        let metadata = ObjectiveMetadata {
            id: Some(ObjectiveId::new_unchecked("objv-test")),
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".to_string()),
            created: Some(CreatedAt::new_unchecked("2024-01-01T00:00:00Z")),
            satisfied_by: Some("plan-x1y2".to_string()),
            title: Some("Test Objective".to_string()),
            description: Some("A test objective description.".to_string()),
            acceptance_criteria: vec!["Criterion 1".to_string(), "Criterion 2".to_string()],
            ..Default::default()
        };

        let content = serialize_objective(&metadata).unwrap();
        assert!(content.contains("id: objv-test"));
        assert!(content.contains("uuid: 550e8400"));
        assert!(content.contains("created:") && content.contains("2024-01-01T00:00:00Z"));
        assert!(content.contains("satisfied-by: plan-x1y2"));
        assert!(content.contains("# Test Objective"));
        assert!(content.contains("## Description"));
        assert!(content.contains("A test objective description."));
        assert!(content.contains("## Acceptance Criteria"));
        assert!(content.contains("- Criterion 1"));
        assert!(content.contains("- Criterion 2"));
    }

    #[test]
    fn test_serialize_minimal_objective() {
        let metadata = ObjectiveMetadata {
            id: Some(ObjectiveId::new_unchecked("objv-min")),
            title: Some("Minimal".to_string()),
            ..Default::default()
        };

        let content = serialize_objective(&metadata).unwrap();
        assert!(content.contains("id: objv-min"));
        assert!(content.contains("# Minimal"));
        assert!(!content.contains("satisfied-by"));
        assert!(!content.contains("uuid"));
        assert!(!content.contains("## Description"));
        assert!(!content.contains("## Acceptance Criteria"));
    }

    #[test]
    fn test_serialize_with_notes() {
        let metadata = ObjectiveMetadata {
            id: Some(ObjectiveId::new_unchecked("objv-notes")),
            title: Some("With Notes".to_string()),
            notes_raw: Some("\n### 2024-01-15T10:30:00Z\n\nSome note content.".to_string()),
            ..Default::default()
        };

        let content = serialize_objective(&metadata).unwrap();
        assert!(content.contains("## Notes"));
        assert!(content.contains("### 2024-01-15T10:30:00Z"));
        assert!(content.contains("Some note content."));
    }

    #[test]
    fn test_roundtrip() {
        let original = r#"---
id: objv-rt01
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
satisfied-by: plan-x1y2
---
# Roundtrip Test

## Description

This is a description.

## Acceptance Criteria

- First criterion
- Second criterion

## Notes

### 2024-01-15T10:30:00Z

A note.
"#;

        let metadata = parse_objective_content(original).unwrap();
        let serialized = serialize_objective(&metadata).unwrap();

        // Re-parse to verify
        let reparsed = parse_objective_content(&serialized).unwrap();
        assert_eq!(reparsed.id, metadata.id);
        assert_eq!(reparsed.uuid, metadata.uuid);
        assert_eq!(reparsed.satisfied_by, metadata.satisfied_by);
        assert_eq!(reparsed.title, metadata.title);
        assert_eq!(
            reparsed.acceptance_criteria.len(),
            metadata.acceptance_criteria.len()
        );
    }
}
