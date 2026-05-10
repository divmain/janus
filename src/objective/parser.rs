//! Objective parser for objective files.
//!
//! Handles parsing of YAML frontmatter and markdown body from objective files,
//! extracting title, description, acceptance criteria, and notes sections.

use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

use crate::error::Result;
use crate::objective::types::ObjectiveMetadata;
use crate::parser::parse_document_raw;
use crate::types::{CreatedAt, ObjectiveId};

/// Regex to match H1 headings for title extraction.
static TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^#\s+(.*)$").expect("title regex should be valid"));

/// Regex to match bullet list items.
static BULLET_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^[-*]\s+(.*)$").expect("bullet regex should be valid"));

/// Strict frontmatter struct for YAML deserialization.
#[derive(Debug, Deserialize)]
struct ObjectiveFrontmatter {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<ObjectiveId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created: Option<CreatedAt>,
    #[serde(rename = "satisfied-by", default, skip_serializing_if = "Vec::is_empty")]
    satisfied_by: Vec<String>,
}

/// Parse an objective file's content into ObjectiveMetadata.
///
/// This is the main entry point for objective parsing. It extracts:
/// - YAML frontmatter (id, uuid, created, satisfied-by)
/// - H1 title
/// - `## Description` section (everything including sub-headings until next H2)
/// - `## Acceptance Criteria` section (bullet list items)
/// - `## Notes` section (raw, for round-trip)
pub fn parse_objective_content(content: &str) -> Result<ObjectiveMetadata> {
    let (frontmatter_raw, body) = parse_document_raw(content)?;

    // Parse strict frontmatter
    let frontmatter: ObjectiveFrontmatter = serde_yaml_ng::from_str(&frontmatter_raw)?;

    // Extract title from body
    let title = TITLE_RE
        .captures(&body)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().trim().to_string());

    // Parse body sections
    let sections = extract_sections(&body);

    // Extract description
    let (description, description_raw) = sections
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case("Description"))
        .map(|s| {
            let raw = s.raw_content.clone();
            let trimmed = raw.trim().to_string();
            let desc = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            };
            (desc, Some(raw))
        })
        .unwrap_or((None, None));

    // Extract acceptance criteria
    let (acceptance_criteria, acceptance_criteria_raw) = sections
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case("Acceptance Criteria"))
        .map(|s| {
            let raw = s.raw_content.clone();
            let items: Vec<String> = BULLET_RE
                .captures_iter(&raw)
                .filter_map(|caps| caps.get(1).map(|m| m.as_str().trim().to_string()))
                .collect();
            (items, Some(raw))
        })
        .unwrap_or((Vec::new(), None));

    // Extract notes (raw only, for round-trip)
    let notes_raw = sections
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case("Notes"))
        .map(|s| s.raw_content.clone());

    Ok(ObjectiveMetadata {
        id: frontmatter.id,
        uuid: frontmatter.uuid,
        created: frontmatter.created,
        satisfied_by: frontmatter.satisfied_by,
        title,
        description,
        description_raw,
        acceptance_criteria,
        acceptance_criteria_raw,
        notes_raw,
        file_path: None,
        body: Some(body),
        extra_frontmatter: None,
    })
}

/// A parsed section from the markdown body.
struct Section {
    /// Section name (text after ##)
    name: String,
    /// Raw content between this H2 and the next H2 (excluding the header line)
    raw_content: String,
}

/// Extract H2 sections from the body.
///
/// Returns a list of sections with their names and raw content. Content between
/// the title (H1) and the first H2 is NOT captured as a section (it's the
/// document's general description, but for objectives, description is explicitly
/// under `## Description`).
fn extract_sections(body: &str) -> Vec<Section> {
    let mut sections = Vec::new();
    let lines: Vec<&str> = body.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        // Look for H2 headings
        if let Some(name) = lines[i].strip_prefix("## ") {
            let name = name.trim().to_string();
            let start = i + 1;

            // Find the end of this section (next H2 or end of document)
            let end = lines[start..]
                .iter()
                .position(|line| line.starts_with("## "))
                .map(|rel| start + rel)
                .unwrap_or(lines.len());

            let raw_content = lines[start..end].join("\n");

            sections.push(Section { name, raw_content });

            i = end;
        } else {
            i += 1;
        }
    }

    sections
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_objective() {
        let content = r#"---
id: objv-a1b2
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
satisfied-by:
  - plan-x1y2
---
# My Objective

## Description

This is the objective description.

## Acceptance Criteria

- Criterion 1
- Criterion 2
- Criterion 3

## Notes

### 2024-01-15T10:30:00Z

Some note content.
"#;

        let metadata = parse_objective_content(content).unwrap();
        assert_eq!(metadata.id.as_deref(), Some("objv-a1b2"));
        assert_eq!(
            metadata.uuid,
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
        assert_eq!(metadata.created.as_deref(), Some("2024-01-01T00:00:00Z"));
        assert_eq!(metadata.satisfied_by, vec!["plan-x1y2".to_string()]);
        assert_eq!(metadata.title, Some("My Objective".to_string()));
        assert_eq!(
            metadata.description,
            Some("This is the objective description.".to_string())
        );
        assert_eq!(metadata.acceptance_criteria.len(), 3);
        assert_eq!(metadata.acceptance_criteria[0], "Criterion 1");
        assert_eq!(metadata.acceptance_criteria[1], "Criterion 2");
        assert_eq!(metadata.acceptance_criteria[2], "Criterion 3");
        assert!(metadata.notes_raw.is_some());
    }

    #[test]
    fn test_parse_objective_no_satisfied_by() {
        let content = r#"---
id: objv-test
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# Unrealized Objective

## Description

An objective without satisfaction.

## Acceptance Criteria

- Must do something
"#;

        let metadata = parse_objective_content(content).unwrap();
        assert_eq!(metadata.id.as_deref(), Some("objv-test"));
        assert!(metadata.satisfied_by.is_empty());
        assert_eq!(metadata.title, Some("Unrealized Objective".to_string()));
        assert_eq!(metadata.acceptance_criteria.len(), 1);
    }

    #[test]
    fn test_parse_objective_description_with_subheadings() {
        let content = r#"---
id: objv-sub
created: 2024-01-01T00:00:00Z
---
# Objective With Sub

## Description

Main description.

### Sub-heading under description
More content here.

### Another sub-heading
Even more content.

## Acceptance Criteria

- Criterion 1
"#;

        let metadata = parse_objective_content(content).unwrap();
        assert!(metadata.description_raw.is_some());
        let raw = metadata.description_raw.unwrap();
        assert!(raw.contains("Main description."));
        assert!(raw.contains("### Sub-heading under description"));
        assert!(raw.contains("### Another sub-heading"));
    }

    #[test]
    fn test_parse_objective_minimal() {
        let content = r#"---
id: objv-min
---
# Minimal Objective
"#;

        let metadata = parse_objective_content(content).unwrap();
        assert_eq!(metadata.id.as_deref(), Some("objv-min"));
        assert_eq!(metadata.title, Some("Minimal Objective".to_string()));
        assert!(metadata.description.is_none());
        assert!(metadata.acceptance_criteria.is_empty());
        assert!(metadata.notes_raw.is_none());
    }

    #[test]
    fn test_parse_objective_missing_frontmatter() {
        let content = "# No frontmatter\n\nJust content.";
        let result = parse_objective_content(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_objective_empty_sections() {
        let content = r#"---
id: objv-empty
created: 2024-01-01T00:00:00Z
---
# Empty Sections

## Description

## Acceptance Criteria

## Notes
"#;

        let metadata = parse_objective_content(content).unwrap();
        assert!(metadata.description.is_none()); // Empty description should be None
        assert!(metadata.acceptance_criteria.is_empty());
        assert!(metadata.notes_raw.is_some());
    }
}
