//! Plan file parser
//!
//! Parses plan markdown files into `PlanMetadata`, handling both structured
//! sections (phases, tickets, acceptance criteria) and free-form sections.
//!
//! Uses the comrak crate for AST-based markdown parsing to correctly handle
//! edge cases like code blocks containing `##` characters.
//!
//! # Module Structure
//!
//! - `core`: Core parsing logic for plan files (parse_plan_content)
//! - `sections`: Section-specific parsers (phases, tickets, lists)
//! - `import`: Importable plan parsing for AI-generated documents
//! - `serialize`: Serialization functions for writing plans back to markdown

mod import;
mod sections;
mod serialize;

use std::collections::HashSet;
use std::sync::LazyLock;

use comrak::nodes::{AstNode, NodeValue};
use comrak::{parse_document, Arena, Options};
use regex::Regex;
use serde::Deserialize;

use crate::error::{JanusError, Result};
use crate::parser::split_frontmatter;
use crate::plan::types::{FreeFormSection, PlanMetadata, PlanSection};

// Re-export public functions from submodules
pub use import::{
    is_completed_task, is_phase_header, is_section_alias, parse_importable_plan,
    ACCEPTANCE_CRITERIA_ALIASES, DESIGN_SECTION_NAME, IMPLEMENTATION_SECTION_NAME,
    PHASE_HEADER_REGEX, PHASE_PATTERN,
};
pub use sections::parse_ticket_list;
pub use serialize::serialize_plan;

static LIST_ITEM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^[\s]*[-*+][\s]+(.+)$|^[\s]*\d+\.[\s]+(.+)$")
        .expect("item list regex should be valid")
});

/// Strict plan frontmatter struct for YAML deserialization with required fields.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanFrontmatter {
    id: String,
    uuid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    created: Option<String>,
}

/// Parse a plan file's content into PlanMetadata
///
/// The format is:
/// ```text
/// ---
/// id: plan-xxxx
/// uuid: ...
/// created: ...
/// ---
/// # Plan Title
///
/// Description/preamble content...
///
/// ## Acceptance Criteria
///
/// - Criterion 1
/// - Criterion 2
///
/// ## Phase 1: Infrastructure
///
/// Phase description...
///
/// ### Success Criteria
///
/// - Success criterion 1
///
/// ### Tickets
///
/// 1. j-a1b2
/// 2. j-c3d4
///
/// ## Free-form Section
///
/// Any content preserved verbatim...
/// ```
pub fn parse_plan_content(content: &str) -> Result<PlanMetadata> {
    let (yaml, body) = split_frontmatter(content)?;

    let mut metadata = parse_yaml_frontmatter(&yaml)?;
    parse_body(&body, &mut metadata)?;

    Ok(metadata)
}

/// Parse YAML frontmatter into PlanMetadata fields
fn parse_yaml_frontmatter(yaml: &str) -> Result<PlanMetadata> {
    let frontmatter: PlanFrontmatter = serde_yaml_ng::from_str(yaml)
        .map_err(|e| JanusError::InvalidFormat(format!("YAML parsing error: {}", e)))?;

    let metadata = PlanMetadata {
        id: Some(frontmatter.id),
        uuid: Some(frontmatter.uuid),
        created: frontmatter.created,
        ..Default::default()
    };

    Ok(metadata)
}

/// Parse the markdown body to extract title, description, and sections
fn parse_body(body: &str, metadata: &mut PlanMetadata) -> Result<()> {
    let arena = Arena::new();
    let options = Options::default();
    let root = parse_document(&arena, body, &options);

    // Collect all sections from the document
    let (title, preamble, h2_sections) = collect_document_sections(root, &options);

    // Set title and description
    metadata.title = title;
    if let Some(preamble_text) = preamble {
        let trimmed = preamble_text.trim();
        if !trimmed.is_empty() {
            metadata.description = Some(trimmed.to_string());
        }
    }

    // Track which structured sections have been seen (first occurrence only)
    let mut seen_sections: HashSet<&'static str> = HashSet::new();

    for section in h2_sections {
        classify_and_add_section(section, metadata, &mut seen_sections);
    }

    Ok(())
}

/// Collect document sections by heading level.
///
/// Returns (title, preamble, h2_sections) tuple where:
/// - title: The H1 heading text if present
/// - preamble: Content between H1 and first H2
/// - h2_sections: All H2 sections with their nested H3 content
fn collect_document_sections<'a>(
    root: &'a AstNode<'a>,
    options: &Options,
) -> (Option<String>, Option<String>, Vec<H2Section>) {
    let mut title = None;
    let mut preamble = String::new();
    let mut h2_sections = Vec::new();
    let mut collector = SectionCollector::new();

    for node in root.children() {
        match &node.data.borrow().value {
            NodeValue::Heading(heading) => {
                let heading_text = extract_text_content(node);

                match heading.level {
                    1 => {
                        title = Some(heading_text.trim().to_string());
                    }
                    2 => {
                        // Finalize any pending section before starting new one
                        if let Some(section) = collector.finalize_h2() {
                            h2_sections.push(section);
                        }
                        collector.start_h2(heading_text.trim().to_string());
                    }
                    3 => {
                        collector.start_h3(heading_text.trim().to_string());
                    }
                    _ => {
                        // H4+ treated as content
                        let rendered = render_node_to_markdown(node, options);
                        collector.append_content(&rendered, &mut preamble);
                    }
                }
            }
            _ => {
                let rendered = render_node_to_markdown(node, options);
                collector.append_content(&rendered, &mut preamble);
            }
        }
    }

    // Finalize last section
    if let Some(section) = collector.finalize_h2() {
        h2_sections.push(section);
    }

    let preamble_opt = if preamble.is_empty() {
        None
    } else {
        Some(preamble)
    };

    (title, preamble_opt, h2_sections)
}

/// Helper struct to track section collection state
struct SectionCollector {
    current_h2: Option<H2Section>,
    current_h3: Option<H3Section>,
    in_preamble: bool,
}

impl SectionCollector {
    fn new() -> Self {
        Self {
            current_h2: None,
            current_h3: None,
            in_preamble: true,
        }
    }

    fn start_h2(&mut self, heading: String) {
        self.in_preamble = false;
        self.current_h2 = Some(H2Section {
            heading,
            content: String::new(),
            h3_sections: Vec::new(),
        });
    }

    fn start_h3(&mut self, heading: String) {
        // Push any pending H3 to current H2
        if let Some(h3) = self.current_h3.take()
            && let Some(ref mut h2) = self.current_h2
        {
            h2.h3_sections.push(h3);
        }

        self.current_h3 = Some(H3Section {
            heading,
            content: String::new(),
        });
    }

    fn append_content(&mut self, content: &str, preamble: &mut String) {
        if let Some(ref mut h3) = self.current_h3 {
            h3.content.push_str(content);
        } else if let Some(ref mut h2) = self.current_h2 {
            h2.content.push_str(content);
        } else if self.in_preamble {
            preamble.push_str(content);
        }
    }

    fn finalize_h2(&mut self) -> Option<H2Section> {
        // Push any pending H3 to current H2
        if let Some(h3) = self.current_h3.take()
            && let Some(ref mut h2) = self.current_h2
        {
            h2.h3_sections.push(h3);
        }

        self.current_h2.take()
    }
}

/// Classify an H2 section and add it to metadata
fn classify_and_add_section(
    section: H2Section,
    metadata: &mut PlanMetadata,
    seen_sections: &mut HashSet<&'static str>,
) {
    let heading_lower = section.heading.to_lowercase();

    // Check for Acceptance Criteria (case-insensitive, exact match)
    // Only the first occurrence is parsed as structured data
    if heading_lower == "acceptance criteria" && seen_sections.insert("acceptance_criteria") {
        metadata.acceptance_criteria = sections::parse_list_items(&section.content);
        return;
    }

    // Check for Tickets section (simple plans, case-insensitive, exact match)
    // Only the first occurrence is parsed as structured data
    if heading_lower == "tickets" && seen_sections.insert("tickets") {
        let tickets = sections::parse_ticket_list(&section.content);
        metadata.sections.push(PlanSection::Tickets(tickets));
        return;
    }

    // Check for Phase header: "Phase N: Name" or "Phase N - Name"
    // Phases can appear multiple times (Phase 1, Phase 2, etc.)
    if let Some(phase) = try_parse_phase_header(&section.heading) {
        let parsed_phase = sections::parse_phase_content(phase, &section);
        metadata.sections.push(PlanSection::Phase(parsed_phase));
        return;
    }

    // Otherwise, treat as free-form section
    let full_content = reconstruct_section_content(&section);
    metadata
        .sections
        .push(PlanSection::FreeForm(FreeFormSection {
            heading: section.heading,
            content: full_content,
        }));
}

/// Temporary structure for collecting H2 section content
pub(crate) struct H2Section {
    pub heading: String,
    pub content: String,
    pub h3_sections: Vec<H3Section>,
}

/// Temporary structure for collecting H3 section content
pub(crate) struct H3Section {
    pub heading: String,
    pub content: String,
}

/// Try to parse a heading as a phase header
/// Matches: "Phase 1: Name", "Phase 2a - Name", "Phase 10:", "Phase 1" (no separator)
fn try_parse_phase_header(heading: &str) -> Option<(String, String)> {
    PHASE_HEADER_REGEX.captures(heading).map(|caps| {
        let number = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
        let name = caps
            .get(3)
            .map(|m| m.as_str().trim())
            .unwrap_or("")
            .to_string();
        (number, name)
    })
}

/// Reconstruct the full content of a section including H3 subsections
fn reconstruct_section_content(section: &H2Section) -> String {
    let mut content = section.content.clone();

    for h3 in &section.h3_sections {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&format!("\n### {}\n\n", h3.heading));
        content.push_str(&h3.content);
    }

    content.trim().to_string()
}

/// Extract plain text content from a node and its children
pub(crate) fn extract_text_content<'a>(node: &'a AstNode<'a>) -> String {
    let mut text = String::new();
    collect_text(node, &mut text);
    text
}

/// Recursively collect text from nodes
fn collect_text<'a>(node: &'a AstNode<'a>, text: &mut String) {
    match &node.data.borrow().value {
        NodeValue::Text(t) => {
            text.push_str(t);
        }
        NodeValue::Code(c) => {
            text.push_str(&c.literal);
        }
        NodeValue::SoftBreak | NodeValue::LineBreak => {
            text.push(' ');
        }
        _ => {
            for child in node.children() {
                collect_text(child, text);
            }
        }
    }
}

/// Render a single AST node back to markdown
pub(crate) fn render_node_to_markdown<'a>(node: &'a AstNode<'a>, options: &Options) -> String {
    let mut output = Vec::new();
    comrak::format_commonmark(node, options, &mut output)
        .expect("failed to format markdown node to in-memory buffer");
    String::from_utf8_lossy(&output).to_string()
}

/// Parse a bullet or numbered list into string items (used by sections module)
pub(crate) fn parse_list_items_with_regex(content: &str) -> Vec<String> {
    let mut items = Vec::new();

    for caps in LIST_ITEM_RE.captures_iter(content) {
        // Try bullet point match first, then numbered
        let item = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str().trim().to_string());

        if let Some(text) = item
            && !text.is_empty()
        {
            items.push(text);
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Basic Parsing Tests ====================

    #[test]
    fn test_parse_simple_plan() {
        let content = r#"---
id: plan-a1b2
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# Simple Plan Title

This is the plan description.

## Acceptance Criteria

- All tests pass
- Documentation complete

## Tickets

1. j-a1b2
2. j-c3d4
3. j-e5f6
"#;

        let metadata = parse_plan_content(content).unwrap();

        assert_eq!(metadata.id, Some("plan-a1b2".to_string()));
        assert_eq!(
            metadata.uuid,
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
        assert_eq!(metadata.created, Some("2024-01-01T00:00:00Z".to_string()));
        assert_eq!(metadata.title, Some("Simple Plan Title".to_string()));
        assert_eq!(
            metadata.description,
            Some("This is the plan description.".to_string())
        );

        assert_eq!(metadata.acceptance_criteria.len(), 2);
        assert_eq!(metadata.acceptance_criteria[0], "All tests pass");
        assert_eq!(metadata.acceptance_criteria[1], "Documentation complete");

        assert!(metadata.is_simple());
        assert!(!metadata.is_phased());

        let tickets = metadata.all_tickets();
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);
    }

    #[test]
    fn test_parse_phased_plan() {
        let content = r#"---
id: plan-b2c3
uuid: 550e8400-e29b-41d4-a716-446655440014
created: 2024-01-01T00:00:00Z
---
# Phased Plan

Overview of the plan.

## Acceptance Criteria

- Performance targets met

## Phase 1: Infrastructure

Set up the foundational components.

### Success Criteria

- Database tables created
- Helper functions work

### Tickets

1. j-a1b2
2. j-c3d4

## Phase 2: Implementation

Implement the core logic.

### Tickets

1. j-e5f6
"#;

        let metadata = parse_plan_content(content).unwrap();

        assert_eq!(metadata.title, Some("Phased Plan".to_string()));
        assert!(metadata.is_phased());
        assert!(!metadata.is_simple());

        let phases = metadata.phases();
        assert_eq!(phases.len(), 2);

        // Phase 1
        assert_eq!(phases[0].number, "1");
        assert_eq!(phases[0].name, "Infrastructure");
        assert_eq!(
            phases[0].description,
            Some("Set up the foundational components.".to_string())
        );
        assert_eq!(phases[0].success_criteria.len(), 2);
        assert_eq!(phases[0].tickets, vec!["j-a1b2", "j-c3d4"]);

        // Phase 2
        assert_eq!(phases[1].number, "2");
        assert_eq!(phases[1].name, "Implementation");
        assert_eq!(phases[1].tickets, vec!["j-e5f6"]);

        // All tickets across phases
        let all_tickets = metadata.all_tickets();
        assert_eq!(all_tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);
    }

    #[test]
    fn test_parse_plan_with_freeform_sections() {
        let content = r#"---
id: plan-c3d4
uuid: 550e8400-e29b-41d4-a716-446655440007
created: 2024-01-01T00:00:00Z
---
# Plan with Free-form Content

Description.

## Overview

### Motivation

This section explains why we're doing this.

### Key Decisions

1. Decision one
2. Decision two

## SQLite Schema

```sql
CREATE TABLE tickets (
    id TEXT PRIMARY KEY
);
```

## Phase 1: Setup

Initial setup.

### Tickets

1. j-a1b2

## Open Questions

1. How often should we sync?
2. What about edge cases?
"#;

        let metadata = parse_plan_content(content).unwrap();

        assert!(metadata.is_phased());

        // Check free-form sections
        let freeform = metadata.free_form_sections();
        assert_eq!(freeform.len(), 3); // Overview, SQLite Schema, Open Questions

        // Overview section with nested H3s
        assert_eq!(freeform[0].heading, "Overview");
        assert!(freeform[0].content.contains("Motivation"));
        assert!(freeform[0].content.contains("Key Decisions"));

        // SQLite Schema section with code block
        assert_eq!(freeform[1].heading, "SQLite Schema");
        assert!(freeform[1].content.contains("CREATE TABLE"));

        // Open Questions
        assert_eq!(freeform[2].heading, "Open Questions");

        // Check phases
        let phases = metadata.phases();
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].name, "Setup");
    }

    // ==================== Phase Header Parsing Tests ====================

    #[test]
    fn test_parse_phase_header_variants() {
        // Standard format
        let result = try_parse_phase_header("Phase 1: Infrastructure");
        assert_eq!(
            result,
            Some(("1".to_string(), "Infrastructure".to_string()))
        );

        // Dash separator
        let result = try_parse_phase_header("Phase 2 - Implementation");
        assert_eq!(
            result,
            Some(("2".to_string(), "Implementation".to_string()))
        );

        // Sub-phase notation
        let result = try_parse_phase_header("Phase 2a: Sub-task");
        assert_eq!(result, Some(("2a".to_string(), "Sub-task".to_string())));

        // Multi-digit
        let result = try_parse_phase_header("Phase 10: Final Phase");
        assert_eq!(result, Some(("10".to_string(), "Final Phase".to_string())));

        // No name (just colon)
        let result = try_parse_phase_header("Phase 1:");
        assert_eq!(result, Some(("1".to_string(), "".to_string())));

        // No separator or name
        let result = try_parse_phase_header("Phase 1");
        assert_eq!(result, Some(("1".to_string(), "".to_string())));

        // Case insensitive
        let result = try_parse_phase_header("PHASE 1: Test");
        assert_eq!(result, Some(("1".to_string(), "Test".to_string())));

        // Not a phase header
        let result = try_parse_phase_header("Phase Diagrams");
        assert!(result.is_none());

        let result = try_parse_phase_header("Phase without number");
        assert!(result.is_none());
    }

    // ==================== Edge Case Tests ====================

    #[test]
    fn test_parse_plan_missing_frontmatter() {
        let content = "# No frontmatter\n\nJust content.";
        let result = parse_plan_content(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_plan_minimal() {
        let content = r#"---
id: plan-min
uuid: 550e8400-e29b-41d4-a716-446655440000
---
# Minimal Plan
"#;

        let metadata = parse_plan_content(content).unwrap();
        assert_eq!(metadata.id, Some("plan-min".to_string()));
        assert_eq!(metadata.title, Some("Minimal Plan".to_string()));
        assert!(metadata.description.is_none());
        assert!(metadata.acceptance_criteria.is_empty());
        assert!(metadata.sections.is_empty());
    }

    #[test]
    fn test_parse_plan_no_title() {
        let content = r#"---
id: plan-notitle
uuid: 550e8400-e29b-41d4-a716-446655440001
---
No H1 heading, just content.

## Phase 1: Test

### Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(content).unwrap();
        assert!(metadata.title.is_none());
        // Content before first H2 is description
        assert!(metadata
            .description
            .as_ref()
            .unwrap()
            .contains("No H1 heading"));
    }

    #[test]
    fn test_parse_plan_empty_phase() {
        let content = r#"---
id: plan-empty
uuid: 550e8400-e29b-41d4-a716-446655440002
---
# Plan with Empty Phase

## Phase 1: Empty Phase

This phase has no tickets.

### Success Criteria

- Something works
"#;

        let metadata = parse_plan_content(content).unwrap();
        let phases = metadata.phases();
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].name, "Empty Phase");
        assert!(phases[0].tickets.is_empty());
        assert_eq!(phases[0].success_criteria.len(), 1);
    }

    #[test]
    fn test_parse_plan_code_block_with_hash() {
        // Code blocks containing ## should not be treated as section headers
        let content = r#"---
id: plan-code
uuid: 550e8400-e29b-41d4-a716-446655440003
---
# Code Test Plan

## Overview

Here's some code:

```markdown
## This is not a real header

It's inside a code block.
```

## Phase 1: Real Phase

### Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(content).unwrap();

        // Should have Overview as free-form and Phase 1 as phase
        let freeform = metadata.free_form_sections();
        assert_eq!(freeform.len(), 1);
        assert_eq!(freeform[0].heading, "Overview");
        // The code block should be in the content
        assert!(freeform[0].content.contains("This is not a real header"));

        let phases = metadata.phases();
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].name, "Real Phase");
    }

    #[test]
    fn test_parse_plan_duplicate_acceptance_criteria() {
        // First occurrence is used, subsequent treated as free-form
        let content = r#"---
id: plan-dup
uuid: 550e8400-e29b-41d4-a716-446655440004
---
# Duplicate Test

## Acceptance Criteria

- First criteria list

## Acceptance Criteria

- Second criteria list (should be free-form)
"#;

        let metadata = parse_plan_content(content).unwrap();

        assert_eq!(metadata.acceptance_criteria.len(), 1);
        assert_eq!(metadata.acceptance_criteria[0], "First criteria list");

        let freeform = metadata.free_form_sections();
        assert_eq!(freeform.len(), 1);
        assert_eq!(freeform[0].heading, "Acceptance Criteria");
    }

    #[test]
    fn test_parse_plan_case_insensitive_sections() {
        let content = r#"---
id: plan-case
uuid: 550e8400-e29b-41d4-a716-446655440005
---
# Case Test

## ACCEPTANCE CRITERIA

- All caps works

## phase 1: lowercase

### tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(content).unwrap();

        assert_eq!(metadata.acceptance_criteria.len(), 1);
        assert_eq!(metadata.acceptance_criteria[0], "All caps works");

        let phases = metadata.phases();
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].number, "1");
        assert_eq!(phases[0].name, "lowercase");
        assert_eq!(phases[0].tickets, vec!["j-a1b2"]);
    }

    #[test]
    fn test_parse_plan_section_ordering_preserved() {
        let content = r#"---
id: plan-order
uuid: 550e8400-e29b-41d4-a716-446655440006
---
# Section Order Test

## Overview

First section.

## Phase 1: First Phase

### Tickets

1. j-a1b2

## Technical Details

Middle section.

## Phase 2: Second Phase

### Tickets

1. j-c3d4

## Conclusion

Last section.
"#;

        let metadata = parse_plan_content(content).unwrap();

        // Verify section order
        assert_eq!(metadata.sections.len(), 5);

        // Overview (free-form)
        assert!(
            matches!(&metadata.sections[0], PlanSection::FreeForm(f) if f.heading == "Overview")
        );
        // Phase 1
        assert!(matches!(&metadata.sections[1], PlanSection::Phase(p) if p.number == "1"));
        // Technical Details (free-form)
        assert!(
            matches!(&metadata.sections[2], PlanSection::FreeForm(f) if f.heading == "Technical Details")
        );
        // Phase 2
        assert!(matches!(&metadata.sections[3], PlanSection::Phase(p) if p.number == "2"));
        // Conclusion (free-form)
        assert!(
            matches!(&metadata.sections[4], PlanSection::FreeForm(f) if f.heading == "Conclusion")
        );
    }

    #[test]
    fn test_parse_plan_phase_diagrams_is_freeform() {
        // "Phase Diagrams" should be treated as free-form (no number after "Phase")
        let content = r#"---
id: plan-diagrams
uuid: 550e8400-e29b-41d4-a716-446655440008
---
# Diagrams Test

## Phase Diagrams

Some diagrams here.

## Phase 1: Real Phase

### Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(content).unwrap();

        let freeform = metadata.free_form_sections();
        assert_eq!(freeform.len(), 1);
        assert_eq!(freeform[0].heading, "Phase Diagrams");

        let phases = metadata.phases();
        assert_eq!(phases.len(), 1);
    }

    #[test]
    fn test_parse_plan_tickets_discussion_is_freeform() {
        // "Tickets Discussion" should be free-form (not exact match for "Tickets")
        let content = r#"---
id: plan-discussion
uuid: 550e8400-e29b-41d4-a716-446655440009
---
# Discussion Test

## Tickets

1. j-a1b2

## Tickets Discussion

Discussion about tickets.
"#;

        let metadata = parse_plan_content(content).unwrap();

        // First "Tickets" is structured
        assert!(metadata.is_simple());
        let tickets = metadata.all_tickets();
        assert_eq!(tickets, vec!["j-a1b2"]);

        // "Tickets Discussion" is free-form
        let freeform = metadata.free_form_sections();
        assert_eq!(freeform.len(), 1);
        assert_eq!(freeform[0].heading, "Tickets Discussion");
    }

    #[test]
    fn test_parse_plan_multiline_description() {
        let content = r#"---
id: plan-multi
uuid: 550e8400-e29b-41d4-a716-446655440015
---
# Multi-line Description Test

This is the first paragraph of the description.

This is the second paragraph with **bold** text.

- A bullet point
- Another bullet point

## Phase 1: Test

### Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(content).unwrap();

        let desc = metadata.description.unwrap();
        assert!(desc.contains("first paragraph"));
        assert!(desc.contains("second paragraph"));
        assert!(desc.contains("bold"));
        assert!(desc.contains("bullet point"));
    }

    #[test]
    fn test_parse_plan_with_h4_nested_headers() {
        // H4+ headers within free-form sections should be preserved
        let content = r#"---
id: plan-h4
uuid: 550e8400-e29b-41d4-a716-446655440011
---
# Plan with Deep Nesting

## Technical Spec

### Component A

#### Sub-component A1

Details about A1.

#### Sub-component A2

Details about A2.

### Component B

More details.

## Phase 1: Implementation

### Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(content).unwrap();

        let freeform = metadata.free_form_sections();
        assert_eq!(freeform.len(), 1);
        assert_eq!(freeform[0].heading, "Technical Spec");

        // H3s and H4s should be in the content
        let content = &freeform[0].content;
        assert!(content.contains("Component A"));
        assert!(content.contains("Sub-component A1"));
        assert!(content.contains("Sub-component A2"));
        assert!(content.contains("Component B"));
    }

    // ==================== Helper Function Tests ====================

    #[test]
    fn test_split_frontmatter() {
        let content = "---\nid: test\n---\n# Title\n\nBody";
        let (yaml, body) = split_frontmatter(content).unwrap();
        assert_eq!(yaml, "id: test");
        assert_eq!(body, "# Title\n\nBody");
    }

    #[test]
    fn test_split_frontmatter_missing() {
        let content = "# No frontmatter";
        let result = split_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_yaml_frontmatter() {
        let yaml = "id: plan-test\nuuid: abc-123\ncreated: 2024-01-01";
        let metadata = parse_yaml_frontmatter(yaml).unwrap();
        assert_eq!(metadata.id, Some("plan-test".to_string()));
        assert_eq!(metadata.uuid, Some("abc-123".to_string()));
        assert_eq!(metadata.created, Some("2024-01-01".to_string()));
    }

    // ==================== Integration Tests ====================

    #[test]
    fn test_parse_complex_plan() {
        // This tests a realistic, complex plan similar to DESIGN.md examples
        let content = r#"---
id: plan-cache
uuid: 7c9e6679-7425-40de-944b-e07fc1f90ae7
created: 2024-01-15T10:30:00Z
---
# SQLite Cache Implementation Plan

This plan implements a SQLite-based caching layer for improved performance.

## Acceptance Criteria

- Cache provides <5ms lookups
- All existing tests continue to pass
- Graceful degradation when cache unavailable

## Overview

### Motivation

**Current performance characteristics (10,000 tickets):**
- Single ticket lookup: ~500ms
- `janus ls` / TUI startup: ~1-5s

**Target performance (with cache):**
- Single ticket lookup: <5ms
- `janus ls` / TUI startup: ~25-50ms

### Key Design Decisions

1. **Cache is optional and transparent** - Falls back to file-based operations
2. **Per-repo isolation** - Each repo has its own cache database
3. **Metadata-only cache** - Store frontmatter fields; read body on demand

## SQLite Schema

```sql
CREATE TABLE IF NOT EXISTS tickets (
    ticket_id TEXT PRIMARY KEY,
    mtime_ns INTEGER NOT NULL,
    status TEXT,
    title TEXT,
    priority INTEGER
);

CREATE INDEX IF NOT EXISTS idx_tickets_status ON tickets(status);
```

## Phase 1: Infrastructure

Set up the foundational components for the caching system.

### Success Criteria

- Database tables created correctly
- Helper functions work as expected
- 7 unit tests pass

### Tickets

1. j-dep1 - Add cache dependencies to Cargo.toml
2. j-mod2 - Create src/cache.rs with basic structure

## Phase 2: Sync Algorithm

Implement the core synchronization logic.

### Success Criteria

- Mtime comparison detects changes
- Full sync cycle completes correctly

### Tickets

1. j-scan3 - Implement directory scanning
2. j-sync4 - Implement sync algorithm
3. j-txn5 - Add transaction support

## Performance Benchmarks

| Operation | Before | After |
|-----------|--------|-------|
| Single ticket lookup | ~500ms | <5ms |
| `janus ls` (cache warm) | ~1-5s | ~25-50ms |

## Open Questions

1. **TUI reload frequency:** How often should cache sync during TUI session?
2. **Body content caching:** Should we cache ticket bodies for full-text search?
"#;

        let metadata = parse_plan_content(content).unwrap();

        // Frontmatter
        assert_eq!(metadata.id, Some("plan-cache".to_string()));
        assert_eq!(
            metadata.uuid,
            Some("7c9e6679-7425-40de-944b-e07fc1f90ae7".to_string())
        );
        assert_eq!(metadata.created, Some("2024-01-15T10:30:00Z".to_string()));

        // Title and description
        assert_eq!(
            metadata.title,
            Some("SQLite Cache Implementation Plan".to_string())
        );
        assert!(metadata
            .description
            .as_ref()
            .unwrap()
            .contains("SQLite-based caching layer"));

        // Acceptance criteria
        assert_eq!(metadata.acceptance_criteria.len(), 3);
        assert!(metadata.acceptance_criteria[0].contains("<5ms"));

        // Structure
        assert!(metadata.is_phased());
        assert!(!metadata.is_simple());

        // Phases
        let phases = metadata.phases();
        assert_eq!(phases.len(), 2);

        assert_eq!(phases[0].number, "1");
        assert_eq!(phases[0].name, "Infrastructure");
        assert_eq!(phases[0].success_criteria.len(), 3);
        assert_eq!(phases[0].tickets.len(), 2);
        assert_eq!(phases[0].tickets[0], "j-dep1");

        assert_eq!(phases[1].number, "2");
        assert_eq!(phases[1].name, "Sync Algorithm");
        assert_eq!(phases[1].tickets.len(), 3);

        // Free-form sections
        let freeform = metadata.free_form_sections();
        assert_eq!(freeform.len(), 4); // Overview, SQLite Schema, Performance Benchmarks, Open Questions

        // Verify section ordering
        let headings: Vec<&str> = freeform.iter().map(|f| f.heading.as_str()).collect();
        assert!(headings.contains(&"Overview"));
        assert!(headings.contains(&"SQLite Schema"));
        assert!(headings.contains(&"Performance Benchmarks"));
        assert!(headings.contains(&"Open Questions"));

        // Verify Overview content has nested headers
        let overview = freeform.iter().find(|f| f.heading == "Overview").unwrap();
        assert!(overview.content.contains("Motivation"));
        assert!(overview.content.contains("Key Design Decisions"));

        // Verify SQLite Schema has code block
        let schema = freeform
            .iter()
            .find(|f| f.heading == "SQLite Schema")
            .unwrap();
        assert!(schema.content.contains("CREATE TABLE"));
        assert!(schema.content.contains("ticket_id TEXT PRIMARY KEY"));

        // All tickets
        let all_tickets = metadata.all_tickets();
        assert_eq!(
            all_tickets,
            vec!["j-dep1", "j-mod2", "j-scan3", "j-sync4", "j-txn5"]
        );
    }

    #[test]
    fn test_parse_plan_with_crlf_line_endings() {
        let content = "---\r\n\
id: plan-crlf\r\n\
uuid: 550e8400-e29b-41d4-a716-446655440010\r\n\
created: 2024-01-01T00:00:00Z\r\n\
---\r\n\
# CRLF Plan\r\n\
\r\n\
A plan with Windows-style line endings.\r\n\
\r\n\
## Acceptance Criteria\r\n\
\r\n\
- Criterion 1\r\n\
- Criterion 2\r\n\
\r\n\
## Tickets\r\n\
\r\n\
1. j-a1b2\r\n\
2. j-c3d4\r\n\
";

        let metadata = parse_plan_content(content).unwrap();
        assert_eq!(metadata.id, Some("plan-crlf".to_string()));
        assert_eq!(metadata.title, Some("CRLF Plan".to_string()));
        assert_eq!(metadata.acceptance_criteria.len(), 2);
        assert!(metadata.is_simple());
        let tickets = metadata.all_tickets();
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4"]);
    }

    #[test]
    fn test_parse_phased_plan_with_crlf() {
        let content = "---\r\n\
id: plan-crlf-phased\r\n\
uuid: 550e8400-e29b-41d4-a716-446655440013\r\n\
created: 2024-01-01T00:00:00Z\r\n\
---\r\n\
# CRLF Phased Plan\r\n\
\r\n\
Overview.\r\n\
\r\n\
## Phase 1: First Phase\r\n\
\r\n\
First phase description.\r\n\
\r\n\
### Tickets\r\n\
\r\n\
1. j-a1b2\r\n\
2. j-c3d4\r\n\
\r\n\
## Phase 2: Second Phase\r\n\
\r\n\
### Tickets\r\n\
\r\n\
1. j-e5f6\r\n\
";

        let metadata = parse_plan_content(content).unwrap();
        assert_eq!(metadata.title, Some("CRLF Phased Plan".to_string()));
        assert!(metadata.is_phased());
        let phases = metadata.phases();
        assert_eq!(phases.len(), 2);
        assert_eq!(phases[0].number, "1");
        assert_eq!(phases[0].name, "First Phase");
        assert_eq!(phases[0].tickets, vec!["j-a1b2", "j-c3d4"]);
        assert_eq!(phases[1].tickets, vec!["j-e5f6"]);
    }

    #[test]
    fn test_parse_plan_with_mixed_line_endings() {
        let content = "---\n\
id: plan-mixed\n\
uuid: 550e8400-e29b-41d4-a716-446655440012\n\
created: 2024-01-01T00:00:00Z\n\
---\n\
# Mixed Line Endings\r\n\
\r\n\
Mixed line ending content.\r\n\
\r\n\
## Tickets\r\n\
\r\n\
1. j-a1b2\r\n\
2. j-c3d4\n\
3. j-e5f6\r\n\
";

        let metadata = parse_plan_content(content).unwrap();
        assert_eq!(metadata.id, Some("plan-mixed".to_string()));
        assert_eq!(metadata.title, Some("Mixed Line Endings".to_string()));
        let tickets = metadata.all_tickets();
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);
    }

    #[test]
    fn test_parse_plan_missing_required_id_field() {
        let content = r#"---
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# Plan Without ID
"#;

        let result = parse_plan_content(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_plan_missing_required_uuid_field() {
        let content = r#"---
id: plan-test
created: 2024-01-01T00:00:00Z
---
# Plan Without UUID
"#;

        let result = parse_plan_content(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_plan_unknown_field_rejected() {
        let content = r#"---
id: plan-test
uuid: 550e8400-e29b-41d4-a716-446655440000
unknown_field: should_be_rejected
---
# Plan With Unknown Field
"#;

        let result = parse_plan_content(content);
        assert!(result.is_err());
    }
}
