use regex::Regex;

use crate::error::{JanusError, Result};
use crate::parser::{FRONTMATTER_RE, ParsedDocument, TITLE_RE, parse_document};
use crate::types::{IMMUTABLE_TICKET_FIELDS, TicketMetadata, VALID_TICKET_FIELDS};

/// Parse a ticket file's content into TicketMetadata.
///
/// This is the main entry point for ticket parsing. It parses the document
/// and converts it to TicketMetadata, extracting both frontmatter fields
/// and body-derived fields (title, completion summary).
pub fn parse(content: &str) -> Result<TicketMetadata> {
    let doc = parse_document(content)?;
    ticket_metadata_from_document(doc)
}

/// Convert a ParsedDocument to TicketMetadata.
///
/// This handles the ticket-specific conversion logic, including:
/// - Deserializing frontmatter into TicketMetadata fields
/// - Extracting title from the first H1 heading
/// - Extracting completion summary from the `## Completion Summary` section
fn ticket_metadata_from_document(doc: ParsedDocument) -> Result<TicketMetadata> {
    // Deserialize frontmatter using the raw YAML string for proper type handling
    let mut metadata: TicketMetadata = doc.deserialize_frontmatter()?;

    // Extract body-derived fields
    metadata.title = doc.extract_title();
    metadata.completion_summary = doc.extract_section("completion summary");

    Ok(metadata)
}

/// Update a field in the YAML frontmatter of a ticket file.
///
/// If the field exists, it will be updated in place. If it doesn't exist, it will be inserted
/// after the first line (typically the `id` field).
pub fn update_field(raw_content: &str, field: &str, value: &str) -> Result<String> {
    let captures = FRONTMATTER_RE.captures(raw_content).ok_or_else(|| {
        JanusError::InvalidFormat("missing or malformed YAML frontmatter".to_string())
    })?;

    let yaml = captures.get(1).map(|m| m.as_str()).unwrap_or("");
    let body = captures.get(2).map(|m| m.as_str()).unwrap_or("");

    let mut yaml_lines: Vec<String> = yaml.lines().map(String::from).collect();
    let line_re = Regex::new(&format!(r"^{}:\s*.*$", regex::escape(field)))
        .expect("field regex should be valid");

    let field_exists = yaml_lines.iter().any(|line| line_re.is_match(line));

    if field_exists {
        for line in &mut yaml_lines {
            if line_re.is_match(line) {
                *line = format!("{}: {}", field, value);
                break;
            }
        }
    } else {
        yaml_lines.insert(1, format!("{}: {}", field, value));
    }

    let new_frontmatter = yaml_lines.join("\n");
    Ok(format!("---\n{}\n---\n{}", new_frontmatter, body))
}

/// Remove a field from the YAML frontmatter of a ticket file.
pub fn remove_field(raw_content: &str, field: &str) -> Result<String> {
    let field_pattern = Regex::new(&format!(r"(?m)^{}:\s*.*\n?", regex::escape(field)))
        .expect("field pattern regex should be valid");
    Ok(field_pattern.replace(raw_content, "").into_owned())
}

/// Extract the body content from a ticket file (everything after the title).
pub fn extract_body(raw_content: &str) -> String {
    if let Some(end_idx) = raw_content.find("\n---\n") {
        let after_frontmatter = &raw_content[end_idx + 5..];
        let lines: Vec<&str> = after_frontmatter.lines().collect();
        let body_start = lines
            .iter()
            .position(|l| !l.starts_with('#') && !l.is_empty())
            .unwrap_or(0);
        lines[body_start..].join("\n").trim().to_string()
    } else {
        String::new()
    }
}

/// Extract the value of a field from the YAML frontmatter of a ticket file.
pub fn extract_field_value(raw_content: &str, field: &str) -> Option<String> {
    let field_pattern = Regex::new(&format!(r"(?m)^{}:\s*.*$", regex::escape(field)))
        .expect("field pattern regex should be valid");
    field_pattern.find(raw_content).map(|m| {
        m.as_str()
            .split(':')
            .nth(1)
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    })
}

/// Update the title (H1 heading) in a ticket file.
pub fn update_title(raw_content: &str, new_title: &str) -> String {
    TITLE_RE
        .replace(raw_content, format!("# {}", new_title))
        .into_owned()
}

pub(crate) fn validate_field_name(field: &str, operation: &str) -> Result<()> {
    if !VALID_TICKET_FIELDS.contains(&field) {
        return Err(JanusError::InvalidField {
            field: field.to_string(),
            valid_fields: VALID_TICKET_FIELDS.iter().map(|s| s.to_string()).collect(),
        });
    }

    if IMMUTABLE_TICKET_FIELDS.contains(&field) {
        return Err(JanusError::Other(format!(
            "cannot {} immutable field '{}'",
            operation, field
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TicketPriority, TicketStatus, TicketType};

    // ==================== Ticket Parsing Tests ====================

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

        let metadata = parse(content).unwrap();
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

        let metadata = parse(content).unwrap();
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

        let metadata = parse(content).unwrap();
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

        let metadata = parse(content).unwrap();
        let summary = metadata.completion_summary.unwrap();
        assert_eq!(summary, "All caps header should work.");
    }

    #[test]
    fn test_parse_yaml_with_multiline_string() {
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

        let metadata = parse(content).unwrap();
        assert_eq!(metadata.id, Some("test-1234".to_string()));
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
        assert_eq!(metadata.id, Some("test-5678".to_string()));
        assert_eq!(metadata.status, Some(TicketStatus::Next));
        assert_eq!(metadata.priority, Some(TicketPriority::P1));
    }

    #[test]
    fn test_parse_yaml_with_empty_arrays() {
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

        let metadata = parse(content).unwrap();
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

        let metadata = parse(content).unwrap();
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

        let metadata = parse(content).unwrap();
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

        let metadata = parse(content).unwrap();
        assert_eq!(metadata.id, Some("test-mixed".to_string()));
        assert_eq!(metadata.status, Some(TicketStatus::New));
        assert_eq!(metadata.title, Some("Mixed Line Endings".to_string()));
    }

    // ==================== Field Update Tests ====================

    #[test]
    fn test_update_field_existing_field() {
        let content = r#"---
id: test-1234
status: new
priority: 2
---
# Test Ticket"#;

        let result = update_field(content, "status", "complete").unwrap();
        assert!(result.contains("status: complete"));
        assert!(result.contains("id: test-1234"));
        assert!(result.contains("# Test Ticket"));
    }

    #[test]
    fn test_update_field_new_field() {
        let content = r#"---
id: test-1234
status: new
---
# Test Ticket"#;

        let result = update_field(content, "priority", "3").unwrap();
        assert!(result.contains("id: test-1234"));
        assert!(result.contains("status: new"));
        assert!(result.contains("priority: 3"));
        assert!(result.contains("# Test Ticket"));
    }

    #[test]
    fn test_update_field_preserves_frontmatter_structure() {
        let content = r#"---
id: test-1234
status: new
priority: 2
type: bug
---
# Test Ticket"#;

        let result = update_field(content, "status", "in_progress").unwrap();

        assert!(result.starts_with("---\n"));
        assert!(result.contains("\n---\n"));
        assert!(result.contains("id: test-1234"));
        assert!(result.contains("status: in_progress"));
        assert!(result.contains("priority: 2"));
        assert!(result.contains("type: bug"));
        assert!(result.contains("# Test Ticket"));
    }

    #[test]
    fn test_update_field_multiple_dashes_in_body() {
        let content = r#"---
id: test-1234
status: new
---
# Test Ticket

Body with --- multiple dashes ---
"#;

        let result = update_field(content, "priority", "1").unwrap();

        assert!(result.contains("id: test-1234"));
        assert!(result.contains("status: new"));
        assert!(result.contains("priority: 1"));
        assert!(result.contains("--- multiple dashes ---"));
    }

    #[test]
    fn test_update_field_malformed_frontmatter() {
        let content = "No frontmatter here\n# Just content";
        let result = update_field(content, "status", "complete");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::InvalidFormat(msg) => {
                assert!(msg.contains("missing or malformed"));
            }
            _ => panic!("Expected InvalidFormat error"),
        }
    }

    #[test]
    fn test_validate_field_name_valid() {
        assert!(validate_field_name("status", "update").is_ok());
        assert!(validate_field_name("priority", "update").is_ok());
        assert!(validate_field_name("type", "update").is_ok());
    }

    #[test]
    fn test_validate_field_name_invalid() {
        let result = validate_field_name("unknown_field", "update");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::InvalidField {
                field,
                valid_fields: _,
            } => {
                assert_eq!(field, "unknown_field");
            }
            _ => panic!("Expected InvalidField error"),
        }
    }

    #[test]
    fn test_validate_field_name_immutable_id() {
        let result = validate_field_name("id", "update");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::Other(msg) => {
                assert!(msg.contains("cannot update immutable field 'id'"));
            }
            _ => panic!("Expected Other error for immutable field"),
        }
    }

    #[test]
    fn test_validate_field_name_immutable_uuid() {
        let result = validate_field_name("uuid", "update");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::Other(msg) => {
                assert!(msg.contains("cannot update immutable field 'uuid'"));
            }
            _ => panic!("Expected Other error for immutable field"),
        }
    }

    #[test]
    fn test_validate_field_name_remove_immutable() {
        let result = validate_field_name("id", "remove");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::Other(msg) => {
                assert!(msg.contains("cannot remove immutable field 'id'"));
            }
            _ => panic!("Expected Other error for immutable field"),
        }
    }
}
