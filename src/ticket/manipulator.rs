use crate::error::Result;
use crate::parser::TITLE_RE;

/// Update a field in the YAML frontmatter of a ticket file.
///
/// If the field exists, it will be updated in place. If it doesn't exist, it will be inserted
/// after the first line (typically the `id` field).
pub fn update_field(raw_content: &str, field: &str, value: &str) -> Result<String> {
    let normalized = raw_content.replace("\r\n", "\n");
    let (frontmatter, body) = crate::parser::split_frontmatter(&normalized)?;

    let frontmatter_lines: Vec<&str> = frontmatter.lines().collect();
    let mut updated_lines = Vec::new();
    let mut field_found = false;

    for line in frontmatter_lines {
        if line.starts_with(&format!("{}:", field)) {
            updated_lines.push(format!("{}: {}", field, value));
            field_found = true;
        } else {
            updated_lines.push(line.to_string());
        }
    }

    if !field_found {
        updated_lines.push(format!("{}: {}", field, value));
    }

    let updated_frontmatter = updated_lines.join("\n");

    Ok(format!("---\n{}\n---\n{}", updated_frontmatter, body))
}

/// Remove a field from the YAML frontmatter of a ticket file.
pub fn remove_field(raw_content: &str, field: &str) -> Result<String> {
    let normalized = raw_content.replace("\r\n", "\n");
    let (frontmatter, body) = crate::parser::split_frontmatter(&normalized)?;

    let updated_frontmatter: Vec<&str> = frontmatter
        .lines()
        .filter(|line| !line.starts_with(&format!("{}:", field)))
        .collect();

    let updated_frontmatter = updated_frontmatter.join("\n");

    Ok(format!("---\n{}\n---\n{}", updated_frontmatter, body))
}

/// Extract the body content from a ticket file (everything after the title).
pub fn extract_body(raw_content: &str) -> Result<String> {
    let (_, body) = crate::parser::split_frontmatter(raw_content)?;

    let title_re = crate::parser::TITLE_RE.clone();
    let body_without_title = title_re.replace(&body, "").to_string();

    Ok(body_without_title.trim().to_string())
}

/// Extract the value of a field from the YAML frontmatter of a ticket file.
pub fn extract_field_value(raw_content: &str, field: &str) -> Result<Option<String>> {
    let (frontmatter, _) = crate::parser::split_frontmatter(raw_content)?;

    for line in frontmatter.lines() {
        if let Some(rest) = line.strip_prefix(&format!("{}:", field)) {
            return Ok(Some(rest.trim().to_string()));
        }
    }

    Ok(None)
}

/// Update the title (H1 heading) in a ticket file.
pub fn update_title(raw_content: &str, new_title: &str) -> String {
    TITLE_RE
        .replace(raw_content, format!("# {}", new_title))
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
                assert!(msg.contains("missing"));
            }
            _ => panic!("Expected InvalidFormat error"),
        }
    }
}
