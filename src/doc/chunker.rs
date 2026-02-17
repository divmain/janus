//! AST-based document chunker using comrak.
//!
//! This module provides document chunking by parsing markdown into an AST
//! and creating chunks at heading boundaries. Each chunk tracks its heading
//! path, content, and line numbers for precise referencing.

use comrak::nodes::{Ast, NodeValue};
use comrak::{Arena, Options};

use crate::doc::types::DocChunk;
use crate::error::Result;

/// Chunk a document into sections based on heading boundaries.
///
/// Uses comrak to parse the markdown into an AST, then walks the tree
/// to identify chunk boundaries at heading nodes. Each chunk includes:
/// - The heading path (hierarchy of headings)
/// - The chunk content
/// - Start and end line numbers
///
/// Headerless regions (intro paragraphs before first heading) are handled
/// as chunks with an empty heading path.
pub fn chunk_document(label: &str, content: &str) -> Result<Vec<DocChunk>> {
    let mut options = Options::default();
    options.extension.front_matter_delimiter = Some("---".to_string());

    let arena = Arena::new();
    let root = comrak::parse_document(&arena, content, &options);

    let mut chunks = Vec::new();
    let mut current_chunk_start: Option<usize> = None;
    let mut current_chunk_content = String::new();
    let mut heading_stack: Vec<String> = Vec::new();
    let mut last_heading_level: u8 = 0;

    // Walk the AST and collect chunks
    walk_node(
        root,
        content,
        &mut chunks,
        &mut current_chunk_start,
        &mut current_chunk_content,
        &mut heading_stack,
        &mut last_heading_level,
        label,
    );

    // Handle any remaining content as a final chunk
    if let Some(start_line) = current_chunk_start {
        if !current_chunk_content.trim().is_empty() {
            let end_line = content.lines().count();
            chunks.push(DocChunk::new(
                label,
                heading_stack.clone(),
                current_chunk_content.trim().to_string(),
                start_line,
                end_line,
            ));
        }
    }

    Ok(chunks)
}

/// Walk a node and collect chunks.
#[allow(clippy::too_many_arguments)]
fn walk_node<'a>(
    node: &'a comrak::arena_tree::Node<'a, std::cell::RefCell<Ast>>,
    _content: &str,
    chunks: &mut Vec<DocChunk>,
    current_chunk_start: &mut Option<usize>,
    current_chunk_content: &mut String,
    heading_stack: &mut Vec<String>,
    last_heading_level: &mut u8,
    label: &str,
) {
    let data = node.data.borrow();

    // Process heading nodes - these create chunk boundaries
    if let NodeValue::Heading(ref heading) = data.value {
        // Finish the current chunk before starting a new one
        if let Some(start_line) = *current_chunk_start {
            if !current_chunk_content.trim().is_empty() {
                let end_line = data.sourcepos.start.line.saturating_sub(1).max(start_line);
                chunks.push(DocChunk::new(
                    label,
                    heading_stack.clone(),
                    current_chunk_content.trim().to_string(),
                    start_line,
                    end_line,
                ));
            }
        }

        // Extract heading text
        let heading_text = extract_heading_text(node);

        // Update heading stack based on level
        let level = heading.level;
        while heading_stack.len() >= level as usize {
            heading_stack.pop();
        }
        heading_stack.push(heading_text);
        *last_heading_level = level;

        // Start a new chunk from this heading
        *current_chunk_start = Some(data.sourcepos.start.line);
        *current_chunk_content = format!("{}\n", render_heading(node));
    } else if is_content_node(&data.value) {
        // This is content - include it in current chunk
        if current_chunk_start.is_none() {
            *current_chunk_start = Some(data.sourcepos.start.line);
        }

        // Extract the text content
        let node_content = render_node_to_markdown(node);
        if !node_content.is_empty() {
            if !current_chunk_content.is_empty() && !current_chunk_content.ends_with('\n') {
                current_chunk_content.push('\n');
            }
            current_chunk_content.push_str(&node_content);
        }
    }

    // Recurse into children
    for child in node.children() {
        walk_node(
            child,
            _content,
            chunks,
            current_chunk_start,
            current_chunk_content,
            heading_stack,
            last_heading_level,
            label,
        );
    }
}

/// Check if a node type represents content that should be included in chunks.
fn is_content_node(value: &NodeValue) -> bool {
    matches!(
        value,
        NodeValue::Paragraph
            | NodeValue::Text(_)
            | NodeValue::Code(_)
            | NodeValue::CodeBlock(_)
            | NodeValue::List(_)
            | NodeValue::BlockQuote
            | NodeValue::Item(_)
            | NodeValue::Table(_)
            | NodeValue::LineBreak
            | NodeValue::SoftBreak
    )
}

/// Extract the text content of a heading node.
fn extract_heading_text<'a>(
    node: &'a comrak::arena_tree::Node<'a, std::cell::RefCell<Ast>>,
) -> String {
    let mut text = String::new();
    for child in node.children() {
        let data = child.data.borrow();
        match &data.value {
            NodeValue::Text(t) => {
                text.push_str(t);
            }
            NodeValue::Code(node_code) => {
                text.push_str(&node_code.literal);
            }
            _ => {
                // Recurse for other inline elements
                text.push_str(&extract_text_from_node(child));
            }
        }
    }
    text.trim().to_string()
}

/// Extract text content from any node recursively.
fn extract_text_from_node<'a>(
    node: &'a comrak::arena_tree::Node<'a, std::cell::RefCell<Ast>>,
) -> String {
    let mut text = String::new();
    let data = node.data.borrow();

    match &data.value {
        NodeValue::Text(t) => {
            text.push_str(t);
        }
        NodeValue::Code(node_code) => {
            text.push_str(&node_code.literal);
        }
        _ => {
            for child in node.children() {
                text.push_str(&extract_text_from_node(child));
            }
        }
    }

    text
}

/// Render a heading node back to markdown.
fn render_heading<'a>(node: &'a comrak::arena_tree::Node<'a, std::cell::RefCell<Ast>>) -> String {
    let data = node.data.borrow();
    if let NodeValue::Heading(heading) = &data.value {
        let level = heading.level;
        let text = extract_heading_text(node);
        format!("{} {}", "#".repeat(level as usize), text)
    } else {
        String::new()
    }
}

/// Render a node back to markdown format.
///
/// This is a simplified renderer that handles common node types.
fn render_node_to_markdown<'a>(
    node: &'a comrak::arena_tree::Node<'a, std::cell::RefCell<Ast>>,
) -> String {
    let data = node.data.borrow();

    match &data.value {
        NodeValue::Text(text) => text.clone(),
        NodeValue::LineBreak => "\n".to_string(),
        NodeValue::SoftBreak => " ".to_string(),
        NodeValue::Paragraph => {
            let mut content = String::new();
            for child in node.children() {
                content.push_str(&render_node_to_markdown(child));
            }
            content.push('\n');
            content
        }
        NodeValue::CodeBlock(code) => {
            format!("```{}\n{}```\n", code.info, code.literal)
        }
        NodeValue::Code(node_code) => {
            format!("`{}`", node_code.literal)
        }
        NodeValue::BlockQuote => {
            let mut content = String::new();
            for child in node.children() {
                let child_text = render_node_to_markdown(child);
                for line in child_text.lines() {
                    content.push_str("> ");
                    content.push_str(line);
                    content.push('\n');
                }
            }
            content
        }
        NodeValue::List(list) => {
            let mut content = String::new();
            let marker = if list.list_type == comrak::nodes::ListType::Ordered {
                "1. "
            } else {
                "- "
            };
            for child in node.children() {
                let item_text = render_node_to_markdown(child);
                for line in item_text.lines() {
                    content.push_str(marker);
                    content.push_str(line);
                    content.push('\n');
                }
            }
            content
        }
        NodeValue::Item(_) => {
            let mut content = String::new();
            for child in node.children() {
                content.push_str(&render_node_to_markdown(child));
            }
            content
        }
        NodeValue::Strong => {
            let mut content = String::new();
            for child in node.children() {
                content.push_str(&render_node_to_markdown(child));
            }
            format!("**{content}**")
        }
        NodeValue::Emph => {
            let mut content = String::new();
            for child in node.children() {
                content.push_str(&render_node_to_markdown(child));
            }
            format!("*{content}*")
        }
        NodeValue::Link(link) => {
            let mut content = String::new();
            for child in node.children() {
                content.push_str(&render_node_to_markdown(child));
            }
            format!("[{}]({})", content, link.url)
        }
        NodeValue::FrontMatter(_) => {
            // Skip frontmatter in chunk content
            String::new()
        }
        _ => {
            // For other node types, just recurse
            let mut content = String::new();
            for child in node.children() {
                content.push_str(&render_node_to_markdown(child));
            }
            content
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_simple_doc() {
        let content = r#"---
label: test
---
# Title

Introduction paragraph.

## Section 1

Content in section 1.

More content.

## Section 2

Content in section 2.
"#;

        let chunks = chunk_document("test", content).unwrap();
        // 3 chunks: Title section, Section 1, Section 2
        assert_eq!(chunks.len(), 3);

        // First chunk should be the Title section
        assert_eq!(chunks[0].heading_path, vec!["Title"]);
        assert!(chunks[0].content.contains("# Title"));
        assert!(chunks[0].content.contains("Introduction paragraph"));

        // Second chunk should be Section 1
        assert_eq!(chunks[1].heading_path, vec!["Title", "Section 1"]);
        assert!(chunks[1].content.contains("## Section 1"));
        assert!(chunks[1].content.contains("Content in section 1"));

        // Third chunk should be Section 2
        assert_eq!(chunks[2].heading_path, vec!["Title", "Section 2"]);
        assert!(chunks[2].content.contains("## Section 2"));
        assert!(chunks[2].content.contains("Content in section 2"));
    }

    #[test]
    fn test_chunk_with_nested_headings() {
        let content = r#"---
label: test
---
# Main Title

Intro.

## Section A

Content A.

### Subsection A1

Content A1.

### Subsection A2

Content A2.

## Section B

Content B.
"#;

        let chunks = chunk_document("test", content).unwrap();

        // Should have chunks for: Main Title, Section A, Subsection A1, Subsection A2, Section B
        assert_eq!(chunks.len(), 5);

        assert_eq!(chunks[0].heading_path, vec!["Main Title"]);
        assert_eq!(chunks[1].heading_path, vec!["Main Title", "Section A"]);
        assert_eq!(
            chunks[2].heading_path,
            vec!["Main Title", "Section A", "Subsection A1"]
        );
        assert_eq!(
            chunks[3].heading_path,
            vec!["Main Title", "Section A", "Subsection A2"]
        );
        assert_eq!(chunks[4].heading_path, vec!["Main Title", "Section B"]);
    }

    #[test]
    fn test_chunk_preserves_content() {
        let content = r#"---
label: test
---
# Document

This is **bold** and *italic*.

## Code Section

```rust
fn main() {
    println!("Hello");
}
```

- List item 1
- List item 2

> Blockquote here
"#;

        let chunks = chunk_document("test", content).unwrap();
        assert_eq!(chunks.len(), 2);

        // First chunk (Document section) should have formatting
        assert!(chunks[0].content.contains("# Document"));
        assert!(chunks[0].content.contains("**bold**"));
        assert!(chunks[0].content.contains("*italic*"));

        // Second chunk (Code Section) should have code blocks
        assert!(chunks[1].content.contains("## Code Section"));
        assert!(chunks[1].content.contains("```rust"));
        assert!(chunks[1].content.contains("fn main()"));
    }

    #[test]
    fn test_chunk_line_numbers() {
        let content = "---\n\
label: test\n\
---\n\
# Title\n\
\n\
Line 1.\n\
Line 2.\n\
\n\
## Section\n\
\n\
Section line 1.\n\
Section line 2.\n";

        let chunks = chunk_document("test", content).unwrap();
        assert_eq!(chunks.len(), 2);

        // Line numbers should be reasonable and in order
        assert!(chunks[0].start_line > 0);
        assert!(chunks[0].end_line >= chunks[0].start_line);
        assert!(chunks[1].start_line > chunks[0].start_line);
        // Title section ends at or before Section starts
        assert!(chunks[0].end_line <= chunks[1].start_line);
    }

    #[test]
    fn test_chunk_empty_doc() {
        let content = "---\nlabel: test\n---\n";
        let chunks = chunk_document("test", content).unwrap();
        // Empty doc (just frontmatter) should produce no chunks
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_no_intro() {
        let content = r#"---
label: test
---
# Title

Content right after title.
"#;

        let chunks = chunk_document("test", content).unwrap();
        // Should have one chunk: the Title section
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].heading_path, vec!["Title"]);
        assert!(chunks[0].content.contains("# Title"));
        assert!(chunks[0].content.contains("Content right after title"));
    }
}
