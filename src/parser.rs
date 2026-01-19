use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::de::DeserializeOwned;
use serde_yaml_ng as yaml;

use crate::error::{JanusError, Result};
use crate::types::TicketMetadata;

// Compile regexes once at program startup
static FRONTMATTER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)^---\n(.*?)\n---\n(.*)$").expect("frontmatter regex should be valid")
});

static TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^#\s+(.*)$").expect("title regex should be valid"));

/// A generic parsed document with YAML frontmatter and body content.
///
/// This struct decouples the parsing logic from domain-specific types like `TicketMetadata`.
/// The parser extracts raw YAML as both a string (for typed deserialization) and a HashMap
/// (for generic access), along with the body content.
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    /// Raw YAML frontmatter string (for typed deserialization)
    pub frontmatter_raw: String,
    /// Parsed YAML frontmatter as key-value pairs (for generic access)
    pub frontmatter: HashMap<String, yaml::Value>,
    /// The body content after the frontmatter (including title)
    pub body: String,
}

/// Split content into YAML frontmatter and markdown body.
///
/// Handles CRLF line endings by normalizing to LF. Returns a tuple of
/// (frontmatter_content, body_content) or an error if frontmatter is missing.
pub fn split_frontmatter(content: &str) -> Result<(String, String)> {
    let normalized = content.replace("\r\n", "\n");

    let captures = FRONTMATTER_RE
        .captures(&normalized)
        .ok_or_else(|| JanusError::InvalidFormat("missing YAML frontmatter".to_string()))?;

    let frontmatter = captures
        .get(1)
        .map(|m| m.as_str())
        .unwrap_or("")
        .to_string();
    let body = captures
        .get(2)
        .map(|m| m.as_str())
        .unwrap_or("")
        .to_string();

    Ok((frontmatter, body))
}

impl ParsedDocument {
    /// Extract the title from the body (first H1 heading)
    pub fn extract_title(&self) -> Option<String> {
        TITLE_RE
            .captures(&self.body)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
    }

    /// Extract a named section from the body (case-insensitive).
    /// Returns the content between the section header and the next H2 or end of document.
    pub fn extract_section(&self, section_name: &str) -> Option<String> {
        let pattern = format!(
            r"(?ims)^##\s+{}\s*\n(.*?)(?:^##\s|\z)",
            regex::escape(section_name)
        );
        let section_re = Regex::new(&pattern).expect("section regex should be valid");

        section_re.captures(&self.body).map(|caps| {
            caps.get(1)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default()
        })
    }

    /// Deserialize the frontmatter into a specific type.
    ///
    /// This uses the raw YAML string for proper type conversion via serde.
    pub fn deserialize_frontmatter<T: DeserializeOwned>(&self) -> Result<T> {
        yaml::from_str(&self.frontmatter_raw)
            .map_err(|e| JanusError::Other(format!("YAML parsing error: {}", e)))
    }
}

/// Parse a document with YAML frontmatter into a generic ParsedDocument.
///
/// The format is:
/// ```text
/// ---
/// key: value
/// key: ["array", "values"]
/// ---
/// # Title
///
/// Body content...
/// ```
///
/// This function returns a generic structure that can be converted to
/// domain-specific types via `TryFrom` implementations.
pub fn parse_document(content: &str) -> Result<ParsedDocument> {
    let (frontmatter_raw, body) = split_frontmatter(content)?;

    let frontmatter: HashMap<String, yaml::Value> = yaml::from_str(&frontmatter_raw)
        .map_err(|e| JanusError::Other(format!("YAML parsing error: {}", e)))?;

    Ok(ParsedDocument {
        frontmatter_raw,
        frontmatter,
        body,
    })
}

/// Parse a ticket file's content into TicketMetadata.
///
/// This is a convenience function that parses the document and converts it
/// to TicketMetadata. For more control, use `parse_document()` directly and
/// implement your own conversion.
pub fn parse_ticket_content(content: &str) -> Result<TicketMetadata> {
    let doc = parse_document(content)?;
    TicketMetadata::try_from(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TicketPriority, TicketStatus, TicketType};

    #[test]
    fn test_parse_basic_ticket() {
        let content = r#"---
id: test-1234
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Test Ticket

This is the description.
"#;

        let metadata = parse_ticket_content(content).unwrap();
        assert_eq!(metadata.id, Some("test-1234".to_string()));
        assert_eq!(metadata.status, Some(TicketStatus::New));
        assert_eq!(metadata.title, Some("Test Ticket".to_string()));
        assert_eq!(metadata.ticket_type, Some(TicketType::Task));
        assert_eq!(metadata.priority, Some(TicketPriority::P2));
        assert!(metadata.deps.is_empty());
    }

    #[test]
    fn test_parse_with_deps() {
        let content = r#"---
id: test-5678
status: new
deps: ["dep-1", "dep-2"]
links: ["link-1"]
---
# Another Ticket
"#;

        let metadata = parse_ticket_content(content).unwrap();
        assert_eq!(metadata.deps, vec!["dep-1", "dep-2"]);
        assert_eq!(metadata.links, vec!["link-1"]);
    }

    #[test]
    fn test_parse_missing_frontmatter() {
        let content = "# No frontmatter\n\nJust content.";
        let result = parse_ticket_content(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_with_completion_summary() {
        let content = r#"---
id: j-a1b2
status: complete
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
---
# Implement cache initialization

Description of the task.

## Completion Summary

Successfully implemented cache initialization using Turso's async API.
Key decisions:
- Used `OnceCell` for global cache singleton
- Implemented corruption detection and auto-recovery

Performance results: Cold start ~22ms, subsequent lookups <5ms.
"#;

        let metadata = parse_ticket_content(content).unwrap();
        assert_eq!(metadata.id, Some("j-a1b2".to_string()));
        assert_eq!(metadata.status, Some(TicketStatus::Complete));

        let summary = metadata.completion_summary.unwrap();
        assert!(summary.contains("Successfully implemented cache initialization"));
        assert!(summary.contains("OnceCell"));
        assert!(summary.contains("Performance results"));
    }

    #[test]
    fn test_parse_completion_summary_with_following_section() {
        let content = r#"---
id: j-c3d4
status: complete
deps: []
links: []
---
# Task Title

Description.

## Completion Summary

This task is done.

## Notes

Some additional notes here.
"#;

        let metadata = parse_ticket_content(content).unwrap();
        let summary = metadata.completion_summary.unwrap();
        assert_eq!(summary, "This task is done.");
        // Ensure Notes section is not included
        assert!(!summary.contains("Notes"));
        assert!(!summary.contains("additional notes"));
    }

    #[test]
    fn test_parse_no_completion_summary() {
        let content = r#"---
id: j-e5f6
status: new
deps: []
links: []
---
# Task Without Summary

Just a description, no completion summary section.
"#;

        let metadata = parse_ticket_content(content).unwrap();
        assert!(metadata.completion_summary.is_none());
    }

    #[test]
    fn test_parse_completion_summary_case_insensitive() {
        let content = r#"---
id: j-g7h8
status: complete
deps: []
links: []
---
# Task Title

## COMPLETION SUMMARY

All caps header should work.
"#;

        let metadata = parse_ticket_content(content).unwrap();
        let summary = metadata.completion_summary.unwrap();
        assert_eq!(summary, "All caps header should work.");
    }

    #[test]
    fn test_parsed_document_extract_section_empty() {
        let content = r#"---
id: test
---
# Title

No summary here."#;
        let doc = parse_document(content).unwrap();
        assert!(doc.extract_section("completion summary").is_none());
    }

    #[test]
    fn test_parsed_document_extract_section_at_end() {
        let content = r#"---
id: test
---
# Title

Description.

## Completion Summary

Final summary content.
"#;
        let doc = parse_document(content).unwrap();
        let summary = doc.extract_section("completion summary").unwrap();
        assert_eq!(summary, "Final summary content.");
    }

    #[test]
    fn test_parsed_document_extract_title() {
        let content = r#"---
id: test
status: new
---
# My Test Title

Body content here.
"#;
        let doc = parse_document(content).unwrap();
        assert_eq!(doc.extract_title(), Some("My Test Title".to_string()));
    }

    #[test]
    fn test_parse_document_returns_generic_structure() {
        let content = r#"---
id: test-1234
status: new
custom_field: custom_value
---
# Title

Body content.
"#;
        let doc = parse_document(content).unwrap();

        // Verify we can access fields generically
        assert!(doc.frontmatter.contains_key("id"));
        assert!(doc.frontmatter.contains_key("status"));
        assert!(doc.frontmatter.contains_key("custom_field"));

        // Verify body is preserved
        assert!(doc.body.contains("# Title"));
        assert!(doc.body.contains("Body content"));
    }

    #[test]
    fn test_yaml_with_multiline_string() {
        let content = r#"---
id: test-1234
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
external-ref: |
  This is a multi-line
  string using YAML block
  scalar syntax
---
# Test Ticket

Description.
"#;

        let metadata = parse_ticket_content(content).unwrap();
        assert_eq!(metadata.id, Some("test-1234".to_string()));
        assert!(metadata.external_ref.is_some());
        let ref_str = metadata.external_ref.unwrap();
        assert!(ref_str.contains("multi-line"));
        assert!(ref_str.contains("scalar syntax"));
    }

    #[test]
    fn test_yaml_with_comments() {
        let content = r#"---
# This is a YAML comment that should be ignored
id: test-5678  # Inline comment
status: next   # Another inline comment
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 1
---
# Comment Test

YAML comments should be handled properly.
"#;

        let metadata = parse_ticket_content(content).unwrap();
        assert_eq!(metadata.id, Some("test-5678".to_string()));
        assert_eq!(metadata.status, Some(TicketStatus::Next));
        assert_eq!(metadata.priority, Some(TicketPriority::P1));
    }

    #[test]
    fn test_yaml_with_empty_arrays() {
        let content = r#"---
id: test-9012
status: new
deps:
links:
created: 2024-01-01T00:00:00Z
type: feature
priority: 0
---
# Empty Arrays Test

Both deps and links should be empty vectors.
"#;

        let metadata = parse_ticket_content(content).unwrap();
        assert_eq!(metadata.id, Some("test-9012".to_string()));
        assert!(metadata.deps.is_empty());
        assert!(metadata.links.is_empty());
    }

    #[test]
    fn test_parse_with_crlf_line_endings() {
        let content = "---\r\n\
id: test-crlf\r\n\
status: new\r\n\
deps: []\r\n\
links: []\r\n\
created: 2024-01-01T00:00:00Z\r\n\
type: task\r\n\
priority: 2\r\n\
---\r\n\
# CRLF Ticket\r\n\
\r\n\
This ticket uses Windows-style line endings.\r\n\
";

        let metadata = parse_ticket_content(content).unwrap();
        assert_eq!(metadata.id, Some("test-crlf".to_string()));
        assert_eq!(metadata.status, Some(TicketStatus::New));
        assert_eq!(metadata.title, Some("CRLF Ticket".to_string()));
        assert_eq!(metadata.ticket_type, Some(TicketType::Task));
        assert_eq!(metadata.priority, Some(TicketPriority::P2));
    }

    #[test]
    fn test_parse_with_crlf_completion_summary() {
        let content = "---\r\n\
id: j-a1b2\r\n\
status: complete\r\n\
deps: []\r\n\
links: []\r\n\
created: 2024-01-01T00:00:00Z\r\n\
type: task\r\n\
---\r\n\
# CRLF Summary Test\r\n\
\r\n\
Description.\r\n\
\r\n\
## Completion Summary\r\n\
\r\n\
Task completed with CRLF line endings.\r\n\
";

        let metadata = parse_ticket_content(content).unwrap();
        assert_eq!(metadata.id, Some("j-a1b2".to_string()));
        assert_eq!(metadata.status, Some(TicketStatus::Complete));
        let summary = metadata.completion_summary.unwrap();
        assert_eq!(summary, "Task completed with CRLF line endings.");
    }

    #[test]
    fn test_parse_with_mixed_line_endings() {
        let content = "---\n\
id: test-mixed\n\
status: new\n\
deps: []\r\n\
links: []\r\n\
created: 2024-01-01T00:00:00Z\n\
type: task\r\n\
priority: 2\r\n\
---\n\
# Mixed Line Endings\r\n\
\r\n\
This document has mixed line endings.\r\n\
";

        let metadata = parse_ticket_content(content).unwrap();
        assert_eq!(metadata.id, Some("test-mixed".to_string()));
        assert_eq!(metadata.status, Some(TicketStatus::New));
        assert_eq!(metadata.title, Some("Mixed Line Endings".to_string()));
    }
}
