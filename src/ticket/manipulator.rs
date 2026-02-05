use crate::error::Result;
use crate::parser::TITLE_RE;

/// A helper struct for editing YAML frontmatter in ticket files.
///
/// Handles the common logic of splitting frontmatter from body,
/// manipulating fields, and reconstructing the content.
pub struct FrontmatterEditor {
    frontmatter_lines: Vec<String>,
    body: String,
}

impl FrontmatterEditor {
    /// Create a new FrontmatterEditor from raw ticket content.
    pub fn new(raw_content: &str) -> Result<Self> {
        let normalized = raw_content.replace("\r\n", "\n");
        let (frontmatter, body) = crate::parser::split_frontmatter(&normalized)?;

        Ok(Self {
            frontmatter_lines: frontmatter.lines().map(|s| s.to_string()).collect(),
            body: body.to_string(),
        })
    }

    /// Update a field in the frontmatter.
    ///
    /// If the field exists, it will be updated in place. If it doesn't exist, it will be appended.
    /// Values are properly escaped using serde_yaml_ng to ensure special YAML characters
    /// are handled correctly and prevent YAML injection.
    pub fn update_field(&mut self, field: &str, value: &str) -> Result<()> {
        use serde_yaml_ng::Value;

        let serialized_value = if let Ok(_v) = serde_yaml_ng::from_str::<Value>(value)
            && !value.contains('\n')
            && !value.contains('\r')
        {
            value.trim().to_string()
        } else {
            serde_yaml_ng::to_string(&Value::String(value.to_string()))
                .map_err(|e| {
                    crate::error::JanusError::InvalidFormat(format!(
                        "Failed to serialize value: {e}"
                    ))
                })?
                .trim()
                .to_string()
        };

        let yaml_line = format!("{field}: {serialized_value}");
        let mut field_found = false;

        for line in &mut self.frontmatter_lines {
            if line.starts_with(&format!("{field}:")) {
                *line = yaml_line.clone();
                field_found = true;
                break;
            }
        }

        if !field_found {
            self.frontmatter_lines.push(yaml_line);
        }

        Ok(())
    }

    /// Remove a field from the frontmatter.
    pub fn remove_field(&mut self, field: &str) {
        self.frontmatter_lines
            .retain(|line| !line.starts_with(&format!("{field}:")));
    }

    /// Build the final content with the updated frontmatter.
    pub fn build(self) -> String {
        let frontmatter = self.frontmatter_lines.join("\n");
        format!("---\n{frontmatter}\n---\n{}", self.body)
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
    Ok(editor.build())
}

/// Remove a field from the YAML frontmatter of a ticket file.
pub fn remove_field(raw_content: &str, field: &str) -> Result<String> {
    let mut editor = FrontmatterEditor::new(raw_content)?;
    editor.remove_field(field);
    Ok(editor.build())
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
}
