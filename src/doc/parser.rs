//! Document parser for project knowledge documents.
//!
//! Handles parsing of YAML frontmatter and markdown body from document files,
//! as well as serializing metadata back to full file content.

use std::collections::HashMap;

use serde::Deserialize;

use crate::doc::types::{DocLabel, DocMetadata};
use crate::error::{JanusError, Result};
use crate::parser::parse_document_raw;

/// Strict frontmatter struct for YAML deserialization.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DocFrontmatter {
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<DocLabel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created: Option<crate::types::CreatedAt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated: Option<crate::types::CreatedAt>,
}

/// Parse a document file's content into DocMetadata.
///
/// This is the main entry point for document parsing. It parses the document
/// and converts it to DocMetadata, extracting both frontmatter fields
/// and body-derived fields (title).
pub fn parse_doc_content(content: &str) -> Result<DocMetadata> {
    let (frontmatter_raw, body) = parse_document_raw(content)?;
    doc_metadata_from_document(&frontmatter_raw, &body)
}

/// Convert parsed document parts to DocMetadata.
///
/// This handles the document-specific conversion logic, including:
/// - Deserializing frontmatter into strict DocFrontmatter
/// - Mapping strict frontmatter to lenient DocMetadata
/// - Extracting title from the first H1 heading
fn doc_metadata_from_document(frontmatter_raw: &str, body: &str) -> Result<DocMetadata> {
    let frontmatter: DocFrontmatter = serde_yaml_ng::from_str(frontmatter_raw)?;

    let metadata = DocMetadata {
        label: frontmatter.label,
        description: frontmatter.description,
        tags: frontmatter.tags,
        created: frontmatter.created,
        updated: frontmatter.updated,
        title: extract_title(body),
        file_path: None,
        extra_frontmatter: None,
    };

    Ok(metadata)
}

/// Extract the title from the body (first H1 heading)
fn extract_title(body: &str) -> Option<String> {
    crate::parser::TITLE_RE
        .captures(body)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

/// Serialize DocMetadata to full file content.
///
/// Generates the complete markdown document with YAML frontmatter and body.
/// The body includes the title as an H1 heading and any description.
pub fn serialize_doc(metadata: &DocMetadata) -> Result<String> {
    let mut frontmatter_map: HashMap<String, serde_yaml_ng::Value> = HashMap::new();

    // Add label
    if let Some(label) = &metadata.label {
        frontmatter_map.insert(
            "label".to_string(),
            serde_yaml_ng::Value::String(label.to_string()),
        );
    }

    // Add description
    if let Some(description) = &metadata.description {
        frontmatter_map.insert(
            "description".to_string(),
            serde_yaml_ng::Value::String(description.clone()),
        );
    }

    // Add tags
    if !metadata.tags.is_empty() {
        let tags: Vec<serde_yaml_ng::Value> = metadata
            .tags
            .iter()
            .map(|t| serde_yaml_ng::Value::String(t.clone()))
            .collect();
        frontmatter_map.insert("tags".to_string(), serde_yaml_ng::Value::Sequence(tags));
    }

    // Add created timestamp
    if let Some(created) = &metadata.created {
        frontmatter_map.insert(
            "created".to_string(),
            serde_yaml_ng::Value::String(created.to_string()),
        );
    }

    // Add updated timestamp
    if let Some(updated) = &metadata.updated {
        frontmatter_map.insert(
            "updated".to_string(),
            serde_yaml_ng::Value::String(updated.to_string()),
        );
    }

    // Serialize frontmatter
    let frontmatter_yaml = serde_yaml_ng::to_string(&frontmatter_map)
        .map_err(|e| JanusError::InvalidFormat(format!("Failed to serialize frontmatter: {e}")))?;

    // Build body
    let mut body_parts = Vec::new();

    // Add title as H1
    if let Some(title) = &metadata.title {
        body_parts.push(format!("# {title}"));
    }

    // Add description if present
    if let Some(description) = &metadata.description {
        if !description.is_empty() {
            body_parts.push(String::new());
            body_parts.push(description.clone());
        }
    }

    let body = if body_parts.is_empty() {
        "\n".to_string()
    } else {
        format!("\n{}\n", body_parts.join("\n"))
    };

    // Combine frontmatter and body
    Ok(format!("---\n{frontmatter_yaml}---{body}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_doc() {
        let content = r#"---
label: architecture
description: System architecture overview
tags:
  - design
  - system
created: 2024-01-01T00:00:00Z
---
# Architecture

This document describes the system architecture.
"#;

        let metadata = parse_doc_content(content).unwrap();
        assert_eq!(
            metadata.label.as_ref().map(|l| l.as_ref()),
            Some("architecture")
        );
        assert_eq!(
            metadata.description,
            Some("System architecture overview".to_string())
        );
        assert_eq!(metadata.tags, vec!["design", "system"]);
        assert_eq!(metadata.title, Some("Architecture".to_string()));
    }

    #[test]
    fn test_parse_doc_with_crlf() {
        let content = "---\r\n\
label: test\r\n\
---\r\n\
# Test Doc\r\n\
\r\n\
Content.\r\n";

        let metadata = parse_doc_content(content).unwrap();
        assert_eq!(metadata.label.as_ref().map(|l| l.as_ref()), Some("test"));
        assert_eq!(metadata.title, Some("Test Doc".to_string()));
    }

    #[test]
    fn test_parse_missing_frontmatter() {
        let content = "# No frontmatter\n\nJust content.";
        let result = parse_doc_content(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_serialize_doc() {
        let metadata = DocMetadata {
            label: Some(DocLabel::new_unchecked("test-doc")),
            description: Some("A test document".to_string()),
            tags: vec!["test".to_string(), "example".to_string()],
            created: Some(crate::types::CreatedAt::new_unchecked(
                "2024-01-01T00:00:00Z",
            )),
            updated: None,
            title: Some("Test Document".to_string()),
            file_path: None,
            extra_frontmatter: None,
        };

        let content = serialize_doc(&metadata).unwrap();
        assert!(content.contains("label: test-doc"));
        assert!(content.contains("description: A test document"));
        assert!(content.contains("- test"));
        assert!(content.contains("- example"));
        assert!(content.contains("created: 2024-01-01T00:00:00Z"));
        assert!(content.contains("# Test Document"));
    }

    #[test]
    fn test_serialize_doc_minimal() {
        let metadata = DocMetadata {
            label: Some(DocLabel::new_unchecked("minimal")),
            description: None,
            tags: vec![],
            created: None,
            updated: None,
            title: Some("Minimal".to_string()),
            file_path: None,
            extra_frontmatter: None,
        };

        let content = serialize_doc(&metadata).unwrap();
        assert!(content.contains("label: minimal"));
        assert!(content.contains("# Minimal"));
        assert!(!content.contains("description:"));
        assert!(!content.contains("tags:"));
    }

    #[test]
    fn test_roundtrip() {
        let original_content = r#"---
label: roundtrip-test
description: Testing roundtrip
tags:
  - test
---
# Roundtrip Test

This is the content.
"#;

        let metadata = parse_doc_content(original_content).unwrap();
        let serialized = serialize_doc(&metadata).unwrap();

        // Parse again to verify
        let reparsed = parse_doc_content(&serialized).unwrap();
        assert_eq!(reparsed.label, metadata.label);
        assert_eq!(reparsed.description, metadata.description);
        assert_eq!(reparsed.tags, metadata.tags);
        assert_eq!(reparsed.title, metadata.title);
    }
}
