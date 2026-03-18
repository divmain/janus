//! Markdown AST walker for syntax highlighting.

use comrak::nodes::{Ast, NodeValue};
use comrak::{Arena, Options};
use iocraft::prelude::*;

use super::code::highlight_code_block;
use super::types::{StyledLine, StyledSegment};
use crate::tui::theme;

/// Tracks the inherited style for nested inline elements.
#[derive(Debug, Clone, Copy)]
struct StyleContext {
    color: Option<Color>,
    weight: Weight,
    italic: bool,
    decoration: TextDecoration,
}

impl Default for StyleContext {
    fn default() -> Self {
        Self {
            color: None,
            weight: Weight::Normal,
            italic: false,
            decoration: TextDecoration::None,
        }
    }
}

/// The main walker that accumulates styled lines.
struct HighlightWalker {
    lines: Vec<StyledLine>,
    current_segments: Vec<StyledSegment>,
    style_stack: Vec<StyleContext>,
    theme: &'static theme::Theme,
}

impl HighlightWalker {
    fn new(theme: &'static theme::Theme) -> Self {
        Self {
            lines: Vec::new(),
            current_segments: Vec::new(),
            style_stack: vec![StyleContext::default()],
            theme,
        }
    }

    fn push_segment(&mut self, text: String, color: Option<Color>) {
        let style = *self.current_style();
        let segment = StyledSegment {
            text,
            color: color.or(style.color),
            weight: style.weight,
            italic: style.italic,
            decoration: style.decoration,
        };
        self.current_segments.push(segment);
    }

    fn push_segment_with_style(&mut self, text: String, ctx: StyleContext) {
        let segment = StyledSegment {
            text,
            color: ctx.color,
            weight: ctx.weight,
            italic: ctx.italic,
            decoration: ctx.decoration,
        };
        self.current_segments.push(segment);
    }

    fn finish_line(&mut self) {
        if self.current_segments.is_empty() {
            self.lines.push(StyledLine::empty());
        } else {
            let segments = std::mem::take(&mut self.current_segments);
            self.lines.push(StyledLine::new(segments));
        }
    }

    fn push_style(&mut self, ctx: StyleContext) {
        self.style_stack.push(ctx);
    }

    fn pop_style(&mut self) {
        if self.style_stack.len() > 1 {
            self.style_stack.pop();
        }
    }

    fn current_style(&self) -> &StyleContext {
        self.style_stack.last().unwrap()
    }

    fn walk_node<'a>(&mut self, node: &'a comrak::arena_tree::Node<'a, std::cell::RefCell<Ast>>) {
        let data = node.data.borrow();

        match &data.value {
            NodeValue::Document => {
                self.walk_children(node);
            }
            NodeValue::FrontMatter(_) => {
                // Skip frontmatter entirely
            }
            NodeValue::Heading(heading) => {
                let level = heading.level;
                let color = self.theme.md_heading_color(level);

                // Push bold style for entire heading (prefix and content)
                self.push_style(StyleContext {
                    color: Some(color),
                    weight: Weight::Bold,
                    italic: false,
                    decoration: TextDecoration::None,
                });

                // Emit the heading prefix (#, ##, etc.)
                let prefix = "#".repeat(level as usize);
                self.push_segment(format!("{} ", prefix), None);

                self.walk_inline_children(node);

                self.pop_style();
                self.finish_line();
                // Add blank line after heading
                self.lines.push(StyledLine::empty());
            }
            NodeValue::Paragraph => {
                self.walk_inline_children(node);
                self.finish_line();
                // Add blank line after paragraph
                self.lines.push(StyledLine::empty());
            }
            NodeValue::CodeBlock(code) => {
                // Emit opening fence with language identifier
                let fence_line = format!("```{}", code.info);
                self.push_segment(fence_line, Some(self.theme.md_code_fence));
                self.finish_line();

                // Use syntect for code highlighting
                let code_lines =
                    highlight_code_block(&code.info, &code.literal, self.theme.md_code_inline);
                self.lines.extend(code_lines);

                // Emit closing fence
                self.push_segment("```".to_string(), Some(self.theme.md_code_fence));
                self.finish_line();
                // Add blank line after code block
                self.lines.push(StyledLine::empty());
            }
            NodeValue::List(list) => {
                let is_ordered = list.list_type == comrak::nodes::ListType::Ordered;
                let mut item_number = 1;

                for child in node.children() {
                    let child_data = child.data.borrow();
                    if matches!(child_data.value, NodeValue::Item(_)) {
                        drop(child_data);
                        self.walk_list_item(child, is_ordered, item_number);
                        item_number += 1;
                    }
                }
            }
            NodeValue::BlockQuote => {
                // Emit "> " prefix in blockquote color with italic
                let prefix = "> ";
                self.push_segment_with_style(
                    prefix.to_string(),
                    StyleContext {
                        color: Some(self.theme.md_blockquote),
                        weight: Weight::Normal,
                        italic: true,
                        decoration: TextDecoration::None,
                    },
                );

                // Push italic style for blockquote content
                self.push_style(StyleContext {
                    color: Some(self.theme.md_blockquote),
                    weight: Weight::Normal,
                    italic: true,
                    decoration: TextDecoration::None,
                });

                self.walk_inline_children(node);

                self.pop_style();
                self.finish_line();
                // Add blank line after blockquote
                self.lines.push(StyledLine::empty());
            }
            NodeValue::ThematicBreak => {
                // Emit a horizontal rule
                self.push_segment("───".to_string(), Some(self.theme.md_rule));
                self.finish_line();
                // Add blank line after rule
                self.lines.push(StyledLine::empty());
            }
            _ => {
                // For other block types, just recurse
                self.walk_children(node);
            }
        }
    }

    fn walk_list_item<'a>(
        &mut self,
        node: &'a comrak::arena_tree::Node<'a, std::cell::RefCell<Ast>>,
        is_ordered: bool,
        item_number: u64,
    ) {
        let marker = if is_ordered {
            format!("{}. ", item_number)
        } else {
            "- ".to_string()
        };
        self.push_segment(marker, Some(self.theme.md_list_marker));

        // Process inline content of the list item
        self.walk_inline_children(node);
        self.finish_line();
    }

    fn walk_children<'a>(
        &mut self,
        node: &'a comrak::arena_tree::Node<'a, std::cell::RefCell<Ast>>,
    ) {
        for child in node.children() {
            self.walk_node(child);
        }
    }

    fn walk_inline_children<'a>(
        &mut self,
        node: &'a comrak::arena_tree::Node<'a, std::cell::RefCell<Ast>>,
    ) {
        for child in node.children() {
            self.walk_inline_node(child);
        }
    }

    fn walk_inline_node<'a>(
        &mut self,
        node: &'a comrak::arena_tree::Node<'a, std::cell::RefCell<Ast>>,
    ) {
        let data = node.data.borrow();

        match &data.value {
            NodeValue::Text(text) => {
                let style = *self.current_style();
                self.push_segment_with_style(text.clone(), style);
            }
            NodeValue::Code(code) => {
                // Inline code with specific color
                self.push_segment(code.literal.clone(), Some(self.theme.md_code_inline));
            }
            NodeValue::Strong => {
                let current = *self.current_style();
                self.push_style(StyleContext {
                    color: current.color,
                    weight: Weight::Bold,
                    italic: current.italic,
                    decoration: current.decoration,
                });
                self.walk_inline_children(node);
                self.pop_style();
            }
            NodeValue::Emph => {
                let current = *self.current_style();
                self.push_style(StyleContext {
                    color: current.color,
                    weight: current.weight,
                    italic: true,
                    decoration: current.decoration,
                });
                self.walk_inline_children(node);
                self.pop_style();
            }
            NodeValue::Link(link) => {
                // Emit link text with link color and underline
                let link_color = self.theme.md_link;
                self.push_style(StyleContext {
                    color: Some(link_color),
                    weight: Weight::Normal,
                    italic: false,
                    decoration: TextDecoration::Underline,
                });
                self.walk_inline_children(node);
                self.pop_style();

                // Emit URL in link color without underline
                if !link.url.is_empty() {
                    let url_text = format!(" ({})", link.url);
                    self.push_segment_with_style(
                        url_text,
                        StyleContext {
                            color: Some(link_color),
                            weight: Weight::Normal,
                            italic: false,
                            decoration: TextDecoration::None,
                        },
                    );
                }
            }
            NodeValue::SoftBreak => {
                // Soft breaks become spaces
                self.push_segment(" ".to_string(), None);
            }
            NodeValue::LineBreak => {
                // Hard breaks start a new line
                self.finish_line();
            }
            NodeValue::Strikethrough => {
                // Strikethrough: emit with ~text~ representation since iocraft may not support it
                self.push_segment("~".to_string(), None);
                self.walk_inline_children(node);
                self.push_segment("~".to_string(), None);
            }
            _ => {
                // For other inline elements, recurse
                self.walk_inline_children(node);
            }
        }
    }
}

/// Get comrak options consistent with project usage.
fn comrak_options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.front_matter_delimiter = Some("---".to_string());
    options.extension.tasklist = true;
    options.extension.strikethrough = true;
    options.extension.table = true;
    options
}

/// Highlight a markdown string into styled lines.
///
/// Parses the markdown using comrak's AST, walks the tree, and produces
/// styled segments organized into lines.
pub fn highlight_markdown(text: &str) -> Vec<StyledLine> {
    if text.is_empty() {
        return Vec::new();
    }

    let options = comrak_options();
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, text, &options);

    let theme = theme::theme();
    let mut walker = HighlightWalker::new(theme);
    walker.walk_node(root);

    // Remove trailing blank lines for cleaner output
    while walker
        .lines
        .last()
        .is_some_and(|line| line.segments.len() == 1 && line.segments[0].text.is_empty())
    {
        walker.lines.pop();
    }

    walker.lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input() {
        let lines = highlight_markdown("");
        assert!(lines.is_empty());
    }

    #[test]
    fn test_plain_paragraph() {
        let lines = highlight_markdown("plain text");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].segments.len(), 1);
        assert_eq!(lines[0].segments[0].text, "plain text");
        assert_eq!(lines[0].segments[0].color, None);
    }

    #[test]
    fn test_heading_h1() {
        let lines = highlight_markdown("# Title");
        assert_eq!(lines.len(), 1); // trailing blank line removed
        assert!(lines[0].segments[0].text.starts_with("# "));
        assert_eq!(
            lines[0].segments[0].color,
            Some(theme::theme().md_heading_1)
        );
        assert_eq!(lines[0].segments[0].weight, Weight::Bold);
    }

    #[test]
    fn test_heading_h2() {
        let lines = highlight_markdown("## Section");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].segments[0].text.starts_with("## "));
        assert_eq!(
            lines[0].segments[0].color,
            Some(theme::theme().md_heading_2)
        );
        assert_eq!(lines[0].segments[0].weight, Weight::Bold);
    }

    #[test]
    fn test_heading_h3() {
        let lines = highlight_markdown("### Subsection");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].segments[0].text.starts_with("### "));
        assert_eq!(
            lines[0].segments[0].color,
            Some(theme::theme().md_heading_3)
        );
    }

    #[test]
    fn test_heading_h4() {
        let lines = highlight_markdown("#### Deep");
        assert_eq!(lines.len(), 1);
        // H4-H6 use md_heading_3 color
        assert_eq!(
            lines[0].segments[0].color,
            Some(theme::theme().md_heading_3)
        );
    }

    #[test]
    fn test_bold_text() {
        let lines = highlight_markdown("**bold**");
        assert!(lines[0].segments.iter().any(|s| s.weight == Weight::Bold));
    }

    #[test]
    fn test_italic_text() {
        let lines = highlight_markdown("*italic*");
        assert!(lines[0].segments.iter().any(|s| s.italic));
    }

    #[test]
    fn test_bold_italic_nested() {
        let lines = highlight_markdown("***both***");
        let has_bold_italic = lines[0]
            .segments
            .iter()
            .any(|s| s.weight == Weight::Bold && s.italic);
        assert!(has_bold_italic);
    }

    #[test]
    fn test_inline_code() {
        let lines = highlight_markdown("`code`");
        let code_segment = lines[0]
            .segments
            .iter()
            .find(|s| s.color == Some(theme::theme().md_code_inline));
        assert!(code_segment.is_some());
        assert_eq!(code_segment.unwrap().text, "code");
    }

    #[test]
    fn test_fenced_code_block_no_lang() {
        let md = "```\ncode line\n```";
        let lines = highlight_markdown(md);
        // Should have: opening fence, code line, closing fence
        assert!(lines.len() >= 3);
        assert_eq!(
            lines[0].segments[0].color,
            Some(theme::theme().md_code_fence)
        );
    }

    #[test]
    fn test_fenced_code_block_with_lang() {
        let md = "```rust\nfn main() {}\n```";
        let lines = highlight_markdown(md);
        assert!(lines.len() >= 3);
        assert!(lines[0].segments[0].text.contains("rust"));
    }

    #[test]
    fn test_empty_code_block() {
        let md = "```\n```";
        let lines = highlight_markdown(md);
        // Should have opening and closing fence
        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_link() {
        let lines = highlight_markdown("[text](https://example.com)");
        let link_segment = lines[0]
            .segments
            .iter()
            .find(|s| s.decoration == TextDecoration::Underline);
        assert!(link_segment.is_some());
        assert_eq!(link_segment.unwrap().text, "text");

        let url_segment = lines[0]
            .segments
            .iter()
            .find(|s| s.text.contains("example.com"));
        assert!(url_segment.is_some());
    }

    #[test]
    fn test_unordered_list() {
        let lines = highlight_markdown("- item 1\n- item 2");
        assert!(lines.len() >= 2);
        let marker_segment = lines[0].segments.iter().find(|s| s.text == "- ");
        assert!(marker_segment.is_some());
        assert_eq!(
            marker_segment.unwrap().color,
            Some(theme::theme().md_list_marker)
        );
    }

    #[test]
    fn test_ordered_list() {
        let lines = highlight_markdown("1. first\n2. second");
        assert!(lines.len() >= 2);
        let marker_segment = lines[0].segments.iter().find(|s| s.text == "1. ");
        assert!(marker_segment.is_some());
        assert_eq!(
            marker_segment.unwrap().color,
            Some(theme::theme().md_list_marker)
        );
    }

    #[test]
    fn test_blockquote() {
        let lines = highlight_markdown("> quoted text");
        assert!(!lines.is_empty());
        let prefix_segment = lines[0].segments.iter().find(|s| s.text.starts_with(">"));
        assert!(prefix_segment.is_some());
        assert_eq!(
            prefix_segment.unwrap().color,
            Some(theme::theme().md_blockquote)
        );
    }

    #[test]
    fn test_horizontal_rule() {
        let lines = highlight_markdown("---\n\nafter rule");
        let rule_line = lines.iter().find(|l| {
            l.segments
                .iter()
                .any(|s| s.text.starts_with("───") && s.color == Some(theme::theme().md_rule))
        });
        assert!(rule_line.is_some());
    }

    #[test]
    fn test_frontmatter_stripped() {
        let md = "---\nlabel: test\n---\n# Title";
        let lines = highlight_markdown(md);
        let has_frontmatter = lines
            .iter()
            .any(|l| l.segments.iter().any(|s| s.text.contains("label:")));
        assert!(!has_frontmatter);
    }

    #[test]
    fn test_two_paragraphs() {
        let lines = highlight_markdown("para 1\n\npara 2");
        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_whitespace_only() {
        let lines = highlight_markdown("   \n   ");
        // Should handle whitespace gracefully without panic
        assert!(!lines
            .iter()
            .any(|l| l.segments.iter().any(|s| s.text.contains("label"))));
    }

    #[test]
    fn test_heading_with_inline_code() {
        let lines = highlight_markdown("# Title with `code`");
        assert!(!lines.is_empty());
        let has_code = lines[0]
            .segments
            .iter()
            .any(|s| s.color == Some(theme::theme().md_code_inline));
        assert!(has_code);
    }

    #[test]
    fn test_mixed_inline() {
        let lines = highlight_markdown("normal **bold** *italic* `code`");
        assert!(!lines.is_empty());
        // Should have at least one bold segment
        let has_bold = lines[0].segments.iter().any(|s| s.weight == Weight::Bold);
        assert!(has_bold);
        // Should have at least one italic segment
        let has_italic = lines[0].segments.iter().any(|s| s.italic);
        assert!(has_italic);
        // Should have at least one code segment
        let has_code = lines[0]
            .segments
            .iter()
            .any(|s| s.color == Some(theme::theme().md_code_inline));
        assert!(has_code);
    }

    #[test]
    fn test_complex_document() {
        let md = r#"# Main Title

This is a paragraph with **bold** and *italic*.

## Code Example

```rust
fn main() {
    println!("hello");
}
```

- List item 1
- List item 2

> A blockquote

---

End of document.
"#;
        let lines = highlight_markdown(md);
        // Should have multiple lines without panic
        assert!(lines.len() > 10);
    }
}
