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
                "assignee" => metadata.assignee = Some(value.to_string()),
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

    Ok(metadata)
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
assignee: John Doe
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
        assert_eq!(metadata.assignee, Some("John Doe".to_string()));
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
}
