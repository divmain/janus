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

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use comrak::nodes::{AstNode, NodeValue};
use comrak::{parse_document, Arena, Options};
use regex::Regex;
use serde::Deserialize;

use crate::error::{JanusError, Result};
use crate::parser::split_frontmatter;
use crate::plan::types::{FreeFormSection, PlanMetadata, PlanSection, TicketsSection};

// Re-export public functions from submodules
pub use import::{
    is_completed_task, is_phase_header, is_section_alias, parse_importable_plan,
    ACCEPTANCE_CRITERIA_ALIASES, DESIGN_SECTION_NAME, IMPLEMENTATION_SECTION_NAME,
    PHASE_HEADER_REGEX, PHASE_PATTERN,
};
pub use sections::parse_ticket_list;
pub use serialize::serialize_plan;

/// Regex pattern for matching phase headers in regular plan files.
///
/// Only matches "Phase" as the keyword. Headings like "Stage 1: Planning" or
/// "Part 2: Setup" are treated as freeform sections in regular plan files.
/// The broader pattern (stage, part, step) is intentionally limited to the
/// import parser where the user has explicitly chosen to import a structured plan.
const PLAN_FILE_PHASE_PATTERN: &str = r"(?i)^phase\s+(\d+[a-z]?)\s*[-:]?\s*(.*)$";

/// Compiled regex for matching phase headers in regular plan files.
/// Only matches "Phase N: Name" — not "Stage", "Part", or "Step".
static PLAN_FILE_PHASE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(PLAN_FILE_PHASE_PATTERN).expect("plan file phase regex should be valid")
});

/// Tolerant plan frontmatter struct for YAML deserialization.
///
/// All known fields are optional at parse time to allow reading plans that may
/// be missing identity fields (e.g., during migration or manual creation).
/// Unknown fields are captured into `extra` for round-trip preservation, so
/// that external tools or future versions adding new fields won't brick files.
#[derive(Debug, Deserialize)]
struct PlanFrontmatter {
    id: Option<String>,
    uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created: Option<String>,
    /// Unknown/extra YAML keys are captured here for round-trip preservation.
    #[serde(flatten)]
    extra: HashMap<String, serde_yaml_ng::Value>,
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
        .map_err(|e| JanusError::InvalidFormat(format!("YAML parsing error: {e}")))?;

    let metadata = PlanMetadata {
        id: frontmatter.id.map(crate::types::PlanId::new_unchecked),
        uuid: frontmatter.uuid,
        created: frontmatter
            .created
            .map(crate::types::CreatedAt::new_unchecked),
        extra_frontmatter: if frontmatter.extra.is_empty() {
            None
        } else {
            Some(frontmatter.extra)
        },
        ..Default::default()
    };

    Ok(metadata)
}

/// Create comrak Options with the table extension enabled.
///
/// Tables are a commonly used markdown feature that must be parsed as structured
/// nodes (not paragraphs with pipe characters) so they round-trip correctly
/// through `format_commonmark`.
fn comrak_options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options
}

/// Parse the markdown body to extract title, description, and sections
fn parse_body(body: &str, metadata: &mut PlanMetadata) -> Result<()> {
    let arena = Arena::new();
    let options = comrak_options();
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
    let mut h2_sections: Vec<H2Section> = Vec::new();
    let mut current_h3: Option<H3Section> = None;

    for node in root.children() {
        match &node.data.borrow().value {
            NodeValue::Heading(heading) => {
                let heading_text = extract_text_content(node);

                match heading.level {
                    1 => {
                        title = Some(heading_text.trim().to_string());
                    }
                    2 => {
                        // Finalize any pending H3 into the last H2
                        if let Some(h3) = current_h3.take() {
                            if let Some(last_h2) = h2_sections.last_mut() {
                                last_h2.h3_sections.push(h3);
                            }
                        }
                        // Start new H2 section
                        h2_sections.push(H2Section {
                            heading: heading_text.trim().to_string(),
                            content: String::new(),
                            h3_sections: Vec::new(),
                        });
                    }
                    3 => {
                        // Finalize pending H3 into current H2 before starting new one
                        if let Some(h3) = current_h3.take() {
                            if let Some(last_h2) = h2_sections.last_mut() {
                                last_h2.h3_sections.push(h3);
                            }
                        }
                        // Start new H3 subsection
                        current_h3 = Some(H3Section {
                            heading: heading_text.trim().to_string(),
                            content: String::new(),
                        });
                    }
                    _ => {
                        // H4+ treated as content
                        let rendered = render_node_to_markdown(node, options);
                        append_content(&mut h2_sections, &mut current_h3, &mut preamble, &rendered);
                    }
                }
            }
            _ => {
                let rendered = render_node_to_markdown(node, options);
                append_content(&mut h2_sections, &mut current_h3, &mut preamble, &rendered);
            }
        }
    }

    // Finalize any pending H3 into the last H2
    if let Some(h3) = current_h3.take() {
        if let Some(last_h2) = h2_sections.last_mut() {
            last_h2.h3_sections.push(h3);
        }
    }

    let preamble_opt = if preamble.is_empty() {
        None
    } else {
        Some(preamble)
    };

    (title, preamble_opt, h2_sections)
}

/// Append content to the appropriate location based on current parsing state.
fn append_content(
    h2_sections: &mut [H2Section],
    current_h3: &mut Option<H3Section>,
    preamble: &mut String,
    content: &str,
) {
    if let Some(h3) = current_h3 {
        h3.content.push_str(content);
    } else if let Some(last_h2) = h2_sections.last_mut() {
        last_h2.content.push_str(content);
    } else {
        preamble.push_str(content);
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
        metadata.acceptance_criteria = parse_list_items(&section.content);
        // Store raw content for round-trip fidelity — preserves non-list prose
        // (paragraphs, code blocks, etc.) that parse_list_items discards.
        let trimmed_raw = section.content.trim();
        if !trimmed_raw.is_empty() {
            metadata.acceptance_criteria_raw = Some(trimmed_raw.to_string());
        }
        metadata.acceptance_criteria_extra = section
            .h3_sections
            .iter()
            .map(|h3| FreeFormSection {
                heading: h3.heading.clone(),
                content: h3.content.trim().to_string(),
            })
            .collect();
        return;
    }

    // Check for Tickets section (simple plans, case-insensitive, exact match)
    // Only the first occurrence is parsed as structured data
    if heading_lower == "tickets" && seen_sections.insert("tickets") {
        let tickets = sections::parse_ticket_list(&section.content);
        let extra_subsections: Vec<FreeFormSection> = section
            .h3_sections
            .iter()
            .map(|h3| FreeFormSection {
                heading: h3.heading.clone(),
                content: h3.content.trim().to_string(),
            })
            .collect();
        let trimmed_raw = section.content.trim();
        let tickets_raw = if !trimmed_raw.is_empty() {
            Some(trimmed_raw.to_string())
        } else {
            None
        };
        metadata.sections.push(PlanSection::Tickets(TicketsSection {
            tickets,
            tickets_raw,
            extra_subsections,
        }));
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

/// Try to parse a heading as a phase header in regular plan files.
///
/// Only matches "Phase N: Name" variants. Headings like "Stage 1: Planning"
/// or "Part 2: Setup" are treated as freeform sections — the broader pattern
/// is reserved for the import parser where the user has explicitly chosen to
/// import a structured plan.
fn try_parse_phase_header(heading: &str) -> Option<(String, String)> {
    PLAN_FILE_PHASE_REGEX.captures(heading).map(|caps| {
        let number = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
        let name = caps
            .get(2)
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

/// Parse a bullet or numbered list into string items using comrak AST.
///
/// This correctly handles:
/// - Bullet lists (`-`, `*`, `+`)
/// - Numbered lists (`1.`, `2.`)
/// - Task list markers (`- [ ]`, `- [x]`) — the marker is stripped, text preserved
/// - Multiline list items (full text content is extracted)
/// - Code blocks containing list-like text (ignored, since they don't produce list nodes)
/// - Inline formatting (bold, italic, code spans, links) is preserved
pub fn parse_list_items(content: &str) -> Vec<String> {
    let arena = Arena::new();
    let options = comrak_options_with_tasklist();
    let root = parse_document(&arena, content, &options);

    extract_list_items_from_ast(root, &options)
}

/// Create comrak Options with task list extension enabled.
pub(crate) fn comrak_options_with_tasklist() -> Options<'static> {
    let mut options = comrak_options();
    options.extension.tasklist = true;
    options
}

/// Extract list items from a comrak AST node tree.
///
/// Walks top-level `NodeValue::List` nodes and collects text from each
/// `NodeValue::Item` or `NodeValue::TaskItem` child. Inline formatting
/// (bold, italic, code spans, links, etc.) is preserved by rendering
/// each item's children back to markdown.
fn extract_list_items_from_ast<'a>(root: &'a AstNode<'a>, options: &Options) -> Vec<String> {
    let mut items = Vec::new();

    for node in root.children() {
        if let NodeValue::List(_) = &node.data.borrow().value {
            for child in node.children() {
                match &child.data.borrow().value {
                    NodeValue::Item(_) | NodeValue::TaskItem(_) => {
                        // Render each child node (typically Paragraph) of the list
                        // item to markdown, preserving inline formatting. This avoids
                        // rendering the list marker itself (e.g. "- " or "1. ").
                        let text: String = child
                            .children()
                            .map(|c| render_node_to_markdown(c, options))
                            .collect::<Vec<_>>()
                            .join("\n")
                            .trim()
                            .to_string();
                        if !text.is_empty() {
                            items.push(text);
                        }
                    }
                    _ => {}
                }
            }
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

        assert_eq!(metadata.id.as_deref(), Some("plan-a1b2"));
        assert_eq!(
            metadata.uuid,
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
        assert_eq!(metadata.created.as_deref(), Some("2024-01-01T00:00:00Z"));
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

        // Stage, Part, Step should NOT match in regular plan file parsing
        let result = try_parse_phase_header("Stage 1: Planning");
        assert!(
            result.is_none(),
            "Stage should not match in plan file parser"
        );

        let result = try_parse_phase_header("Part 2: Implementation");
        assert!(
            result.is_none(),
            "Part should not match in plan file parser"
        );

        let result = try_parse_phase_header("Step 3: Testing");
        assert!(
            result.is_none(),
            "Step should not match in plan file parser"
        );
    }

    #[test]
    fn test_stage_heading_is_freeform_in_plan_files() {
        let content = r#"---
id: plan-stage
uuid: 550e8400-e29b-41d4-a716-446655440030
created: 2024-01-01T00:00:00Z
---
# Plan with Stage Headings

## Stage 1: Planning

This describes the planning stage.

## Phase 1: Implementation

### Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(content).unwrap();

        // "Stage 1: Planning" should be treated as freeform
        let freeform = metadata.free_form_sections();
        assert_eq!(freeform.len(), 1);
        assert_eq!(freeform[0].heading, "Stage 1: Planning");
        assert!(freeform[0].content.contains("planning stage"));

        // "Phase 1: Implementation" should still be recognized as a phase
        let phases = metadata.phases();
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].number, "1");
        assert_eq!(phases[0].name, "Implementation");
    }

    #[test]
    fn test_part_and_step_headings_are_freeform_in_plan_files() {
        let content = r#"---
id: plan-parts
uuid: 550e8400-e29b-41d4-a716-446655440031
created: 2024-01-01T00:00:00Z
---
# Plan with Part and Step Headings

## Part 1: Background

Background information.

## Step 2: Prerequisites

Prerequisite steps.

## Phase 1: Actual Phase

### Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(content).unwrap();

        // Part and Step should be freeform
        let freeform = metadata.free_form_sections();
        assert_eq!(freeform.len(), 2);
        assert_eq!(freeform[0].heading, "Part 1: Background");
        assert_eq!(freeform[1].heading, "Step 2: Prerequisites");

        // Only Phase should be a phase
        let phases = metadata.phases();
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].number, "1");
        assert_eq!(phases[0].name, "Actual Phase");
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
        assert_eq!(metadata.id.as_deref(), Some("plan-min"));
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
        assert_eq!(metadata.id.as_deref(), Some("plan-test"));
        assert_eq!(metadata.uuid, Some("abc-123".to_string()));
        assert_eq!(metadata.created.as_deref(), Some("2024-01-01"));
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
        assert_eq!(metadata.id.as_deref(), Some("plan-cache"));
        assert_eq!(
            metadata.uuid,
            Some("7c9e6679-7425-40de-944b-e07fc1f90ae7".to_string())
        );
        assert_eq!(metadata.created.as_deref(), Some("2024-01-15T10:30:00Z"));

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
        assert_eq!(metadata.id.as_deref(), Some("plan-crlf"));
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
        assert_eq!(metadata.id.as_deref(), Some("plan-mixed"));
        assert_eq!(metadata.title, Some("Mixed Line Endings".to_string()));
        let tickets = metadata.all_tickets();
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);
    }

    #[test]
    fn test_parse_plan_missing_id_field_succeeds() {
        let content = r#"---
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# Plan Without ID
"#;

        let metadata = parse_plan_content(content).unwrap();
        assert!(metadata.id.is_none());
        assert_eq!(
            metadata.uuid,
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
        assert_eq!(metadata.title, Some("Plan Without ID".to_string()));
    }

    #[test]
    fn test_parse_plan_missing_uuid_field_succeeds() {
        let content = r#"---
id: plan-test
created: 2024-01-01T00:00:00Z
---
# Plan Without UUID
"#;

        let metadata = parse_plan_content(content).unwrap();
        assert_eq!(metadata.id.as_deref(), Some("plan-test"));
        assert!(metadata.uuid.is_none());
    }

    #[test]
    fn test_parse_plan_unknown_field_preserved() {
        let content = r#"---
id: plan-test
uuid: 550e8400-e29b-41d4-a716-446655440000
custom_tool_field: some_value
---
# Plan With Unknown Field
"#;

        let metadata = parse_plan_content(content).unwrap();
        assert_eq!(metadata.id.as_deref(), Some("plan-test"));
        assert_eq!(
            metadata.uuid,
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );

        // Unknown field is captured in extra_frontmatter
        let extra = metadata.extra_frontmatter.as_ref().unwrap();
        assert_eq!(
            extra.get("custom_tool_field").and_then(|v| v.as_str()),
            Some("some_value")
        );
    }

    #[test]
    fn test_parse_plan_unknown_fields_roundtrip() {
        let content = r#"---
id: plan-extra
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
custom_tool: external-tool-v2
priority_override: 5
---
# Plan With Extra Fields

## Tickets

1. j-a1b2
"#;

        // Parse
        let metadata = parse_plan_content(content).unwrap();
        assert_eq!(metadata.id.as_deref(), Some("plan-extra"));

        let extra = metadata.extra_frontmatter.as_ref().unwrap();
        assert_eq!(extra.len(), 2);
        assert_eq!(
            extra.get("custom_tool").and_then(|v| v.as_str()),
            Some("external-tool-v2")
        );

        // Serialize and re-parse to verify round-trip
        let serialized = crate::plan::parser::serialize::serialize_plan(&metadata);
        assert!(serialized.contains("custom_tool: external-tool-v2"));
        assert!(serialized.contains("priority_override: 5"));

        let reparsed = parse_plan_content(&serialized).unwrap();
        assert_eq!(reparsed.id, metadata.id);
        let reparsed_extra = reparsed.extra_frontmatter.as_ref().unwrap();
        assert_eq!(
            reparsed_extra.get("custom_tool").and_then(|v| v.as_str()),
            Some("external-tool-v2")
        );
    }

    #[test]
    fn test_parse_plan_no_identity_fields() {
        // A plan with no id, no uuid — just created timestamp
        let content = r#"---
created: 2024-01-01T00:00:00Z
---
# Bare Plan
"#;

        let metadata = parse_plan_content(content).unwrap();
        assert!(metadata.id.is_none());
        assert!(metadata.uuid.is_none());
        assert_eq!(metadata.created.as_deref(), Some("2024-01-01T00:00:00Z"));
        assert_eq!(metadata.title, Some("Bare Plan".to_string()));
    }

    #[test]
    fn test_parse_plan_empty_frontmatter() {
        // Completely empty frontmatter should return an error
        let content = "---\n---\n# Empty Frontmatter Plan\n";

        let result = parse_plan_content(content);
        assert!(result.is_err());
        match result {
            Err(JanusError::EmptyFrontmatter) => {}
            other => panic!("Expected EmptyFrontmatter error, got: {other:?}"),
        }
    }

    #[test]
    fn test_parse_plan_no_extra_fields_has_none() {
        // Normal plan without extra fields should have extra_frontmatter = None
        let content = r#"---
id: plan-normal
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# Normal Plan
"#;

        let metadata = parse_plan_content(content).unwrap();
        assert!(metadata.extra_frontmatter.is_none());
    }

    #[test]
    fn test_parse_acceptance_criteria_preserves_inline_formatting() {
        let content = r#"---
id: plan-fmt
uuid: 550e8400-e29b-41d4-a716-446655440090
created: 2024-01-01T00:00:00Z
---
# Inline Formatting Plan

## Acceptance Criteria

- Performance must be **under 5ms** per lookup
- Use the `TicketStore` API for all queries
- See [design doc](https://example.com) for details
- Must support *italic* and **bold** text
- Combine `code`, **bold**, and *italic* in one item

## Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(content).unwrap();
        assert_eq!(metadata.acceptance_criteria.len(), 5);

        // Bold formatting preserved
        assert!(
            metadata.acceptance_criteria[0].contains("**under 5ms**"),
            "Expected bold formatting preserved, got: {}",
            metadata.acceptance_criteria[0]
        );

        // Code span preserved
        assert!(
            metadata.acceptance_criteria[1].contains("`TicketStore`"),
            "Expected code span preserved, got: {}",
            metadata.acceptance_criteria[1]
        );

        // Link preserved
        assert!(
            metadata.acceptance_criteria[2].contains("[design doc](https://example.com)"),
            "Expected link preserved, got: {}",
            metadata.acceptance_criteria[2]
        );

        // Italic preserved
        assert!(
            metadata.acceptance_criteria[3].contains("*italic*"),
            "Expected italic preserved, got: {}",
            metadata.acceptance_criteria[3]
        );

        // Mixed formatting preserved
        assert!(
            metadata.acceptance_criteria[4].contains("`code`"),
            "Expected code span preserved in mixed item, got: {}",
            metadata.acceptance_criteria[4]
        );
        assert!(
            metadata.acceptance_criteria[4].contains("**bold**"),
            "Expected bold preserved in mixed item, got: {}",
            metadata.acceptance_criteria[4]
        );
        assert!(
            metadata.acceptance_criteria[4].contains("*italic*"),
            "Expected italic preserved in mixed item, got: {}",
            metadata.acceptance_criteria[4]
        );
    }

    // ==================== AST-Based List Parsing Regression Tests ====================

    #[test]
    fn test_parse_plan_task_list_tickets() {
        // Task list markers (- [ ] and - [x]) should be recognized as ticket IDs
        let content = r#"---
id: plan-task
uuid: 550e8400-e29b-41d4-a716-446655440020
created: 2024-01-01T00:00:00Z
---
# Task List Plan

## Tickets

- [ ] j-a1b2
- [x] j-c3d4
- [ ] j-e5f6
"#;

        let metadata = parse_plan_content(content).unwrap();
        assert!(metadata.is_simple());
        let tickets = metadata.all_tickets();
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);
    }

    #[test]
    fn test_parse_plan_task_list_acceptance_criteria() {
        // Task list markers in acceptance criteria should extract the text
        let content = r#"---
id: plan-taskcrit
uuid: 550e8400-e29b-41d4-a716-446655440021
created: 2024-01-01T00:00:00Z
---
# Task List Criteria Plan

## Acceptance Criteria

- [ ] All tests pass
- [x] Documentation complete
- [ ] Performance targets met

## Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(content).unwrap();
        assert_eq!(metadata.acceptance_criteria.len(), 3);
        assert_eq!(metadata.acceptance_criteria[0], "All tests pass");
        assert_eq!(metadata.acceptance_criteria[1], "Documentation complete");
        assert_eq!(metadata.acceptance_criteria[2], "Performance targets met");
    }

    #[test]
    fn test_parse_plan_code_block_with_list_like_content() {
        // List-like text inside code blocks must NOT be parsed as tickets or criteria
        let content = r#"---
id: plan-codeblock
uuid: 550e8400-e29b-41d4-a716-446655440022
created: 2024-01-01T00:00:00Z
---
# Code Block Immunity Plan

## Acceptance Criteria

- Real criterion 1
- Real criterion 2

## Phase 1: Setup

Here's an example ticket list format:

```markdown
- j-fake1 This is in a code block
- j-fake2 Also in a code block
1. j-fake3 Numbered inside code block
```

### Tickets

1. j-real1
2. j-real2
"#;

        let metadata = parse_plan_content(content).unwrap();

        // Acceptance criteria should only have the real ones
        assert_eq!(metadata.acceptance_criteria.len(), 2);
        assert_eq!(metadata.acceptance_criteria[0], "Real criterion 1");
        assert_eq!(metadata.acceptance_criteria[1], "Real criterion 2");

        // Tickets should only have the real ones, not those inside code blocks
        let phases = metadata.phases();
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].tickets, vec!["j-real1", "j-real2"]);
    }

    #[test]
    fn test_parse_plan_phased_with_task_list_tickets() {
        // Phased plan using task list markers for tickets
        let content = r#"---
id: plan-phased-task
uuid: 550e8400-e29b-41d4-a716-446655440023
created: 2024-01-01T00:00:00Z
---
# Phased Task List Plan

## Phase 1: Infrastructure

### Success Criteria

- [ ] Database tables created
- [x] Helper functions work

### Tickets

- [ ] j-a1b2
- [x] j-c3d4

## Phase 2: Implementation

### Tickets

- [ ] j-e5f6 - Implement core logic
"#;

        let metadata = parse_plan_content(content).unwrap();
        assert!(metadata.is_phased());

        let phases = metadata.phases();
        assert_eq!(phases.len(), 2);

        // Phase 1 success criteria should include task list text
        assert_eq!(phases[0].success_criteria.len(), 2);
        assert_eq!(phases[0].success_criteria[0], "Database tables created");
        assert_eq!(phases[0].success_criteria[1], "Helper functions work");

        // Phase 1 tickets
        assert_eq!(phases[0].tickets, vec!["j-a1b2", "j-c3d4"]);

        // Phase 2 tickets (with description after ID)
        assert_eq!(phases[1].tickets, vec!["j-e5f6"]);
    }

    #[test]
    fn test_parse_plan_multiline_acceptance_criteria() {
        // Multiline list items should be fully captured
        let content = r#"---
id: plan-multiline
uuid: 550e8400-e29b-41d4-a716-446655440024
created: 2024-01-01T00:00:00Z
---
# Multiline List Items Plan

## Acceptance Criteria

- First criterion with
  continuation on next line
- Second criterion
- Third criterion spanning
  multiple lines of text

## Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(content).unwrap();
        assert_eq!(metadata.acceptance_criteria.len(), 3);
        // Multiline items should have their continuation text included
        assert!(metadata.acceptance_criteria[0].contains("First criterion"));
        assert!(metadata.acceptance_criteria[0].contains("continuation"));
        assert_eq!(metadata.acceptance_criteria[1], "Second criterion");
        assert!(metadata.acceptance_criteria[2].contains("Third criterion"));
        assert!(metadata.acceptance_criteria[2].contains("multiple lines"));
    }

    #[test]
    fn test_parse_success_criteria_preserves_inline_formatting() {
        let content = r#"---
id: plan-fmt-sc
uuid: 550e8400-e29b-41d4-a716-446655440091
created: 2024-01-01T00:00:00Z
---
# Success Criteria Formatting Plan

## Phase 1: Infrastructure

### Success Criteria

- Database responds in **under 10ms**
- The `cache_init()` function returns `Ok`
- See [RFC-42](https://example.com/rfc42) for schema details
- All *critical* paths are tested

### Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(content).unwrap();
        let phases = metadata.phases();
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].success_criteria.len(), 4);

        // Bold preserved
        assert!(
            phases[0].success_criteria[0].contains("**under 10ms**"),
            "Expected bold formatting preserved, got: {}",
            phases[0].success_criteria[0]
        );

        // Code spans preserved
        assert!(
            phases[0].success_criteria[1].contains("`cache_init()`"),
            "Expected code span preserved, got: {}",
            phases[0].success_criteria[1]
        );
        assert!(
            phases[0].success_criteria[1].contains("`Ok`"),
            "Expected code span preserved, got: {}",
            phases[0].success_criteria[1]
        );

        // Link preserved
        assert!(
            phases[0].success_criteria[2].contains("[RFC-42](https://example.com/rfc42)"),
            "Expected link preserved, got: {}",
            phases[0].success_criteria[2]
        );

        // Italic preserved
        assert!(
            phases[0].success_criteria[3].contains("*critical*"),
            "Expected italic preserved, got: {}",
            phases[0].success_criteria[3]
        );
    }

    // ==================== Unknown Phase H3 Subsection Tests ====================

    #[test]
    fn test_parse_phase_with_unknown_h3_subsections() {
        let content = r#"---
id: plan-unk-h3
uuid: 550e8400-e29b-41d4-a716-446655440030
created: 2024-01-01T00:00:00Z
---
# Plan with Unknown Phase Subsections

## Phase 1: Infrastructure

Set up the foundational components.

### Success Criteria

- Database tables created

### Implementation Notes

These are custom notes the user added to the phase.
They contain useful context that must not be lost.

### Tickets

1. j-a1b2
2. j-c3d4

### Risk Assessment

- Risk 1: Performance under load
- Risk 2: Backwards compatibility
"#;

        let metadata = parse_plan_content(content).unwrap();

        assert!(metadata.is_phased());

        let phases = metadata.phases();
        assert_eq!(phases.len(), 1);

        // Known subsections parsed correctly
        assert_eq!(phases[0].success_criteria, vec!["Database tables created"]);
        assert_eq!(phases[0].tickets, vec!["j-a1b2", "j-c3d4"]);

        // Unknown subsections preserved
        assert_eq!(phases[0].extra_subsections.len(), 2);
        assert_eq!(
            phases[0].extra_subsections[0].heading,
            "Implementation Notes"
        );
        assert!(phases[0].extra_subsections[0]
            .content
            .contains("custom notes"));
        assert!(phases[0].extra_subsections[0]
            .content
            .contains("must not be lost"));
        assert_eq!(phases[0].extra_subsections[1].heading, "Risk Assessment");
        assert!(phases[0].extra_subsections[1]
            .content
            .contains("Performance under load"));

        // Subsection order tracks all H3s
        assert_eq!(
            phases[0].subsection_order,
            vec![
                "success criteria",
                "implementation notes",
                "tickets",
                "risk assessment"
            ]
        );
    }

    #[test]
    fn test_parse_phase_unknown_h3_with_code_blocks() {
        let content = r#"---
id: plan-unk-code
uuid: 550e8400-e29b-41d4-a716-446655440031
created: 2024-01-01T00:00:00Z
---
# Plan with Code in Custom Subsection

## Phase 1: Implementation

### API Examples

```rust
fn example() {
    println!("This should be preserved");
}
```

### Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(content).unwrap();
        let phases = metadata.phases();

        assert_eq!(phases[0].extra_subsections.len(), 1);
        assert_eq!(phases[0].extra_subsections[0].heading, "API Examples");
        assert!(phases[0].extra_subsections[0]
            .content
            .contains("fn example()"));
        assert!(phases[0].extra_subsections[0]
            .content
            .contains("should be preserved"));
    }

    // ==================== Tickets Section H3 Subsection Tests ====================

    #[test]
    fn test_parse_simple_plan_with_h3_under_tickets() {
        let content = r#"---
id: plan-tkt-h3
uuid: 550e8400-e29b-41d4-a716-446655440040
created: 2024-01-01T00:00:00Z
---
# Simple Plan with Ticket Subsections

## Tickets

1. j-a1b2
2. j-c3d4

### Ordering Notes

Ticket j-a1b2 must be completed before j-c3d4 because of API dependency.

### Risk Assessment

- Risk 1: Timeline pressure
- Risk 2: External dependency
"#;

        let metadata = parse_plan_content(content).unwrap();

        assert!(metadata.is_simple());
        assert!(!metadata.is_phased());

        // Tickets are parsed correctly
        let tickets = metadata.all_tickets();
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4"]);

        // H3 subsections are captured
        if let PlanSection::Tickets(ts) = &metadata.sections[0] {
            assert_eq!(ts.extra_subsections.len(), 2);
            assert_eq!(ts.extra_subsections[0].heading, "Ordering Notes");
            assert!(ts.extra_subsections[0].content.contains("API dependency"));
            assert_eq!(ts.extra_subsections[1].heading, "Risk Assessment");
            assert!(ts.extra_subsections[1]
                .content
                .contains("Timeline pressure"));
        } else {
            panic!("Expected PlanSection::Tickets");
        }
    }

    #[test]
    fn test_parse_simple_plan_tickets_no_h3_still_works() {
        let content = r#"---
id: plan-tkt-noh3
uuid: 550e8400-e29b-41d4-a716-446655440041
created: 2024-01-01T00:00:00Z
---
# Simple Plan without Ticket Subsections

## Tickets

1. j-a1b2
2. j-c3d4
"#;

        let metadata = parse_plan_content(content).unwrap();

        assert!(metadata.is_simple());
        let tickets = metadata.all_tickets();
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4"]);

        // No extra subsections
        if let PlanSection::Tickets(ts) = &metadata.sections[0] {
            assert!(ts.extra_subsections.is_empty());
        } else {
            panic!("Expected PlanSection::Tickets");
        }
    }

    // ==================== Acceptance Criteria H3 Subsection Tests ====================

    #[test]
    fn test_parse_acceptance_criteria_with_h3_subsections() {
        let content = r#"---
id: plan-ac-h3
uuid: 550e8400-e29b-41d4-a716-446655440050
created: 2024-01-01T00:00:00Z
---
# Plan with AC Subsections

## Acceptance Criteria

- All tests pass
- Documentation complete

### Testing Notes

Detailed testing instructions here...

Run the full integration suite before merging.

### Verification Steps

1. Deploy to staging
2. Run smoke tests
3. Check metrics

## Tickets

1. j-a1b2
2. j-c3d4
"#;

        let metadata = parse_plan_content(content).unwrap();

        // Acceptance criteria list items parsed correctly
        assert_eq!(metadata.acceptance_criteria.len(), 2);
        assert_eq!(metadata.acceptance_criteria[0], "All tests pass");
        assert_eq!(metadata.acceptance_criteria[1], "Documentation complete");

        // H3 subsections captured
        assert_eq!(metadata.acceptance_criteria_extra.len(), 2);
        assert_eq!(
            metadata.acceptance_criteria_extra[0].heading,
            "Testing Notes"
        );
        assert!(metadata.acceptance_criteria_extra[0]
            .content
            .contains("Detailed testing instructions"));
        assert!(metadata.acceptance_criteria_extra[0]
            .content
            .contains("integration suite"));
        assert_eq!(
            metadata.acceptance_criteria_extra[1].heading,
            "Verification Steps"
        );
        assert!(metadata.acceptance_criteria_extra[1]
            .content
            .contains("Deploy to staging"));
    }

    #[test]
    fn test_parse_acceptance_criteria_no_h3_still_works() {
        let content = r#"---
id: plan-ac-noh3
uuid: 550e8400-e29b-41d4-a716-446655440051
created: 2024-01-01T00:00:00Z
---
# Plan without AC Subsections

## Acceptance Criteria

- All tests pass
- Documentation complete

## Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(content).unwrap();

        assert_eq!(metadata.acceptance_criteria.len(), 2);
        assert!(metadata.acceptance_criteria_extra.is_empty());
    }

    #[test]
    fn test_parse_acceptance_criteria_h3_with_code_blocks() {
        let content = r#"---
id: plan-ac-code
uuid: 550e8400-e29b-41d4-a716-446655440052
created: 2024-01-01T00:00:00Z
---
# Plan with Code in AC Subsection

## Acceptance Criteria

- API responds correctly

### Example Responses

```json
{
    "status": "ok",
    "data": []
}
```

## Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(content).unwrap();

        assert_eq!(metadata.acceptance_criteria.len(), 1);
        assert_eq!(metadata.acceptance_criteria_extra.len(), 1);
        assert_eq!(
            metadata.acceptance_criteria_extra[0].heading,
            "Example Responses"
        );
        assert!(metadata.acceptance_criteria_extra[0]
            .content
            .contains("\"status\": \"ok\""));
    }

    // ==================== Table Round-Trip Tests ====================

    #[test]
    fn test_roundtrip_table_in_freeform_section() {
        let original = r#"---
id: plan-table
uuid: 550e8400-e29b-41d4-a716-446655440060
created: 2024-01-01T00:00:00Z
---
# Plan with Table

## Performance Benchmarks

| Operation | Before | After |
|-----------|--------|-------|
| Single ticket lookup | ~500ms | <5ms |
| Full list | ~1-5s | ~25-50ms |

## Tickets

1. j-a1b2
"#;

        // Parse
        let metadata = parse_plan_content(original).unwrap();

        let freeform = metadata.free_form_sections();
        assert_eq!(freeform.len(), 1);
        assert_eq!(freeform[0].heading, "Performance Benchmarks");
        // Table content should be present
        assert!(freeform[0].content.contains("Operation"));
        assert!(freeform[0].content.contains("Single ticket lookup"));

        // Serialize
        let serialized = serialize_plan(&metadata);

        // The serialized output should contain the table
        assert!(serialized.contains("Operation"));
        assert!(serialized.contains("Single ticket lookup"));
        assert!(serialized.contains("<5ms"));

        // Re-parse and verify round-trip
        let reparsed = parse_plan_content(&serialized).unwrap();

        let new_freeform = reparsed.free_form_sections();
        assert_eq!(new_freeform.len(), 1);
        assert_eq!(new_freeform[0].heading, "Performance Benchmarks");
        assert!(new_freeform[0].content.contains("Operation"));
        assert!(new_freeform[0].content.contains("Single ticket lookup"));
        assert!(new_freeform[0].content.contains("<5ms"));

        // Verify tickets still parse correctly
        assert_eq!(reparsed.all_tickets(), vec!["j-a1b2"]);
    }

    #[test]
    fn test_roundtrip_table_in_phase_description() {
        let original = r#"---
id: plan-table-phase
uuid: 550e8400-e29b-41d4-a716-446655440061
created: 2024-01-01T00:00:00Z
---
# Plan with Table in Phase

## Phase 1: Schema Design

The following schema will be used:

| Column | Type | Description |
|--------|------|-------------|
| id | TEXT | Primary key |
| name | TEXT | Display name |
| status | TEXT | Current status |

### Tickets

1. j-a1b2
2. j-c3d4
"#;

        // Parse
        let metadata = parse_plan_content(original).unwrap();

        let phases = metadata.phases();
        assert_eq!(phases.len(), 1);
        let desc = phases[0].description.as_ref().unwrap();
        assert!(desc.contains("schema will be used"));
        assert!(desc.contains("Column"));
        assert!(desc.contains("Primary key"));

        // Serialize
        let serialized = serialize_plan(&metadata);

        // Re-parse and verify round-trip
        let reparsed = parse_plan_content(&serialized).unwrap();

        let new_phases = reparsed.phases();
        assert_eq!(new_phases.len(), 1);
        let new_desc = new_phases[0].description.as_ref().unwrap();
        assert!(new_desc.contains("schema will be used"));
        assert!(new_desc.contains("Column"));
        assert!(new_desc.contains("Primary key"));
        assert!(new_desc.contains("Display name"));
        assert_eq!(new_phases[0].tickets, vec!["j-a1b2", "j-c3d4"]);
    }
}
