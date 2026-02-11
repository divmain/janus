//! Sanitization functions for remote content adopted into local tickets.
//!
//! Remote issue titles and bodies come from untrusted sources (GitHub, Linear, etc.)
//! and must be sanitized before writing into local Markdown ticket files. Without
//! sanitization, remote content could:
//!
//! - Inject YAML frontmatter by including `---` delimiters
//! - Break title parsing with embedded newlines
//! - Introduce control characters that corrupt the file
//! - Cause excessive resource usage with very large payloads

use crate::error::{JanusError, Result};

/// Maximum length for a ticket title (in characters).
const MAX_TITLE_LENGTH: usize = 200;

/// Maximum size for a ticket body (in bytes).
const MAX_BODY_SIZE: usize = 100 * 1024; // 100 KB

/// Sanitize a remote issue title for use as a local ticket title.
///
/// - Strips control characters (except spaces and tabs)
/// - Collapses all whitespace (newlines, tabs, etc.) to single spaces
/// - Trims leading/trailing whitespace
/// - Truncates to `MAX_TITLE_LENGTH` characters
/// - Rejects empty titles after sanitization
pub fn sanitize_remote_title(title: &str) -> Result<String> {
    let sanitized: String = title
        .chars()
        .map(|c| {
            if c.is_control() && c != '\t' {
                // Replace control characters (including newlines) with space
                ' '
            } else {
                c
            }
        })
        .collect();

    // Collapse all whitespace runs to single spaces
    let sanitized = sanitized.split_whitespace().collect::<Vec<_>>().join(" ");

    if sanitized.is_empty() {
        return Err(JanusError::InvalidInput(
            "remote issue title is empty after sanitization".to_string(),
        ));
    }

    // Truncate to max length on a char boundary
    let truncated = if sanitized.chars().count() > MAX_TITLE_LENGTH {
        sanitized.chars().take(MAX_TITLE_LENGTH).collect::<String>()
    } else {
        sanitized
    };

    Ok(truncated)
}

/// Sanitize a remote issue body for use as a local ticket description.
///
/// - Strips control characters except newlines (`\n`), carriage returns (`\r`),
///   and tabs (`\t`)
/// - Neutralizes frontmatter delimiter injection: any line that is exactly `---`
///   (possibly with surrounding whitespace) is prefixed with a zero-width space
///   to prevent the parser from treating it as a frontmatter boundary
/// - Truncates to `MAX_BODY_SIZE` bytes (on a UTF-8 char boundary)
pub fn sanitize_remote_body(body: &str) -> Result<String> {
    // Enforce size limit first (truncate on char boundary)
    let body = truncate_to_byte_limit(body, MAX_BODY_SIZE);

    let sanitized: String = body
        .chars()
        .map(|c| {
            if c.is_control() && c != '\n' && c != '\r' && c != '\t' {
                // Strip control characters by replacing with empty (via filter below)
                '\0'
            } else {
                c
            }
        })
        .filter(|c| *c != '\0')
        .collect();

    // Neutralize frontmatter delimiter lines.
    // A line that is exactly `---` (with optional whitespace) at the start of the
    // body or after a newline could be interpreted as a frontmatter boundary.
    // We escape it by prefixing with a Unicode zero-width space (U+200B).
    let lines: Vec<String> = sanitized
        .lines()
        .map(|line| {
            if line.trim() == "---" {
                format!("\u{200B}{line}")
            } else {
                line.to_string()
            }
        })
        .collect();

    Ok(lines.join("\n"))
}

/// Truncate a string to fit within a byte limit, respecting UTF-8 char boundaries.
fn truncate_to_byte_limit(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Find the largest char boundary <= max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Title Sanitization Tests ====================

    #[test]
    fn test_sanitize_title_normal() {
        let result = sanitize_remote_title("Fix the login bug").unwrap();
        assert_eq!(result, "Fix the login bug");
    }

    #[test]
    fn test_sanitize_title_strips_newlines() {
        let result = sanitize_remote_title("Title with\nnewline").unwrap();
        assert_eq!(result, "Title with newline");
    }

    #[test]
    fn test_sanitize_title_strips_carriage_return() {
        let result = sanitize_remote_title("Title with\r\nCRLF").unwrap();
        assert_eq!(result, "Title with CRLF");
    }

    #[test]
    fn test_sanitize_title_strips_control_chars() {
        let result = sanitize_remote_title("Title with\x00null\x01and\x7Fcontrol").unwrap();
        assert_eq!(result, "Title with null and control");
    }

    #[test]
    fn test_sanitize_title_collapses_whitespace() {
        let result = sanitize_remote_title("Title   with   extra   spaces").unwrap();
        assert_eq!(result, "Title with extra spaces");
    }

    #[test]
    fn test_sanitize_title_trims() {
        let result = sanitize_remote_title("  padded title  ").unwrap();
        assert_eq!(result, "padded title");
    }

    #[test]
    fn test_sanitize_title_truncates_long_title() {
        let long_title = "a".repeat(300);
        let result = sanitize_remote_title(&long_title).unwrap();
        assert_eq!(result.chars().count(), MAX_TITLE_LENGTH);
    }

    #[test]
    fn test_sanitize_title_truncates_unicode() {
        // Each emoji is multiple bytes but one char
        let title = "🎉".repeat(250);
        let result = sanitize_remote_title(&title).unwrap();
        assert_eq!(result.chars().count(), MAX_TITLE_LENGTH);
        // Should still be valid UTF-8
        assert!(result.is_char_boundary(result.len()));
    }

    #[test]
    fn test_sanitize_title_rejects_empty() {
        let result = sanitize_remote_title("");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_sanitize_title_rejects_only_whitespace() {
        let result = sanitize_remote_title("   \n\t  ");
        assert!(result.is_err());
    }

    #[test]
    fn test_sanitize_title_rejects_only_control_chars() {
        let result = sanitize_remote_title("\x00\x01\x02");
        assert!(result.is_err());
    }

    #[test]
    fn test_sanitize_title_preserves_unicode() {
        let result = sanitize_remote_title("修复登录错误 🐛").unwrap();
        assert_eq!(result, "修复登录错误 🐛");
    }

    #[test]
    fn test_sanitize_title_tabs_collapsed() {
        let result = sanitize_remote_title("Title\twith\ttabs").unwrap();
        assert_eq!(result, "Title with tabs");
    }

    // ==================== Body Sanitization Tests ====================

    #[test]
    fn test_sanitize_body_normal() {
        let result = sanitize_remote_body("This is a normal body.\n\nWith paragraphs.").unwrap();
        assert_eq!(result, "This is a normal body.\n\nWith paragraphs.");
    }

    #[test]
    fn test_sanitize_body_strips_control_chars() {
        let result = sanitize_remote_body("Body with\x00null\x01chars").unwrap();
        assert_eq!(result, "Body withnullchars");
    }

    #[test]
    fn test_sanitize_body_preserves_newlines() {
        let result = sanitize_remote_body("Line 1\nLine 2\nLine 3").unwrap();
        assert_eq!(result, "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_sanitize_body_preserves_tabs() {
        let result = sanitize_remote_body("Indented:\n\t- item").unwrap();
        assert_eq!(result, "Indented:\n\t- item");
    }

    #[test]
    fn test_sanitize_body_neutralizes_frontmatter_delimiter() {
        let body = "Some text\n---\nMore text";
        let result = sanitize_remote_body(body).unwrap();
        // The `---` line should be prefixed with a zero-width space
        assert!(result.contains("\u{200B}---"));
        // It should NOT contain a bare `---` line that could be parsed as frontmatter
        for line in result.lines() {
            assert_ne!(line.trim(), "---");
        }
    }

    #[test]
    fn test_sanitize_body_neutralizes_frontmatter_delimiter_with_whitespace() {
        let body = "Text\n  ---  \nMore";
        let result = sanitize_remote_body(body).unwrap();
        // The line with `---` surrounded by spaces should also be neutralized
        for line in result.lines() {
            assert_ne!(line.trim(), "---");
        }
    }

    #[test]
    fn test_sanitize_body_neutralizes_multiple_delimiters() {
        let body = "---\nfake: frontmatter\n---\nBody text";
        let result = sanitize_remote_body(body).unwrap();
        // Both `---` lines should be neutralized
        for line in result.lines() {
            assert_ne!(line.trim(), "---");
        }
    }

    #[test]
    fn test_sanitize_body_preserves_dashes_in_text() {
        let body = "Use the --flag option\nOr check foo-bar-baz";
        let result = sanitize_remote_body(body).unwrap();
        // Non-delimiter dashes should be preserved
        assert_eq!(result, body);
    }

    #[test]
    fn test_sanitize_body_preserves_horizontal_rule_with_more_dashes() {
        let body = "Text\n----\nMore text";
        let result = sanitize_remote_body(body).unwrap();
        // `----` (four dashes) is NOT exactly `---`, so should be preserved as-is
        assert_eq!(result, body);
    }

    #[test]
    fn test_sanitize_body_truncates_large_body() {
        let large_body = "x".repeat(200 * 1024); // 200KB
        let result = sanitize_remote_body(&large_body).unwrap();
        assert!(result.len() <= MAX_BODY_SIZE);
    }

    #[test]
    fn test_sanitize_body_truncates_on_char_boundary() {
        // Build a string that's over 100KB with multi-byte chars
        let large_body = "é".repeat(60 * 1024); // each é is 2 bytes, so ~120KB
        let result = sanitize_remote_body(&large_body).unwrap();
        assert!(result.len() <= MAX_BODY_SIZE);
        // Should still be valid UTF-8 (if it wasn't, this would panic)
        assert!(result.is_char_boundary(result.len()));
    }

    #[test]
    fn test_sanitize_body_empty_is_ok() {
        let result = sanitize_remote_body("").unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_sanitize_body_preserves_markdown_formatting() {
        let body = "# Heading\n\n- bullet 1\n- bullet 2\n\n```rust\nfn main() {}\n```";
        let result = sanitize_remote_body(body).unwrap();
        assert_eq!(result, body);
    }

    #[test]
    fn test_sanitize_body_preserves_unicode() {
        let body = "修复说明\n\n详细描述 🎉";
        let result = sanitize_remote_body(body).unwrap();
        assert_eq!(result, body);
    }

    // ==================== truncate_to_byte_limit Tests ====================

    #[test]
    fn test_truncate_within_limit() {
        assert_eq!(truncate_to_byte_limit("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_at_limit() {
        assert_eq!(truncate_to_byte_limit("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_over_limit() {
        assert_eq!(truncate_to_byte_limit("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_respects_char_boundary() {
        // "é" is 2 bytes in UTF-8
        let s = "ééé"; // 6 bytes total
        let result = truncate_to_byte_limit(s, 5);
        // Should truncate to 4 bytes (2 chars), not split a char
        assert_eq!(result, "éé");
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_truncate_empty() {
        assert_eq!(truncate_to_byte_limit("", 10), "");
    }

    #[test]
    fn test_truncate_zero_limit() {
        assert_eq!(truncate_to_byte_limit("hello", 0), "");
    }

    // ==================== Integration: roundtrip through frontmatter parsing ====================

    #[test]
    fn test_sanitized_body_does_not_break_frontmatter_parsing() {
        // Simulate what happens when a malicious body is written into a ticket file
        // and then re-parsed. The frontmatter delimiter in the body must not be
        // interpreted as a frontmatter boundary.
        let malicious_body = "Innocent text\n---\nstatus: hacked\n---\nMore text";
        let sanitized = sanitize_remote_body(malicious_body).unwrap();

        // Build a fake ticket file content
        let file_content = format!(
            "---\nid: test-1234\nuuid: 550e8400-e29b-41d4-a716-446655440000\nstatus: new\ndeps: []\nlinks: []\n---\n# Test Title\n\n{sanitized}\n"
        );

        // Parse it and verify frontmatter wasn't corrupted
        let result = crate::parser::split_frontmatter(&file_content);
        assert!(result.is_ok(), "Parsing should succeed");
        let (frontmatter, body) = result.unwrap();
        assert!(frontmatter.contains("id: test-1234"));
        assert!(frontmatter.contains("status: new"));
        // The body should contain our sanitized text, NOT be split by the fake delimiter
        assert!(body.contains("Innocent text"));
        assert!(body.contains("More text"));
    }
}
