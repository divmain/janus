//! Formatting utilities for ticket display
//!
//! Provides central location for formatting ticket data (dates, bodies, etc.)
//! to avoid duplication across CLI and TUI modules.

use std::ops::Range;

/// Format a date string for display
///
/// Extracts just the date part (YYYY-MM-DD) from an ISO datetime string.
/// If the string is too short, returns it unchanged.
///
/// # Examples
///
/// ```
/// use janus::formatting::format_date_for_display;
///
/// assert_eq!(format_date_for_display("2024-01-15T10:30:00Z"), "2024-01-15");
/// assert_eq!(format_date_for_display("2024-01-15"), "2024-01-15");
/// assert_eq!(format_date_for_display("short"), "short");
/// ```
pub fn format_date_for_display(date_str: &str) -> String {
    if date_str.len() >= 10 {
        date_str[..10].to_string()
    } else {
        date_str.to_string()
    }
}

/// Extract body content from ticket file (everything after frontmatter and title)
///
/// Returns `None` if frontmatter is not found or malformed.
/// Skips the title line (starts with #) if present.
///
/// # Arguments
///
/// * `content` - The full ticket file content including YAML frontmatter
///
/// # Examples
///
/// ```
/// use janus::formatting::extract_ticket_body;
///
/// let content = r#"---
/// id: test
/// status: new
/// ---
/// # Test Title
///
/// This is the body.
/// With multiple lines.
/// "#;
///
/// let body = extract_ticket_body(content);
/// assert!(body.is_some());
/// assert!(body.unwrap().contains("This is the body"));
/// ```
pub fn extract_ticket_body(content: &str) -> Option<String> {
    use crate::parser::{TITLE_RE, split_frontmatter};

    let (_frontmatter, body_with_title) = split_frontmatter(content).ok()?;

    let title_re = TITLE_RE.clone();
    let body = title_re.replace(&body_with_title, "").to_string();

    Some(body.trim().to_string())
}

/// Extract title range from ticket file content
///
/// Returns the byte range of the title line (the first H1 after frontmatter)
/// Returns `None` if no title is found.
///
/// # Arguments
///
/// * `content` - The full ticket file content including YAML frontmatter
pub fn extract_title_range(content: &str) -> Option<Range<usize>> {
    use crate::parser::split_frontmatter;

    let (_frontmatter, body_with_title) = split_frontmatter(content).ok()?;

    let title_re = crate::parser::TITLE_RE.clone();
    let title_captures = title_re.captures(&body_with_title)?;
    let title_text = title_captures.get(1)?.as_str();

    let title_with_hash = format!("# {}", title_text);
    let start = content.find(&title_with_hash)?;
    let end = start + title_with_hash.len();

    Some(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_date_for_display() {
        assert_eq!(
            format_date_for_display("2024-01-15T10:30:00Z"),
            "2024-01-15"
        );
        assert_eq!(format_date_for_display("2024-01-15"), "2024-01-15");
        assert_eq!(format_date_for_display("short"), "short");
        assert_eq!(format_date_for_display(""), "");
    }

    #[test]
    fn test_extract_ticket_body() {
        let content = r#"---
id: test
status: new
---
# Test Title

This is the body.
With multiple lines.
"#;
        let body = extract_ticket_body(content).unwrap();
        assert!(body.contains("This is the body"));
        assert!(body.contains("With multiple lines"));
        assert!(!body.contains("Test Title"));
    }

    #[test]
    fn test_extract_ticket_body_no_title() {
        let content = r#"---
id: test
---
No title here, just body.
"#;
        let body = extract_ticket_body(content).unwrap();
        assert!(body.contains("No title here"));
    }

    #[test]
    fn test_extract_ticket_body_no_frontmatter() {
        let content = "No frontmatter here";
        assert!(extract_ticket_body(content).is_none());
    }

    #[test]
    fn test_extract_title_range() {
        let content = r#"---
id: test
---
# My Ticket Title

Body content
"#;
        let range = extract_title_range(content);
        assert!(range.is_some());
        let title = &content[range.unwrap()];
        assert_eq!(title, "# My Ticket Title");
    }

    #[test]
    fn test_extract_title_range_no_title() {
        let content = r#"---
id: test
---
Body without title
"#;
        assert!(extract_title_range(content).is_none());
    }
}
