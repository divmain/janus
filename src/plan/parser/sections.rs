//! Section-specific parsers for plan files
//!
//! Handles parsing of phases, ticket lists, and list items within plan sections.

use comrak::nodes::NodeValue;
use comrak::{Arena, parse_document};

use crate::plan::types::{FreeFormSection, Phase};

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

    // Process H3 sections within the phase, tracking order for round-trip fidelity
    for h3 in &section.h3_sections {
        let h3_heading_lower = h3.heading.to_lowercase();

        if h3_heading_lower == "success criteria" {
            phase.success_criteria = parse_list_items(&h3.content);
            let trimmed = h3.content.trim();
            if !trimmed.is_empty() {
                phase.success_criteria_raw = Some(trimmed.to_string());
            }
            phase.subsection_order.push(h3_heading_lower);
        } else if h3_heading_lower == "tickets" {
            phase.tickets = parse_ticket_list(&h3.content);
            let trimmed = h3.content.trim();
            if !trimmed.is_empty() {
                phase.tickets_raw = Some(trimmed.to_string());
            }
            phase.subsection_order.push(h3_heading_lower);
        } else {
            // Preserve unknown H3 subsections verbatim for round-trip fidelity
            phase.subsection_order.push(h3_heading_lower);
            phase.extra_subsections.push(FreeFormSection {
                heading: h3.heading.clone(),
                content: h3.content.trim().to_string(),
            });
        }
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

    // ==================== Unknown H3 Subsection Preservation Tests ====================

    #[test]
    fn test_parse_phase_preserves_unknown_h3_subsections() {
        use crate::plan::parser::H2Section;
        use crate::plan::parser::H3Section;

        let section = H2Section {
            heading: "Phase 1: Infrastructure".to_string(),
            content: "Phase description.\n".to_string(),
            h3_sections: vec![
                H3Section {
                    heading: "Success Criteria".to_string(),
                    content: "- Criterion 1\n".to_string(),
                },
                H3Section {
                    heading: "Implementation Notes".to_string(),
                    content: "Some important notes about implementation.\n".to_string(),
                },
                H3Section {
                    heading: "Tickets".to_string(),
                    content: "1. j-a1b2\n".to_string(),
                },
                H3Section {
                    heading: "Risk Assessment".to_string(),
                    content: "- Risk 1: Might be slow\n- Risk 2: Compatibility\n".to_string(),
                },
            ],
        };

        let phase = parse_phase_content(("1".to_string(), "Infrastructure".to_string()), &section);

        // Known subsections parsed correctly
        assert_eq!(phase.success_criteria, vec!["Criterion 1"]);
        assert_eq!(phase.tickets, vec!["j-a1b2"]);

        // Unknown subsections preserved
        assert_eq!(phase.extra_subsections.len(), 2);
        assert_eq!(phase.extra_subsections[0].heading, "Implementation Notes");
        assert!(
            phase.extra_subsections[0]
                .content
                .contains("important notes")
        );
        assert_eq!(phase.extra_subsections[1].heading, "Risk Assessment");
        assert!(phase.extra_subsections[1].content.contains("Risk 1"));

        // Subsection order recorded
        assert_eq!(
            phase.subsection_order,
            vec![
                "success criteria",
                "implementation notes",
                "tickets",
                "risk assessment"
            ]
        );
    }

    #[test]
    fn test_parse_phase_no_unknown_subsections() {
        use crate::plan::parser::H2Section;
        use crate::plan::parser::H3Section;

        let section = H2Section {
            heading: "Phase 1: Setup".to_string(),
            content: String::new(),
            h3_sections: vec![
                H3Section {
                    heading: "Success Criteria".to_string(),
                    content: "- Done\n".to_string(),
                },
                H3Section {
                    heading: "Tickets".to_string(),
                    content: "1. j-a1b2\n".to_string(),
                },
            ],
        };

        let phase = parse_phase_content(("1".to_string(), "Setup".to_string()), &section);

        assert!(phase.extra_subsections.is_empty());
        assert_eq!(phase.subsection_order, vec!["success criteria", "tickets"]);
    }
}
