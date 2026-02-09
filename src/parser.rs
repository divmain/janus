use std::collections::HashMap;
use std::sync::LazyLock;

use comrak::nodes::NodeValue;
use comrak::{Arena, Options};
use dashmap::DashMap;
use regex::Regex;
use serde::de::DeserializeOwned;
use serde_yaml_ng as yaml;

use crate::error::{JanusError, Result};
/// Cache for compiled section regexes to avoid recompilation on every call.
/// The key is the regex pattern string, the value is the compiled Regex.
static SECTION_REGEX_CACHE: LazyLock<DashMap<String, Regex>> = LazyLock::new(DashMap::new);

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

        // Try to get from cache first, otherwise compile and cache
        let section_re = SECTION_REGEX_CACHE
            .get(&pattern)
            .map(|r| r.clone())
            .unwrap_or_else(|| {
                let re = Regex::new(&pattern).expect("section regex should be valid");
                SECTION_REGEX_CACHE.insert(pattern.clone(), re.clone());
                re
            });

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

        // Try to get from cache first, otherwise compile and cache
        let section_re = SECTION_REGEX_CACHE
            .get(&pattern)
            .map(|r| r.clone())
            .unwrap_or_else(|| {
                let re = Regex::new(&pattern).expect("section regex should be valid");
                SECTION_REGEX_CACHE.insert(pattern.clone(), re.clone());
                re
            });

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

    /// Remove a section from the document body (case-insensitive).
    ///
    /// Uses a deterministic line scanner to find the `## {section_name}` header
    /// and remove all lines from the header up to (but not including) the next
    /// H2 heading or end of document. Returns the updated body string.
    ///
    /// If the section does not exist, the body is returned unchanged.
    pub fn remove_section(&self, section_name: &str) -> String {
        let section_name_lower = section_name.to_lowercase();
        let lines: Vec<&str> = self.body.split('\n').collect();

        // Find the line index of the target section header
        let header_idx = lines.iter().position(|line| {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("## ") {
                rest.trim().to_lowercase() == section_name_lower
            } else {
                false
            }
        });

        let Some(header_idx) = header_idx else {
            // Section not found, return body unchanged
            return self.body.clone();
        };

        // Find the end of the section: next H2 heading or end of document
        let end_idx = lines[header_idx + 1..]
            .iter()
            .position(|line| {
                let trimmed = line.trim_start();
                trimmed.starts_with("## ")
            })
            .map(|rel| header_idx + 1 + rel)
            .unwrap_or(lines.len());

        // Build the result: lines before the section + lines after the section
        let mut result_lines: Vec<&str> = Vec::with_capacity(lines.len());
        result_lines.extend_from_slice(&lines[..header_idx]);
        result_lines.extend_from_slice(&lines[end_idx..]);

        // Clean up: collapse excessive blank lines at the join point
        let mut result = result_lines.join("\n");

        // Remove runs of more than 2 consecutive newlines (preserve paragraph breaks)
        while result.contains("\n\n\n") {
            result = result.replace("\n\n\n", "\n\n");
        }

        result
    }

    /// Deserialize the frontmatter into a specific type.
    ///
    /// This uses the raw YAML string for proper type conversion via serde.
    pub fn deserialize_frontmatter<T: DeserializeOwned>(&self) -> Result<T> {
        Ok(yaml::from_str(&self.frontmatter_raw)?)
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

    let frontmatter: HashMap<String, yaml::Value> = yaml::from_str(&frontmatter_raw)?;

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

    #[test]
    fn test_remove_section_middle() {
        let content = r#"---
id: test
---
# Title

## First Section

First content.

## Middle Section

Middle content.
More middle content.

## Last Section

Last content.
"#;
        let doc = parse_document(content).unwrap();
        let result = doc.remove_section("Middle Section");

        assert!(result.contains("## First Section"));
        assert!(result.contains("First content."));
        assert!(!result.contains("## Middle Section"));
        assert!(!result.contains("Middle content."));
        assert!(!result.contains("More middle content."));
        assert!(result.contains("## Last Section"));
        assert!(result.contains("Last content."));
    }

    #[test]
    fn test_remove_section_last() {
        let content = r#"---
id: test
---
# Title

## First Section

First content.

## Last Section

Last content.
"#;
        let doc = parse_document(content).unwrap();
        let result = doc.remove_section("Last Section");

        assert!(result.contains("## First Section"));
        assert!(result.contains("First content."));
        assert!(!result.contains("## Last Section"));
        assert!(!result.contains("Last content."));
    }

    #[test]
    fn test_remove_section_first() {
        let content = r#"---
id: test
---
# Title

## First Section

First content.

## Second Section

Second content.
"#;
        let doc = parse_document(content).unwrap();
        let result = doc.remove_section("First Section");

        assert!(!result.contains("## First Section"));
        assert!(!result.contains("First content."));
        assert!(result.contains("## Second Section"));
        assert!(result.contains("Second content."));
    }

    #[test]
    fn test_remove_section_nonexistent() {
        let content = r#"---
id: test
---
# Title

## Existing Section

Content here.
"#;
        let doc = parse_document(content).unwrap();
        let result = doc.remove_section("Nonexistent Section");

        assert!(result.contains("## Existing Section"));
        assert!(result.contains("Content here."));
        assert_eq!(result, doc.body);
    }

    #[test]
    fn test_remove_section_case_insensitive() {
        let content = r#"---
id: test
---
# Title

## UPPERCASE SECTION

Content to remove.

## Other Section

Keep this.
"#;
        let doc = parse_document(content).unwrap();
        let result = doc.remove_section("uppercase section");

        assert!(!result.contains("## UPPERCASE SECTION"));
        assert!(!result.contains("Content to remove."));
        assert!(result.contains("## Other Section"));
        assert!(result.contains("Keep this."));
    }

    #[test]
    fn test_remove_section_only_section() {
        let content = r#"---
id: test
---
# Title

Some description.

## Only Section

Section content.
"#;
        let doc = parse_document(content).unwrap();
        let result = doc.remove_section("Only Section");

        assert!(result.contains("# Title"));
        assert!(result.contains("Some description."));
        assert!(!result.contains("## Only Section"));
        assert!(!result.contains("Section content."));
    }

    #[test]
    fn test_remove_section_multiline_body() {
        let content = r#"---
id: test
---
# Title

## Target Section

Line 1 of content.
Line 2 of content.

A paragraph within the section.

- Bullet 1
- Bullet 2

## Next Section

Next content.
"#;
        let doc = parse_document(content).unwrap();
        let result = doc.remove_section("Target Section");

        assert!(!result.contains("## Target Section"));
        assert!(!result.contains("Line 1 of content."));
        assert!(!result.contains("Line 2 of content."));
        assert!(!result.contains("A paragraph within the section."));
        assert!(!result.contains("Bullet 1"));
        assert!(result.contains("## Next Section"));
        assert!(result.contains("Next content."));
    }
}
