use crate::error::{JanusError, Result};
use crate::parser::parse_ticket_content;
use crate::types::{IMMUTABLE_TICKET_FIELDS, TicketMetadata, VALID_TICKET_FIELDS};

use regex::Regex;

pub struct TicketContent;

impl TicketContent {
    pub fn parse(raw_content: &str) -> Result<TicketMetadata> {
        parse_ticket_content(raw_content)
    }

    pub fn update_field(raw_content: &str, field: &str, value: &str) -> Result<String> {
        let frontmatter_re = Regex::new(r"(?s)^---\n(.*?)\n---\n(.*)$").unwrap();

        let captures = frontmatter_re.captures(raw_content).ok_or_else(|| {
            JanusError::InvalidFormat("missing or malformed YAML frontmatter".to_string())
        })?;

        let yaml = captures.get(1).map(|m| m.as_str()).unwrap_or("");
        let body = captures.get(2).map(|m| m.as_str()).unwrap_or("");

        let mut yaml_lines: Vec<String> = yaml.lines().map(String::from).collect();
        let line_re = Regex::new(&format!(r"^{}:\s*.*$", regex::escape(field))).unwrap();

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

    pub fn remove_field(raw_content: &str, field: &str) -> Result<String> {
        let field_pattern =
            Regex::new(&format!(r"(?m)^{}:\s*.*\n?", regex::escape(field))).unwrap();
        Ok(field_pattern.replace(raw_content, "").into_owned())
    }

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

    pub fn update_title(raw_content: &str, new_title: &str) -> String {
        let title_re = Regex::new(r"(?m)^#\s+.*$").unwrap();
        title_re
            .replace(raw_content, format!("# {}", new_title))
            .into_owned()
    }
}

pub fn extract_body(raw_content: &str) -> String {
    TicketContent::extract_body(raw_content)
}

pub fn update_title(raw_content: &str, new_title: &str) -> String {
    TicketContent::update_title(raw_content, new_title)
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

    #[test]
    fn test_update_field_existing_field() {
        let content = r#"---
id: test-1234
status: new
priority: 2
---
# Test Ticket"#;

        let result = TicketContent::update_field(content, "status", "complete").unwrap();
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

        let result = TicketContent::update_field(content, "priority", "3").unwrap();
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

        let result = TicketContent::update_field(content, "status", "in_progress").unwrap();

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

        let result = TicketContent::update_field(content, "priority", "1").unwrap();

        assert!(result.contains("id: test-1234"));
        assert!(result.contains("status: new"));
        assert!(result.contains("priority: 1"));
        assert!(result.contains("--- multiple dashes ---"));
    }

    #[test]
    fn test_update_field_malformed_frontmatter() {
        let content = "No frontmatter here\n# Just content";
        let result = TicketContent::update_field(content, "status", "complete");
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
