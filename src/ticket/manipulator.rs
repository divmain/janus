use crate::error::{JanusError, Result};
use crate::parser::TITLE_RE;

use serde_json;

/// A helper struct for editing YAML frontmatter in ticket files.
///
/// Uses structural YAML editing: parses frontmatter into a serde_yaml_ng::Mapping,
/// performs edits on the mapping, and re-serializes to ensure valid YAML output.
/// This approach correctly handles multiline values, nested structures, arrays,
/// and all YAML constructs that would break line-based parsers.
pub struct FrontmatterEditor {
    frontmatter: serde_yaml_ng::Mapping,
    body: String,
}

impl FrontmatterEditor {
    /// Create a new FrontmatterEditor from raw ticket content.
    pub fn new(raw_content: &str) -> Result<Self> {
        let normalized = raw_content.replace("\r\n", "\n");
        let (frontmatter_str, body) = crate::parser::split_frontmatter(&normalized)?;

        let frontmatter = if frontmatter_str.trim().is_empty() {
            serde_yaml_ng::Mapping::new()
        } else {
            serde_yaml_ng::from_str(&frontmatter_str).map_err(|e| {
                JanusError::InvalidFormat(format!("Failed to parse frontmatter YAML: {e}"))
            })?
        };

        Ok(Self {
            frontmatter,
            body: body.to_string(),
        })
    }

    /// Update a field in the frontmatter.
    ///
    /// If the field exists, it will be updated. If it doesn't exist, it will be added.
    /// Values are properly escaped via serde_yaml_ng serialization to ensure special
    /// YAML characters are handled correctly and prevent YAML injection.
    ///
    /// Arrays can be passed as JSON strings (e.g., `["item1", "item2"]`) and will be
    /// converted to YAML sequences automatically.
    pub fn update_field(&mut self, field: &str, value: &str) -> Result<()> {
        use serde_yaml_ng::Value;

        // Try to parse the value as JSON first (for arrays)
        let yaml_value = if value.starts_with('[') && value.ends_with(']') {
            // Attempt to parse as JSON array
            match serde_json::from_str::<Vec<String>>(value) {
                Ok(array) => {
                    // Convert Vec<String> to YAML sequence
                    Value::Sequence(array.into_iter().map(Value::String).collect())
                }
                Err(_) => {
                    // Not valid JSON, treat as string
                    Value::String(value.to_string())
                }
            }
        } else {
            // For non-array values, use string
            Value::String(value.to_string())
        };

        self.frontmatter
            .insert(Value::String(field.to_string()), yaml_value);

        Ok(())
    }

    /// Remove a field from the frontmatter.
    pub fn remove_field(&mut self, field: &str) {
        use serde_yaml_ng::Value;
        self.frontmatter.remove(Value::String(field.to_string()));
    }

    /// Build the final content with the updated frontmatter.
    pub fn build(self) -> Result<String> {
        // Serialize the mapping back to YAML
        let frontmatter_str = serde_yaml_ng::to_string(&self.frontmatter)
            .map_err(|e| {
                JanusError::InvalidFormat(format!("Failed to serialize frontmatter: {e}"))
            })?
            .trim()
            .to_string();

        if frontmatter_str.is_empty() {
            Ok(format!("---\n---\n{}", self.body))
        } else {
            Ok(format!("---\n{frontmatter_str}\n---\n{}", self.body))
        }
    }
}

/// Update a field in the YAML frontmatter of a ticket file.
///
/// If the field exists, it will be updated in place. If it doesn't exist, it will be inserted
/// after the first line (typically the `id` field).
///
/// Values are properly escaped using serde_yaml_ng to ensure special YAML characters
/// are handled correctly and prevent YAML injection.
pub fn update_field(raw_content: &str, field: &str, value: &str) -> Result<String> {
    let mut editor = FrontmatterEditor::new(raw_content)?;
    editor.update_field(field, value)?;
    editor.build()
}

/// Remove a field from the YAML frontmatter of a ticket file.
pub fn remove_field(raw_content: &str, field: &str) -> Result<String> {
    let mut editor = FrontmatterEditor::new(raw_content)?;
    editor.remove_field(field);
    editor.build()
}

/// Extract the body content from a ticket file (everything after the title).
pub fn extract_body(raw_content: &str) -> Result<String> {
    let (_, body) = crate::parser::split_frontmatter(raw_content)?;

    let title_re = crate::parser::TITLE_RE.clone();
    let body_without_title = title_re.replace(&body, "").to_string();

    Ok(body_without_title.trim().to_string())
}

/// Extract the value of a field from the YAML frontmatter of a ticket file.
///
/// Uses proper YAML deserialization to handle quoted values, block scalars, and other
/// YAML syntax correctly.
#[cfg(test)]
pub fn extract_field_value(raw_content: &str, field: &str) -> Result<Option<String>> {
    use serde_yaml_ng::Value;

    let (frontmatter, _) = crate::parser::split_frontmatter(raw_content)?;

    match serde_yaml_ng::from_str::<Value>(&frontmatter) {
        Ok(yaml_value) => {
            if let Value::Mapping(map) = yaml_value
                && let Some(Value::String(s)) = map.get(Value::String(field.to_string()))
            {
                return Ok(Some(s.clone()));
            }
            Ok(None)
        }
        Err(_) => {
            eprintln!(
                "Warning: Failed to parse YAML as structured data, falling back to line-based parsing"
            );
            for line in frontmatter.lines() {
                if let Some(rest) = line.strip_prefix(&format!("{field}:")) {
                    return Ok(Some(rest.trim().to_string()));
                }
            }
            Ok(None)
        }
    }
}

/// Update the title (H1 heading) in a ticket file.
pub fn update_title(raw_content: &str, new_title: &str) -> String {
    TITLE_RE
        .replace(raw_content, format!("# {new_title}"))
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::JanusError;

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
        assert!(result.contains("priority"));
        assert!(result.contains("3"));
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
        assert!(result.contains("priority"));
        assert!(result.contains("1"));
        assert!(result.contains("--- multiple dashes ---"));
    }

    #[test]
    fn test_update_field_malformed_frontmatter() {
        let content = "No frontmatter here\n# Just content";
        let result = update_field(content, "status", "complete");
        assert!(result.is_err());
        match result.unwrap_err() {
            JanusError::InvalidFormat(msg) => {
                assert!(msg.contains("missing"));
            }
            _ => panic!("Expected InvalidFormat error"),
        }
    }

    #[test]
    fn test_update_field_with_colon() {
        let content = r#"---
id: test-1234
status: new
---
# Test Ticket"#;

        let value = "value:with:colons";
        let result = update_field(content, "description", value).unwrap();

        assert!(result.contains("description:"));
        assert!(result.contains("value:with:colons"));
    }

    #[test]
    fn test_update_field_with_brackets() {
        let content = r#"---
id: test-1234
---
# Test Ticket"#;

        let result = update_field(content, "tags", "[tag1, tag2]").unwrap();
        assert!(result.contains("tags:"));
        assert!(result.contains("[tag1, tag2]"));
    }

    #[test]
    fn test_update_field_with_braces() {
        let content = r#"---
id: test-1234
---
# Test Ticket"#;

        let result = update_field(content, "metadata", "{key: value}").unwrap();
        assert!(result.contains("metadata:"));
        assert!(result.contains("{key: value}"));
    }

    #[test]
    fn test_update_field_with_hash() {
        let content = r#"---
id: test-1234
---
# Test Ticket"#;

        let result = update_field(content, "comment", "# This is a comment").unwrap();
        assert!(result.contains("comment:"));
        assert!(result.contains("# This is a comment"));
    }

    #[test]
    fn test_update_field_with_newline() {
        let content = r#"---
id: test-1234
---
# Test Ticket"#;

        let result = update_field(content, "description", "line1\nline2").unwrap();
        assert!(result.contains("description:"));
        assert!(result.contains("line1"));
        assert!(result.contains("line2"));

        let extracted = extract_field_value(&result, "description")
            .unwrap()
            .unwrap();
        assert!(extracted.contains("line1"));
        assert!(extracted.contains("line2"));
    }

    #[test]
    fn test_update_field_with_special_yaml_chars() {
        let content = r#"---
id: test-1234
---
# Test Ticket"#;

        let value = "value with > | [ ] { } : # & * ! | ' \" % @ `";
        let result = update_field(content, "special", value).unwrap();
        assert!(result.contains("special:"));
    }

    #[test]
    fn test_update_field_yaml_injection_prevented_newline() {
        let content = r#"---
id: test-1234
---
# Test Ticket"#;

        let malicious = "value\nmalicious_field: injected";
        let result = update_field(content, "safe", malicious).unwrap();

        assert!(result.contains("safe:"));

        let extracted = extract_field_value(&result, "safe").unwrap().unwrap();
        assert!(extracted.contains("value"));
        assert!(extracted.contains("malicious_field: injected"));
        assert_eq!(extracted, malicious);
    }

    #[test]
    fn test_update_field_yaml_injection_colon_with_space() {
        let content = r#"---
id: test-1234
---
# Test Ticket"#;

        let malicious = "value: injected";
        let result = update_field(content, "safe", malicious).unwrap();

        assert!(result.contains("safe:"));

        let extracted = extract_field_value(&result, "safe").unwrap().unwrap();
        assert_eq!(extracted, "value: injected");
    }

    #[test]
    fn test_update_field_empty_value() {
        let content = r#"---
id: test-1234
status: new
---
# Test Ticket"#;

        let result = update_field(content, "description", "").unwrap();
        assert!(result.contains("description:"));
    }

    #[test]
    fn test_update_field_with_quotes() {
        let content = r#"---
id: test-1234
---
# Test Ticket"#;

        let result = update_field(content, "title", "'quoted' and \"double-quoted\"").unwrap();
        assert!(result.contains("title:"));
    }

    #[test]
    fn test_update_field_prefix_collision() {
        // Test that "type:" doesn't match "type_info:"
        let content = r#"---
id: test-1234
type: bug
type_info: some metadata
---
# Test Ticket"#;

        let result = update_field(content, "type", "feature").unwrap();
        // type should be updated
        assert!(result.contains("type: feature"));
        // type_info should remain unchanged
        assert!(result.contains("type_info: some metadata"));
        assert!(!result.contains("type_info: feature"));
    }

    #[test]
    fn test_update_field_prefix_collision_field_with_value() {
        // Test that "status:" doesn't match "status_code:"
        let content = r#"---
id: test-1234
status: new
status_code: 200
---
# Test Ticket"#;

        let result = update_field(content, "status", "complete").unwrap();
        // status should be updated
        assert!(result.contains("status: complete"));
        // status_code should remain unchanged
        assert!(result.contains("status_code: 200"));
        assert!(!result.contains("status_code: complete"));
    }

    #[test]
    fn test_remove_field_prefix_collision() {
        // Test that "type:" doesn't remove "type_info:"
        let content = r#"---
id: test-1234
type: bug
type_info: some metadata
priority: 2
---
# Test Ticket"#;

        let result = remove_field(content, "type").unwrap();
        // type should be removed
        assert!(!result.contains("type: bug"));
        // type_info should remain
        assert!(result.contains("type_info: some metadata"));
        // other fields should remain
        assert!(result.contains("id: test-1234"));
        assert!(result.contains("priority: 2"));
    }
}
