//! Section-specific parsers for plan files
//!
//! Handles parsing of phases, ticket lists, and list items within plan sections.

use comrak::nodes::NodeValue;
use comrak::{Arena, parse_document};

use crate::plan::types::Phase;

use super::{H2Section, comrak_options_with_tasklist, extract_text_content, parse_list_items};

/// Parse a ticket list from markdown content using comrak AST, extracting just the ticket IDs.
///
/// Handles formats like:
/// - "1. j-a1b2"
/// - "1. j-a1b2 - Some description"
/// - "1. j-a1b2 (optional note)"
/// - "- [ ] j-a1b2" (task list, unchecked)
/// - "- [x] j-a1b2" (task list, checked)
///
/// The first whitespace-delimited token of each list item is taken as the ticket ID.
/// Code blocks containing list-like text are correctly ignored.
pub fn parse_ticket_list(content: &str) -> Vec<String> {
    let arena = Arena::new();
    let options = comrak_options_with_tasklist();
    let root = parse_document(&arena, content, &options);

    let mut tickets = Vec::new();

    for node in root.children() {
        if let NodeValue::List(_) = &node.data.borrow().value {
            for child in node.children() {
                match &child.data.borrow().value {
                    NodeValue::Item(_) | NodeValue::TaskItem(_) => {
                        let text = extract_text_content(child);
                        // First whitespace-delimited token is the ticket ID
                        if let Some(id) = text.split_whitespace().next() {
                            if !id.is_empty() {
                                tickets.push(id.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    tickets
}

/// Parse phase content from an H2Section
pub fn parse_phase_content(phase_info: (String, String), section: &H2Section) -> Phase {
    let (number, name) = phase_info;

    let mut phase = Phase::new(number, name);

    // Phase description is the H2 content (before any H3)
    let description = section.content.trim();
    if !description.is_empty() {
        phase.description = Some(description.to_string());
    }

    // Process H3 sections within the phase
    for h3 in &section.h3_sections {
        let h3_heading_lower = h3.heading.to_lowercase();

        if h3_heading_lower == "success criteria" {
            phase.success_criteria = parse_list_items(&h3.content);
        } else if h3_heading_lower == "tickets" {
            phase.tickets = parse_ticket_list(&h3.content);
        }
        // Other H3 sections within a phase are ignored (could be extended later)
    }

    phase
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Ticket List Parsing Tests ====================

    #[test]
    fn test_parse_ticket_list() {
        let content = r#"
1. j-a1b2
2. j-c3d4
3. j-e5f6
"#;

        let tickets = parse_ticket_list(content);
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);
    }

    #[test]
    fn test_parse_ticket_list_with_descriptions() {
        let content = r#"
1. j-a1b2 - Add cache dependencies
2. j-c3d4 (optional: low priority)
3. j-e5f6 Implementation task
"#;

        let tickets = parse_ticket_list(content);
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);
    }

    #[test]
    fn test_parse_ticket_list_bullet_points() {
        let content = r#"
- j-a1b2
- j-c3d4
- j-e5f6
"#;

        let tickets = parse_ticket_list(content);
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);
    }

    #[test]
    fn test_parse_ticket_list_mixed() {
        let content = r#"
1. j-a1b2
- j-c3d4
* j-e5f6
+ j-g7h8
"#;

        let tickets = parse_ticket_list(content);
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6", "j-g7h8"]);
    }

    // ==================== List Item Parsing Tests ====================

    #[test]
    fn test_parse_list_items() {
        let content = r#"
- First item
- Second item
- Third item
"#;

        let items = parse_list_items(content);
        assert_eq!(items, vec!["First item", "Second item", "Third item"]);
    }

    #[test]
    fn test_parse_list_items_numbered() {
        let content = r#"
1. First
2. Second
3. Third
"#;

        let items = parse_list_items(content);
        assert_eq!(items, vec!["First", "Second", "Third"]);
    }

    // ==================== Task List Parsing Tests ====================

    #[test]
    fn test_parse_ticket_list_task_list_unchecked() {
        let content = r#"
- [ ] j-a1b2
- [ ] j-c3d4
- [ ] j-e5f6
"#;

        let tickets = parse_ticket_list(content);
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);
    }

    #[test]
    fn test_parse_ticket_list_task_list_checked() {
        let content = r#"
- [x] j-a1b2
- [ ] j-c3d4
- [x] j-e5f6
"#;

        let tickets = parse_ticket_list(content);
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);
    }

    #[test]
    fn test_parse_ticket_list_task_list_with_descriptions() {
        let content = r#"
- [ ] j-a1b2 - Add cache dependencies
- [x] j-c3d4 (completed: setup done)
- [ ] j-e5f6 Implementation task
"#;

        let tickets = parse_ticket_list(content);
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);
    }

    #[test]
    fn test_parse_list_items_task_list() {
        let content = r#"
- [ ] First criterion
- [x] Second criterion (done)
- [ ] Third criterion
"#;

        let items = parse_list_items(content);
        assert_eq!(
            items,
            vec![
                "First criterion",
                "Second criterion (done)",
                "Third criterion"
            ]
        );
    }

    // ==================== Code Block Immunity Tests ====================

    #[test]
    fn test_parse_ticket_list_ignores_code_blocks() {
        let content = r#"
1. j-a1b2
2. j-c3d4

```markdown
- j-fake1 This is inside a code block
- j-fake2 Should not be parsed
```

3. j-e5f6
"#;

        let tickets = parse_ticket_list(content);
        // Only real list items, not those inside code blocks
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);
    }

    #[test]
    fn test_parse_list_items_ignores_code_blocks() {
        let content = r#"
- Real criterion 1
- Real criterion 2

```
- Fake criterion inside code block
- Another fake criterion
```

- Real criterion 3
"#;

        let items = parse_list_items(content);
        assert_eq!(
            items,
            vec!["Real criterion 1", "Real criterion 2", "Real criterion 3"]
        );
    }

    // ==================== Multiline List Item Tests ====================

    #[test]
    fn test_parse_list_items_multiline() {
        let content = r#"
- First item with
  continuation text
- Second item
- Third item spanning
  multiple lines here
"#;

        let items = parse_list_items(content);
        assert_eq!(items.len(), 3);
        assert!(items[0].contains("First item"));
        assert!(items[0].contains("continuation text"));
        assert_eq!(items[1], "Second item");
        assert!(items[2].contains("Third item"));
        assert!(items[2].contains("multiple lines"));
    }

    #[test]
    fn test_parse_ticket_list_multiline_descriptions() {
        // Even with multiline items, only the first token is the ticket ID
        let content = r#"
1. j-a1b2 - Add cache dependencies
   with additional details on a second line
2. j-c3d4
"#;

        let tickets = parse_ticket_list(content);
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4"]);
    }
}
