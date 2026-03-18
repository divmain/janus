//! Syntect-based syntax highlighting for fenced code blocks.

use std::sync::LazyLock;

use iocraft::prelude::*;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

use super::types::{StyledLine, StyledSegment};

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

static SYNTAX_THEME: LazyLock<syntect::highlighting::Theme> = LazyLock::new(|| {
    let theme_set = ThemeSet::load_defaults();
    theme_set.themes["base16-eighties.dark"].clone()
});

/// Convert a syntect Color (RGBA) to an iocraft Color (RGB).
fn syntect_to_iocraft_color(c: syntect::highlighting::Color) -> Color {
    Color::Rgb {
        r: c.r,
        g: c.g,
        b: c.b,
    }
}

/// Highlight a code block with language-aware syntax highlighting.
///
/// If the language is not recognized or is empty, falls back to
/// rendering all code in the `fallback_color`.
///
/// Returns one `StyledLine` per line of code (excluding fence markers —
/// those are handled by the markdown walker).
pub fn highlight_code_block(language: &str, code: &str, fallback_color: Color) -> Vec<StyledLine> {
    if code.is_empty() {
        return Vec::new();
    }

    // Try to find syntax by language identifier
    let syntax = if language.is_empty() {
        None
    } else {
        SYNTAX_SET.find_syntax_by_token(language)
    };

    match syntax {
        Some(syntax) => {
            // Use syntect for highlighting
            let mut highlighter = HighlightLines::new(syntax, &SYNTAX_THEME);
            let mut lines = Vec::new();

            for line in code.lines() {
                match highlighter.highlight_line(line, &SYNTAX_SET) {
                    Ok(ranges) => {
                        let mut segments = Vec::new();
                        for (style, text) in ranges {
                            segments.push(StyledSegment {
                                text: text.to_string(),
                                color: Some(syntect_to_iocraft_color(style.foreground)),
                                weight: Weight::Normal,
                                italic: false,
                                decoration: TextDecoration::None,
                            });
                        }
                        lines.push(StyledLine::new(segments));
                    }
                    Err(_) => {
                        // Fall back to uniform color for this line
                        lines.push(StyledLine::new(vec![StyledSegment::colored(
                            line,
                            fallback_color,
                        )]));
                    }
                }
            }
            lines
        }
        None => {
            // Language not recognized - use uniform fallback color
            code.lines()
                .map(|line| StyledLine::new(vec![StyledSegment::colored(line, fallback_color)]))
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syntax_set_initializes() {
        let _ = &*SYNTAX_SET;
        let _ = &*SYNTAX_THEME;
    }

    #[test]
    fn test_syntax_set_finds_rust() {
        let syntax = SYNTAX_SET.find_syntax_by_token("rust");
        assert!(syntax.is_some());
    }

    #[test]
    fn test_highlight_rust_snippet() {
        let code = "fn main() { println!(\"hello\"); }";
        let lines = highlight_code_block("rust", code, Color::DarkYellow);
        assert!(!lines.is_empty());
        // Should have multiple segments with different colors
        let all_plain = lines.iter().all(|l| l.segments.len() == 1);
        // Syntect typically produces multiple segments for Rust code
        // (keywords, identifiers, strings, etc.)
        assert!(!all_plain || lines[0].segments[0].color == Some(Color::DarkYellow));
    }

    #[test]
    fn test_highlight_python_snippet() {
        let code = "def foo():\n    return 42";
        let lines = highlight_code_block("python", code, Color::DarkYellow);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_highlight_unknown_language() {
        let code = "x = 1";
        let lines = highlight_code_block("foobar_lang", code, Color::DarkYellow);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].segments.len(), 1);
        assert_eq!(lines[0].segments[0].color, Some(Color::DarkYellow));
    }

    #[test]
    fn test_highlight_empty_language() {
        let code = "x = 1";
        let lines = highlight_code_block("", code, Color::DarkYellow);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].segments[0].color, Some(Color::DarkYellow));
    }

    #[test]
    fn test_highlight_empty_code() {
        let lines = highlight_code_block("rust", "", Color::DarkYellow);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_highlight_code_with_trailing_newline() {
        let code = "fn main() {}\n";
        let lines = highlight_code_block("rust", code, Color::DarkYellow);
        // Should handle trailing newline - syntect may or may not produce an empty line
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_highlight_code_without_trailing_newline() {
        let code = "fn main() {}";
        let lines = highlight_code_block("rust", code, Color::DarkYellow);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_common_language_tokens() {
        let languages = [
            "rust",
            "python",
            "js",
            "javascript",
            "go",
            "bash",
            "sh",
            "json",
            "yaml",
            "sql",
            "html",
            "css",
            "c",
            "cpp",
            "java",
            "ruby",
        ];

        for lang in languages {
            let syntax = SYNTAX_SET.find_syntax_by_token(lang);
            assert!(syntax.is_some(), "Language '{}' not recognized", lang);
        }
    }
}
