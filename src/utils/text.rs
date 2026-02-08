//! Text wrapping and truncation utilities
//!
//! This module provides utilities for text formatting, wrapping, and truncation
//! with support for multi-byte characters and proper ellipsis handling.

/// Truncate a string to a maximum length, handling multi-byte characters properly.
/// Appends "..." if truncated.
pub fn truncate_string(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        s.chars().take(max_len).collect()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

/// Wrap text into multiple lines, breaking at word boundaries.
///
/// Returns up to `max_lines` lines, with "..." appended to the last line
/// if the text was truncated. Each line will be at most `width` characters.
///
/// If a single word is longer than `width`, it will be broken mid-word.
pub fn wrap_text_lines(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    if width == 0 || max_lines == 0 {
        return vec![];
    }

    let text = text.trim();
    if text.is_empty() {
        return vec![];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        let current_len = current_line.chars().count();

        if current_len == 0 {
            // Starting a new line
            if word_len <= width {
                current_line = word.to_string();
            } else {
                // Word is longer than width, need to break it
                let mut chars = word.chars();
                while chars.as_str().chars().count() > 0 {
                    let chunk: String = chars.by_ref().take(width).collect();
                    if chunk.is_empty() {
                        break;
                    }

                    if lines.len() + 1 >= max_lines && chars.as_str().chars().count() > 0 {
                        // This is the last allowed line and there's more text
                        lines.push(truncate_string(&chunk, width));
                        return add_ellipsis_if_truncated(lines, true, width);
                    }

                    if chars.as_str().chars().count() > 0 {
                        lines.push(chunk);
                        if lines.len() >= max_lines {
                            return add_ellipsis_if_truncated(lines, true, width);
                        }
                    } else {
                        current_line = chunk;
                    }
                }
            }
        } else if current_len + 1 + word_len <= width {
            // Word fits on current line with a space
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            // Need to start a new line
            lines.push(current_line);

            if lines.len() >= max_lines {
                // We've hit the max lines, and there's more text
                return add_ellipsis_if_truncated(lines, true, width);
            }

            if word_len <= width {
                current_line = word.to_string();
            } else {
                // Word is longer than width, need to break it
                current_line = String::new();
                let mut chars = word.chars();
                while chars.as_str().chars().count() > 0 {
                    let chunk: String = chars.by_ref().take(width).collect();
                    if chunk.is_empty() {
                        break;
                    }

                    if lines.len() + 1 >= max_lines && chars.as_str().chars().count() > 0 {
                        lines.push(truncate_string(&chunk, width));
                        return add_ellipsis_if_truncated(lines, true, width);
                    }

                    if chars.as_str().chars().count() > 0 {
                        lines.push(chunk);
                        if lines.len() >= max_lines {
                            return add_ellipsis_if_truncated(lines, true, width);
                        }
                    } else {
                        current_line = chunk;
                    }
                }
            }
        }
    }

    // Add the last line if non-empty
    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines
}

/// Helper to add ellipsis to the last line if text was truncated
fn add_ellipsis_if_truncated(mut lines: Vec<String>, truncated: bool, width: usize) -> Vec<String> {
    if truncated && !lines.is_empty() {
        let last_idx = lines.len() - 1;
        let last_line = &lines[last_idx];
        let last_len = last_line.chars().count();

        if last_len + 3 <= width {
            // Room for "..."
            lines[last_idx] = format!("{last_line}...");
        } else if last_len >= 3 {
            // Need to truncate the last line to fit "..."
            let truncated: String = last_line.chars().take(width.saturating_sub(3)).collect();
            lines[last_idx] = format!("{truncated}...");
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_string_short() {
        assert_eq!(truncate_string("Hello", 10), "Hello");
    }

    #[test]
    fn test_truncate_string_exact() {
        assert_eq!(truncate_string("Hello", 5), "Hello");
    }

    #[test]
    fn test_truncate_string_long() {
        assert_eq!(truncate_string("Hello World", 8), "Hello...");
    }

    #[test]
    fn test_truncate_string_very_short_max() {
        assert_eq!(truncate_string("Hello World", 3), "Hel");
    }

    #[test]
    fn test_truncate_string_multibyte() {
        // Japanese text: "Hello World"
        let japanese = "こんにちは世界";
        let truncated = truncate_string(japanese, 5);
        assert_eq!(truncated, "こん...");
    }

    #[test]
    fn test_truncate_string_emoji() {
        let emoji = "Test 🎉🎊🎈 emoji";
        let truncated = truncate_string(emoji, 10);
        // Each emoji counts as 1 char, so 10 chars = "Test 🎉🎊" + "..." = 7 + 3 = 10
        assert_eq!(truncated, "Test 🎉🎊...");
    }

    #[test]
    fn test_wrap_text_lines_single_line() {
        let result = wrap_text_lines("Hello world", 20, 3);
        assert_eq!(result, vec!["Hello world"]);
    }

    #[test]
    fn test_wrap_text_lines_wraps_at_word_boundary() {
        let result = wrap_text_lines("Hello wonderful world", 12, 3);
        assert_eq!(result, vec!["Hello", "wonderful", "world"]);
    }

    #[test]
    fn test_wrap_text_lines_truncates_with_ellipsis() {
        let result = wrap_text_lines(
            "Line one is here and line two is here and line three is here and line four",
            15,
            2,
        );
        assert_eq!(result.len(), 2);
        assert!(result[1].ends_with("..."));
    }

    #[test]
    fn test_wrap_text_lines_long_word() {
        let result = wrap_text_lines("Supercalifragilisticexpialidocious", 10, 5);
        assert!(result.len() > 1);
        // Long word should be broken
    }

    #[test]
    fn test_wrap_text_lines_empty_input() {
        assert!(wrap_text_lines("", 10, 3).is_empty());
        assert!(wrap_text_lines("   ", 10, 3).is_empty());
    }

    #[test]
    fn test_wrap_text_lines_zero_width() {
        assert!(wrap_text_lines("Hello", 0, 3).is_empty());
    }

    #[test]
    fn test_wrap_text_lines_zero_max_lines() {
        assert!(wrap_text_lines("Hello", 10, 0).is_empty());
    }
}
