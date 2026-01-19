use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::de::DeserializeOwned;
use serde_yaml_ng as yaml;

use crate::error::{JanusError, Result};

pub(crate) static FRONTMATTER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)^---\n(.*?)\n---\n(.*)$").expect("frontmatter regex should be valid")
});

pub static TITLE_RE: LazyLock<Regex> =
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
/// This function returns a generic structure that can be used to extract
/// frontmatter fields and body content. For ticket-specific parsing,
/// use `crate::ticket::parse_ticket` instead.
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

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Generic Document Parsing Tests ====================

    #[test]
    fn test_split_frontmatter() {
        let content = "---\nid: test\nstatus: new\n---\n# Title\n\nBody";
        let (yaml, body) = split_frontmatter(content).unwrap();
        assert_eq!(yaml, "id: test\nstatus: new");
        assert_eq!(body, "# Title\n\nBody");
    }

    #[test]
    fn test_split_frontmatter_missing() {
        let content = "# No frontmatter\n\nJust content.";
        let result = split_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_split_frontmatter_crlf() {
        let content = "---\r\nid: test\r\n---\r\n# Title";
        let (yaml, body) = split_frontmatter(content).unwrap();
        assert_eq!(yaml, "id: test");
        assert_eq!(body, "# Title");
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
    fn test_parsed_document_extract_title_missing() {
        let content = r#"---
id: test
---
No H1 heading in this document.
"#;
        let doc = parse_document(content).unwrap();
        assert!(doc.extract_title().is_none());
    }

    #[test]
    fn test_parsed_document_extract_section() {
        let content = r#"---
id: test
---
# Title

Description.

## My Section

Section content here.

## Another Section

More content.
"#;
        let doc = parse_document(content).unwrap();
        let section = doc.extract_section("my section").unwrap();
        assert_eq!(section, "Section content here.");
    }

    #[test]
    fn test_parsed_document_extract_section_at_end() {
        let content = r#"---
id: test
---
# Title

Description.

## Final Section

Final content.
"#;
        let doc = parse_document(content).unwrap();
        let section = doc.extract_section("final section").unwrap();
        assert_eq!(section, "Final content.");
    }

    #[test]
    fn test_parsed_document_extract_section_missing() {
        let content = r#"---
id: test
---
# Title

No sections here."#;
        let doc = parse_document(content).unwrap();
        assert!(doc.extract_section("nonexistent").is_none());
    }

    #[test]
    fn test_parsed_document_extract_section_case_insensitive() {
        let content = r#"---
id: test
---
# Title

## UPPERCASE SECTION

Content.
"#;
        let doc = parse_document(content).unwrap();
        assert!(doc.extract_section("uppercase section").is_some());
        assert!(doc.extract_section("UPPERCASE SECTION").is_some());
    }

    #[test]
    fn test_parsed_document_deserialize_frontmatter() {
        #[derive(serde::Deserialize)]
        struct TestMeta {
            id: String,
            count: i32,
        }

        let content = r#"---
id: test-123
count: 42
---
# Title
"#;
        let doc = parse_document(content).unwrap();
        let meta: TestMeta = doc.deserialize_frontmatter().unwrap();
        assert_eq!(meta.id, "test-123");
        assert_eq!(meta.count, 42);
    }

    #[test]
    fn test_parse_document_with_yaml_comments() {
        let content = r#"---
# This is a YAML comment
id: test-123  # Inline comment
status: active
---
# Title
"#;
        let doc = parse_document(content).unwrap();
        assert!(doc.frontmatter.contains_key("id"));
        assert!(doc.frontmatter.contains_key("status"));
    }

    #[test]
    fn test_parse_document_with_multiline_yaml() {
        let content = r#"---
id: test
description: |
  This is a multi-line
  string in YAML
---
# Title
"#;
        let doc = parse_document(content).unwrap();
        assert!(doc.frontmatter.contains_key("description"));
    }

    #[test]
    fn test_parse_document_with_mixed_line_endings() {
        let content = "---\n\
id: test\r\n\
status: active\n\
---\r\n\
# Title\r\n\
\r\n\
Body with mixed endings.\n\
";
        let doc = parse_document(content).unwrap();
        assert!(doc.frontmatter.contains_key("id"));
        assert!(doc.body.contains("Title"));
    }

    #[test]
    fn test_frontmatter_regex_with_dashes_in_body() {
        let content = r#"---
id: test
---
# Title

Body with --- multiple dashes --- here.
"#;
        let doc = parse_document(content).unwrap();
        assert!(doc.body.contains("--- multiple dashes ---"));
    }
}
