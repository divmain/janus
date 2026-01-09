use regex::Regex;

use crate::error::{JanusError, Result};
use crate::types::{TicketMetadata, TicketPriority, TicketStatus, TicketType};

/// Parse a ticket file's content into TicketMetadata
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
pub fn parse_ticket_content(content: &str) -> Result<TicketMetadata> {
    // Match frontmatter: ---\n...\n---\n...
    let frontmatter_re = Regex::new(r"(?s)^---\n(.*?)\n---\n(.*)$").unwrap();

    let captures = frontmatter_re
        .captures(content)
        .ok_or_else(|| JanusError::InvalidFormat("missing YAML frontmatter".to_string()))?;

    let yaml = captures.get(1).map(|m| m.as_str()).unwrap_or("");
    let body = captures.get(2).map(|m| m.as_str()).unwrap_or("");

    let mut metadata = TicketMetadata::default();

    // Parse YAML line by line (matching TypeScript behavior)
    let line_re = Regex::new(r"^(\w[-\w]*):\s*(.*)$").unwrap();

    for line in yaml.lines() {
        if let Some(caps) = line_re.captures(line) {
            let key = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let value = caps.get(2).map(|m| m.as_str()).unwrap_or("");

            match key {
                "id" => metadata.id = Some(value.to_string()),
                "uuid" => metadata.uuid = Some(value.to_string()),
                "status" => {
                    metadata.status = value.parse::<TicketStatus>().ok();
                }
                "deps" => {
                    metadata.deps = parse_json_array(value);
                }
                "links" => {
                    metadata.links = parse_json_array(value);
                }
                "created" => metadata.created = Some(value.to_string()),
                "type" => {
                    metadata.ticket_type = value.parse::<TicketType>().ok();
                }
                "priority" => {
                    metadata.priority = value.parse::<TicketPriority>().ok();
                }
                "external-ref" => metadata.external_ref = Some(value.to_string()),
                "remote" => metadata.remote = Some(value.to_string()),
                "parent" => metadata.parent = Some(value.to_string()),
                _ => {} // Ignore unknown fields
            }
        }
    }

    // Extract title from body (first # heading)
    let title_re = Regex::new(r"(?m)^#\s+(.*)$").unwrap();
    if let Some(caps) = title_re.captures(body) {
        metadata.title = caps.get(1).map(|m| m.as_str().to_string());
    }

    // Extract completion summary from body (## Completion Summary section)
    metadata.completion_summary = extract_completion_summary(body);

    Ok(metadata)
}

/// Extract the content of the `## Completion Summary` section from a ticket body.
///
/// The section content includes everything after the `## Completion Summary` header
/// until the next H2 header (`## ...`) or end of document.
fn extract_completion_summary(body: &str) -> Option<String> {
    // Match "## Completion Summary" (case-insensitive) and capture everything until next ## or end
    let section_re = Regex::new(r"(?ims)^##\s+completion\s+summary\s*\n(.*?)(?:^##\s|\z)").unwrap();

    section_re.captures(body).map(|caps| {
        caps.get(1)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default()
    })
}

/// Parse a JSON array string like `["a", "b"]` into a Vec<String>
fn parse_json_array(value: &str) -> Vec<String> {
    if value.starts_with('[') && value.ends_with(']') {
        serde_json::from_str(value).unwrap_or_default()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_extract_completion_summary_empty() {
        let body = "# Title\n\nNo summary here.";
        assert!(extract_completion_summary(body).is_none());
    }

    #[test]
    fn test_extract_completion_summary_at_end() {
        let body = r#"# Title

Description.

## Completion Summary

Final summary content.
"#;

        let summary = extract_completion_summary(body).unwrap();
        assert_eq!(summary, "Final summary content.");
    }
}
