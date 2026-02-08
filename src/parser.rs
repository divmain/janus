use std::collections::HashMap;
use std::sync::LazyLock;

use comrak::nodes::NodeValue;
use comrak::{Arena, Options};
use regex::Regex;
use serde::de::DeserializeOwned;
use serde_yaml_ng as yaml;

use crate::error::{JanusError, Result};

pub static TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^#\s+(.*)$").expect("title regex should be valid"));

/// Core frontmatter parsing using comrak's AST-based approach.
///
/// This module provides robust YAML frontmatter extraction that handles
/// edge cases that would break regex-based parsers:
///
/// - YAML comments containing "---"
/// - Multi-line strings with embedded delimiters
/// - Block scalars with "---" patterns
/// - Unicode and special characters
///
/// The parser uses comrak's markdown AST to identify frontmatter boundaries
/// before any YAML parsing occurs, ensuring accurate delimiter detection.
///
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

fn strip_delimiters(fm: &str) -> Option<&str> {
    let delimiter = "---\n";
    let closing_delimiter = "\n---\n";

    let after_first = fm.strip_prefix(delimiter)?;

    if let Some(second_pos) = after_first.find(closing_delimiter) {
        let frontmatter = &after_first[..second_pos];
        Some(frontmatter)
    } else {
        None
    }
}

/// Split content into YAML frontmatter and markdown body using comrak.
///
/// This function uses comrak's AST-based markdown parsing to robustly extract
/// frontmatter. Unlike regex-based approaches, this correctly handles edge cases
/// like YAML comments containing "---", multi-line strings with "---", and
/// block scalars. Comrak parses the markdown structure first, then extracts
/// the frontmatter as a raw string, ensuring accurate delimiter detection.
///
/// # Arguments
/// * `content` - The full markdown document content
///
/// # Returns
/// A tuple of `(frontmatter, body)` where:
/// - `frontmatter` is the raw YAML string (without the --- delimiters)
/// - `body` is everything after the closing ---
///
/// # Errors
/// Returns `JanusError::InvalidFormat` if no frontmatter is found
pub fn split_frontmatter_comrak(content: &str) -> Result<(String, String)> {
    let mut options = Options::default();
    options.extension.front_matter_delimiter = Some("---".to_string());

    let arena = Arena::new();
    let root = comrak::parse_document(&arena, content, &options);

    for node in root.children() {
        if let NodeValue::FrontMatter(fm) = &node.data.borrow().value
            && let Some(frontmatter) = strip_delimiters(fm)
        {
            let body_start = fm.len();

            let body = if body_start <= content.len() {
                content[body_start..].to_string()
            } else {
                String::new()
            };
            return Ok((frontmatter.to_string(), body));
        }
    }

    if content == "---\n---\n" || content.starts_with("---\n---\n") {
        return Ok((String::new(), content[8..].trim_start().to_string()));
    }

    Err(JanusError::InvalidFormat(
        "missing YAML frontmatter".to_string(),
    ))
}

/// Split content into YAML frontmatter and markdown body.
///
/// Handles CRLF and CR line endings by normalizing to LF. Returns a tuple of
/// (frontmatter_content, body_content) or an error if frontmatter is missing.
///
/// This function now uses comrak's robust AST-based parsing instead of regex,
/// correctly handling edge cases like "---" in YAML comments or multi-line strings.
pub fn split_frontmatter(content: &str) -> Result<(String, String)> {
    let normalized = content.replace("\r\n", "\n").replace("\r", "\n");
    split_frontmatter_comrak(&normalized)
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

    /// Update or add a section in the document body.
    ///
    /// If the section exists, its content is replaced. If it doesn't exist,
    /// the section is appended to the end of the body.
    ///
    /// # Arguments
    /// * `section_name` - The name of the section to update (case-insensitive)
    /// * `section_content` - The new content for the section (without the header)
    ///
    /// # Returns
    /// The full updated document content (frontmatter + body)
    pub fn update_section(&self, section_name: &str, section_content: &str) -> String {
        let pattern = format!(
            r"(?ims)^##\s+{}\s*\n(.*?)(?:^##\s|\z)",
            regex::escape(section_name)
        );
        let section_re = Regex::new(&pattern).expect("section regex should be valid");

        if let Some(caps) = section_re.captures(&self.body) {
            // Section exists - replace its content
            let full_match = caps.get(0).expect("full match should exist");
            let content_match = caps.get(1).expect("content group should exist");

            let before = &self.body[..full_match.start()];
            let after = &self.body[content_match.end()..];

            // Build the new section
            let new_section = format!("## {section_name}\n\n{section_content}");

            // Handle spacing after the section
            let after_trimmed = after.trim_start_matches('\n');
            let separator = if after_trimmed.is_empty() {
                "\n".to_string()
            } else {
                format!("\n\n{after_trimmed}")
            };

            format!("{before}{new_section}{separator}")
        } else {
            // Section doesn't exist - append it
            let trimmed_body = self.body.trim_end();
            format!("{trimmed_body}\n\n## {section_name}\n\n{section_content}\n")
        }
    }

    /// Deserialize the frontmatter into a specific type.
    ///
    /// This uses the raw YAML string for proper type conversion via serde.
    pub fn deserialize_frontmatter<T: DeserializeOwned>(&self) -> Result<T> {
        yaml::from_str(&self.frontmatter_raw)
            .map_err(|e| JanusError::Other(format!("YAML parsing error: {e}")))
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
        .map_err(|e| JanusError::Other(format!("YAML parsing error: {e}")))?;

    Ok(ParsedDocument {
        frontmatter_raw,
        frontmatter,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

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
    fn test_parsed_document_update_section_existing() {
        let content = r#"---
id: test
---
# Title

## My Section

Old content here.

## Another Section

More content.
"#;
        let doc = parse_document(content).unwrap();
        let updated = doc.update_section("My Section", "New content here.");

        assert!(updated.contains("## My Section\n\nNew content here."));
        assert!(!updated.contains("Old content here."));
        assert!(updated.contains("## Another Section"));
    }

    #[test]
    fn test_parsed_document_update_section_at_end() {
        let content = r#"---
id: test
---
# Title

## Final Section

Old final content.
"#;
        let doc = parse_document(content).unwrap();
        let updated = doc.update_section("Final Section", "Updated final content.");

        assert!(updated.contains("## Final Section\n\nUpdated final content."));
        assert!(!updated.contains("Old final content."));
    }

    #[test]
    fn test_parsed_document_update_section_add_new() {
        let content = r#"---
id: test
---
# Title

Body content.
"#;
        let doc = parse_document(content).unwrap();
        let updated = doc.update_section("New Section", "New section content.");

        assert!(updated.contains("## New Section\n\nNew section content."));
        assert!(updated.contains("Body content."));
    }

    #[test]
    fn test_parsed_document_update_section_case_insensitive() {
        let content = r#"---
id: test
---
# Title

## UPPERCASE SECTION

Content.
"#;
        let doc = parse_document(content).unwrap();
        let updated = doc.update_section("uppercase section", "Updated content.");

        assert!(updated.contains("## uppercase section\n\nUpdated content."));
    }

    #[test]
    fn test_parsed_document_update_section_preserves_other_sections() {
        let content = r#"---
id: test
---
# Title

## First Section

First content.

## Middle Section

Middle content.

## Last Section

Last content.
"#;
        let doc = parse_document(content).unwrap();
        let updated = doc.update_section("Middle Section", "Updated middle.");

        assert!(updated.contains("## First Section"));
        assert!(updated.contains("First content."));
        assert!(updated.contains("## Middle Section\n\nUpdated middle."));
        assert!(!updated.contains("Middle content."));
        assert!(updated.contains("## Last Section"));
        assert!(updated.contains("Last content."));
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

    #[test]
    #[serial]
    fn test_frontmatter_with_yaml_comments_containing_dashes() {
        let content = r#"---
# This is a YAML comment with dashes ---
id: test-123
# Another comment --- with dashes ---
status: new
---
# Title

Body content.
"#;
        let (frontmatter, body) = split_frontmatter(content).unwrap();

        assert!(frontmatter.contains("id: test-123"));
        assert!(frontmatter.contains("status: new"));
        assert!(body.contains("# Title"));
        assert!(body.contains("Body content"));
    }

    #[test]
    #[serial]
    fn test_frontmatter_with_multiline_string_containing_dashes() {
        let content = r#"---
id: test
description: |-
  This is a multi-line
  string that contains --- dashes
  and should not break parsing
---
# Title

Body.
"#;
        let (frontmatter, body) = split_frontmatter(content).unwrap();

        assert!(frontmatter.contains("id: test"));
        assert!(frontmatter.contains("description"));
        assert!(body.contains("# Title"));
    }

    #[test]
    #[serial]
    fn test_frontmatter_with_block_scalar_containing_dashes() {
        let content = r#"---
id: test
notes: |
  Some notes with
  --- problematic pattern ---
  inside the block
---
# Title

Body.
"#;
        let (frontmatter, body) = split_frontmatter(content).unwrap();

        assert!(frontmatter.contains("id: test"));
        assert!(frontmatter.contains("notes:"));
        assert!(body.contains("# Title"));
    }

    #[test]
    #[serial]
    fn test_empty_frontmatter_and_body() {
        let content = "---\n---\n";
        let (frontmatter, body) = split_frontmatter(content).unwrap();

        assert_eq!(frontmatter, "");
        assert_eq!(body, "");
    }

    #[test]
    #[serial]
    fn test_body_preserves_dashes_pattern() {
        let content = r#"---
id: test
---
# Title

This body has --- multiple dashes ---
and they should be preserved.
"#;
        let (frontmatter, body) = split_frontmatter(content).unwrap();

        assert_eq!(frontmatter, "id: test");
        assert!(body.contains("--- multiple dashes ---"));
    }

    #[test]
    #[serial]
    fn test_complex_yaml_with_nested_structures() {
        let content = r#"---
id: test
metadata:
  author: someone
  tags:
    - tag1
    - tag2
  description: |
    Multi-line description
    with --- dashes
nested:
  level1:
    level2: value
---
# Title

Body.
"#;
        let (frontmatter, body) = split_frontmatter(content).unwrap();

        assert!(frontmatter.contains("id: test"));
        assert!(frontmatter.contains("metadata:"));
        assert!(frontmatter.contains("tags:"));
        assert!(frontmatter.contains("nested:"));
        assert!(body.contains("# Title"));
    }

    #[test]
    #[serial]
    fn test_frontmatter_with_unicode() {
        let content = "---\nid: test-日本語\ntitle: 标题\n---\n# Title\n";
        let (frontmatter, _body) = split_frontmatter(content).unwrap();

        assert!(frontmatter.contains("id: test-日本語"));
        assert!(frontmatter.contains("title: 标题"));
    }
}
