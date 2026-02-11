use serde::Deserialize;

use crate::error::Result;
use crate::parser::parse_document_raw;
use crate::types::{
    CreatedAt, TicketId, TicketMetadata, TicketPriority, TicketSize, TicketStatus, TicketType,
};

/// Strict frontmatter struct for YAML deserialization with required fields.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TicketFrontmatter {
    id: String,
    uuid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<TicketStatus>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    deps: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    ticket_type: Option<TicketType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<TicketPriority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<TicketSize>,
    #[serde(rename = "external-ref", skip_serializing_if = "Option::is_none")]
    external_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<TicketId>,
    #[serde(rename = "spawned-from", skip_serializing_if = "Option::is_none")]
    spawned_from: Option<TicketId>,
    #[serde(rename = "spawn-context", skip_serializing_if = "Option::is_none")]
    spawn_context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    depth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    triaged: Option<bool>,
}

/// Parse a ticket file's content into TicketMetadata.
///
/// This is the main entry point for ticket parsing. It parses the document
/// and converts it to TicketMetadata, extracting both frontmatter fields
/// and body-derived fields (title, completion summary).
pub fn parse(content: &str) -> Result<TicketMetadata> {
    let (frontmatter_raw, body) = parse_document_raw(content)?;
    ticket_metadata_from_document(&frontmatter_raw, &body)
}

/// Convert parsed document parts to TicketMetadata.
///
/// This handles the ticket-specific conversion logic, including:
/// - Deserializing frontmatter into strict TicketFrontmatter (validates required fields at parse time)
/// - Mapping strict frontmatter to lenient TicketMetadata
/// - Extracting title from the first H1 heading
/// - Extracting completion summary from the `## Completion Summary` section
fn ticket_metadata_from_document(frontmatter_raw: &str, body: &str) -> Result<TicketMetadata> {
    let frontmatter: TicketFrontmatter = serde_yaml_ng::from_str(frontmatter_raw)?;

    let metadata = TicketMetadata {
        id: Some(TicketId::new_unchecked(frontmatter.id)),
        uuid: Some(frontmatter.uuid),
        status: frontmatter.status,
        deps: frontmatter.deps,
        links: frontmatter.links,
        created: frontmatter.created.map(CreatedAt::new_unchecked),
        ticket_type: frontmatter.ticket_type,
        priority: frontmatter.priority,
        size: frontmatter.size,
        external_ref: frontmatter.external_ref,
        remote: frontmatter.remote,
        parent: frontmatter.parent,
        spawned_from: frontmatter.spawned_from,
        spawn_context: frontmatter.spawn_context,
        depth: frontmatter.depth,
        triaged: frontmatter.triaged,
        title: extract_title(body),
        completion_summary: extract_section(body, "completion summary")?,
        file_path: None,
        body: None,
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

// Re-export the shared section extraction function from parser module
pub use crate::parser::extract_section_from_body as extract_section;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TicketPriority, TicketStatus, TicketType};

    #[test]
    fn test_parse_basic_ticket() {
        let content = r#"---
id: test-1234
uuid: 550e8400-e29b-41d4-a716-446655440000
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

        let metadata = parse(content).unwrap();
        assert_eq!(metadata.id.as_deref(), Some("test-1234"));
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
uuid: 550e8400-e29b-41d4-a716-446655440001
status: new
deps: ["dep-1", "dep-2"]
links: ["link-1"]
---
# Another Ticket
"#;

        let metadata = parse(content).unwrap();
        assert_eq!(metadata.deps, vec!["dep-1", "dep-2"]);
        assert_eq!(metadata.links, vec!["link-1"]);
    }

    #[test]
    fn test_parse_missing_frontmatter() {
        let content = "# No frontmatter\n\nJust content.";
        let result = parse(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_with_completion_summary() {
        let content = r#"---
id: j-a1b2
uuid: 550e8400-e29b-41d4-a716-446655440002
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

        let metadata = parse(content).unwrap();
        assert_eq!(metadata.id.as_deref(), Some("j-a1b2"));
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
uuid: 550e8400-e29b-41d4-a716-446655440003
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

        let metadata = parse(content).unwrap();
        let summary = metadata.completion_summary.unwrap();
        assert_eq!(summary, "This task is done.");
        assert!(!summary.contains("Notes"));
        assert!(!summary.contains("additional notes"));
    }

    #[test]
    fn test_parse_no_completion_summary() {
        let content = r#"---
id: j-e5f6
uuid: 550e8400-e29b-41d4-a716-446655440004
status: new
deps: []
links: []
---
# Task Without Summary

Just a description, no completion summary section.
"#;

        let metadata = parse(content).unwrap();
        assert!(metadata.completion_summary.is_none());
    }

    #[test]
    fn test_parse_completion_summary_case_insensitive() {
        let content = r#"---
id: j-g7h8
uuid: 550e8400-e29b-41d4-a716-446655440005
status: complete
deps: []
links: []
---
# Task Title

## COMPLETION SUMMARY

All caps header should work.
"#;

        let metadata = parse(content).unwrap();
        let summary = metadata.completion_summary.unwrap();
        assert_eq!(summary, "All caps header should work.");
    }

    #[test]
    fn test_parse_yaml_with_multiline_string() {
        let content = r#"---
id: test-1234
uuid: 550e8400-e29b-41d4-a716-446655440006
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

        let metadata = parse(content).unwrap();
        assert_eq!(metadata.id.as_deref(), Some("test-1234"));
        assert!(metadata.external_ref.is_some());
        let ref_str = metadata.external_ref.unwrap();
        assert!(ref_str.contains("multi-line"));
        assert!(ref_str.contains("scalar syntax"));
    }

    #[test]
    fn test_parse_yaml_with_comments() {
        let content = r#"---
# This is a YAML comment that should be ignored
id: test-5678  # Inline comment
uuid: 550e8400-e29b-41d4-a716-446655440007
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

        let metadata = parse(content).unwrap();
        assert_eq!(metadata.id.as_deref(), Some("test-5678"));
        assert_eq!(metadata.status, Some(TicketStatus::Next));
        assert_eq!(metadata.priority, Some(TicketPriority::P1));
    }

    #[test]
    fn test_parse_yaml_with_empty_arrays() {
        let content = r#"---
id: test-9012
uuid: 550e8400-e29b-41d4-a716-446655440008
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

        let metadata = parse(content).unwrap();
        assert_eq!(metadata.id.as_deref(), Some("test-9012"));
        assert!(metadata.deps.is_empty());
        assert!(metadata.links.is_empty());
    }

    #[test]
    fn test_parse_with_crlf_line_endings() {
        let content = "---\r\n\
id: test-crlf\r\n\
uuid: 550e8400-e29b-41d4-a716-446655440009\r\n\
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

        let metadata = parse(content).unwrap();
        assert_eq!(metadata.id.as_deref(), Some("test-crlf"));
        assert_eq!(metadata.status, Some(TicketStatus::New));
        assert_eq!(metadata.title, Some("CRLF Ticket".to_string()));
        assert_eq!(metadata.ticket_type, Some(TicketType::Task));
        assert_eq!(metadata.priority, Some(TicketPriority::P2));
    }

    #[test]
    fn test_parse_with_crlf_completion_summary() {
        let content = "---\r\n\
id: j-a1b2\r\n\
uuid: 550e8400-e29b-41d4-a716-446655440010\r\n\
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

        let metadata = parse(content).unwrap();
        assert_eq!(metadata.id.as_deref(), Some("j-a1b2"));
        assert_eq!(metadata.status, Some(TicketStatus::Complete));
        let summary = metadata.completion_summary.unwrap();
        assert_eq!(summary, "Task completed with CRLF line endings.");
    }

    #[test]
    fn test_parse_with_mixed_line_endings() {
        let content = "---\n\
id: test-mixed\n\
uuid: 550e8400-e29b-41d4-a716-446655440011\n\
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

        let metadata = parse(content).unwrap();
        assert_eq!(metadata.id.as_deref(), Some("test-mixed"));
        assert_eq!(metadata.status, Some(TicketStatus::New));
        assert_eq!(metadata.title, Some("Mixed Line Endings".to_string()));
        assert_eq!(metadata.ticket_type, Some(TicketType::Task));
        assert_eq!(metadata.priority, Some(TicketPriority::P2));
    }

    #[test]
    fn test_parse_missing_required_id_field() {
        let content = r#"---
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
---
# Test Ticket
"#;

        let result = parse(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_required_uuid_field() {
        let content = r#"---
id: test-1234
status: new
---
# Test Ticket
"#;

        let result = parse(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_unknown_field_rejected() {
        let content = r#"---
id: test-1234
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
unknown_field: should_be_rejected
---
# Test Ticket
"#;

        let result = parse(content);
        assert!(result.is_err());
    }
}
