//! Plan file parser
//!
//! Parses plan markdown files into `PlanMetadata`, handling both structured
//! sections (phases, tickets, acceptance criteria) and free-form sections.
//!
//! Uses the comrak crate for AST-based markdown parsing to correctly handle
//! edge cases like code blocks containing `##` characters.

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};
use regex::Regex;

use crate::error::{JanusError, Result};
use crate::plan_types::{FreeFormSection, Phase, PlanMetadata, PlanSection};

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
    // Split frontmatter from body
    let (yaml, body) = split_frontmatter(content)?;

    // Parse YAML frontmatter
    let mut metadata = parse_yaml_frontmatter(yaml)?;

    // Parse the markdown body
    parse_body(body, &mut metadata)?;

    Ok(metadata)
}

/// Split content into YAML frontmatter and markdown body
fn split_frontmatter(content: &str) -> Result<(&str, &str)> {
    let frontmatter_re = Regex::new(r"(?s)^---\n(.*?)\n---\n(.*)$").unwrap();

    let captures = frontmatter_re
        .captures(content)
        .ok_or_else(|| JanusError::InvalidFormat("missing YAML frontmatter".to_string()))?;

    let yaml = captures.get(1).map(|m| m.as_str()).unwrap_or("");
    let body = captures.get(2).map(|m| m.as_str()).unwrap_or("");

    Ok((yaml, body))
}

/// Parse YAML frontmatter into PlanMetadata fields
fn parse_yaml_frontmatter(yaml: &str) -> Result<PlanMetadata> {
    let mut metadata = PlanMetadata::default();

    let line_re = Regex::new(r"^(\w[-\w]*):\s*(.*)$").unwrap();

    for line in yaml.lines() {
        if let Some(caps) = line_re.captures(line) {
            let key = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let value = caps.get(2).map(|m| m.as_str()).unwrap_or("");

            match key {
                "id" => metadata.id = Some(value.to_string()),
                "uuid" => metadata.uuid = Some(value.to_string()),
                "created" => metadata.created = Some(value.to_string()),
                _ => {} // Ignore unknown fields
            }
        }
    }

    Ok(metadata)
}

/// Parse the markdown body to extract title, description, and sections
fn parse_body(body: &str, metadata: &mut PlanMetadata) -> Result<()> {
    // Use comrak to parse markdown into AST
    let arena = Arena::new();
    let options = Options::default();
    let root = parse_document(&arena, body, &options);

    // Extract content by walking the AST
    let mut current_h2: Option<H2Section> = None;
    let mut current_h3: Option<H3Section> = None;
    let mut preamble_content = String::new();
    let mut in_preamble = true;
    let mut found_acceptance_criteria = false;
    let mut found_tickets_section = false;

    for node in root.children() {
        match &node.data.borrow().value {
            NodeValue::Heading(heading) => {
                let level = heading.level;
                let heading_text = extract_text_content(node);

                match level {
                    1 => {
                        // H1: Plan title
                        metadata.title = Some(heading_text.trim().to_string());
                    }
                    2 => {
                        // End preamble
                        in_preamble = false;

                        // Finalize any pending H3 section
                        if let Some(h3) = current_h3.take()
                            && let Some(ref mut h2) = current_h2
                        {
                            h2.h3_sections.push(h3);
                        }

                        // Finalize any pending H2 section
                        if let Some(h2) = current_h2.take() {
                            process_h2_section(
                                h2,
                                metadata,
                                &mut found_acceptance_criteria,
                                &mut found_tickets_section,
                            );
                        }

                        // Start new H2 section
                        current_h2 = Some(H2Section {
                            heading: heading_text.trim().to_string(),
                            content: String::new(),
                            h3_sections: Vec::new(),
                        });
                    }
                    3 => {
                        // Finalize any pending H3 section
                        if let Some(h3) = current_h3.take()
                            && let Some(ref mut h2) = current_h2
                        {
                            h2.h3_sections.push(h3);
                        }

                        // Start new H3 section
                        current_h3 = Some(H3Section {
                            heading: heading_text.trim().to_string(),
                            content: String::new(),
                        });
                    }
                    _ => {
                        // H4+ are treated as content
                        let rendered = render_node_to_markdown(node, &options);
                        if let Some(ref mut h3) = current_h3 {
                            h3.content.push_str(&rendered);
                        } else if let Some(ref mut h2) = current_h2 {
                            h2.content.push_str(&rendered);
                        } else if in_preamble {
                            preamble_content.push_str(&rendered);
                        }
                    }
                }
            }
            _ => {
                // Non-heading content
                let rendered = render_node_to_markdown(node, &options);

                if let Some(ref mut h3) = current_h3 {
                    h3.content.push_str(&rendered);
                } else if let Some(ref mut h2) = current_h2 {
                    h2.content.push_str(&rendered);
                } else if in_preamble {
                    preamble_content.push_str(&rendered);
                }
            }
        }
    }

    // Finalize any pending sections
    if let Some(h3) = current_h3.take()
        && let Some(ref mut h2) = current_h2
    {
        h2.h3_sections.push(h3);
    }
    if let Some(h2) = current_h2.take() {
        process_h2_section(
            h2,
            metadata,
            &mut found_acceptance_criteria,
            &mut found_tickets_section,
        );
    }

    // Set description from preamble
    let preamble_trimmed = preamble_content.trim();
    if !preamble_trimmed.is_empty() {
        metadata.description = Some(preamble_trimmed.to_string());
    }

    Ok(())
}

/// Temporary structure for collecting H2 section content
struct H2Section {
    heading: String,
    content: String,
    h3_sections: Vec<H3Section>,
}

/// Temporary structure for collecting H3 section content
struct H3Section {
    heading: String,
    content: String,
}

/// Process a collected H2 section and add it to metadata
fn process_h2_section(
    section: H2Section,
    metadata: &mut PlanMetadata,
    found_acceptance_criteria: &mut bool,
    found_tickets_section: &mut bool,
) {
    let heading_lower = section.heading.to_lowercase();

    // Check for Acceptance Criteria (case-insensitive, exact match)
    if heading_lower == "acceptance criteria" && !*found_acceptance_criteria {
        *found_acceptance_criteria = true;
        metadata.acceptance_criteria = parse_list_items(&section.content);
        return;
    }

    // Check for Tickets section (simple plans, case-insensitive, exact match)
    if heading_lower == "tickets" && !*found_tickets_section {
        *found_tickets_section = true;
        let tickets = parse_ticket_list(&section.content);
        metadata.sections.push(PlanSection::Tickets(tickets));
        return;
    }

    // Check for Phase header: "Phase N: Name" or "Phase N - Name"
    if let Some(phase) = try_parse_phase_header(&section.heading) {
        let parsed_phase = parse_phase_content(phase, &section);
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

/// Try to parse a heading as a phase header
/// Matches: "Phase 1: Name", "Phase 2a - Name", "Phase 10:", "Phase 1" (no separator)
fn try_parse_phase_header(heading: &str) -> Option<(String, String)> {
    // Pattern: "Phase" followed by number/letter combo, optional separator and name
    let phase_re = Regex::new(r"(?i)^phase\s+(\d+[a-z]?)\s*(?:[-:]\s*)?(.*)$").unwrap();

    phase_re.captures(heading).map(|caps| {
        let number = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
        let name = caps
            .get(2)
            .map(|m| m.as_str().trim())
            .unwrap_or("")
            .to_string();
        (number, name)
    })
}

/// Parse phase content from an H2Section
fn parse_phase_content(phase_info: (String, String), section: &H2Section) -> Phase {
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

/// Parse a bullet or numbered list into string items
fn parse_list_items(content: &str) -> Vec<String> {
    let mut items = Vec::new();

    // Match bullet points (-, *, +) or numbered lists (1., 2., etc.)
    let item_re = Regex::new(r"(?m)^[\s]*[-*+][\s]+(.+)$|^[\s]*\d+\.[\s]+(.+)$").unwrap();

    for caps in item_re.captures_iter(content) {
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

/// Parse a ticket list, extracting just the ticket IDs
/// Handles formats like:
/// - "1. j-a1b2"
/// - "1. j-a1b2 - Some description"
/// - "1. j-a1b2 (optional note)"
pub fn parse_ticket_list(content: &str) -> Vec<String> {
    let mut tickets = Vec::new();

    // Match numbered or bullet list items, extract first word (the ticket ID)
    // Ticket ID pattern: word chars and hyphens (e.g., j-a1b2, plan-c3d4)
    let item_re = Regex::new(r"(?m)^[\s]*(?:[-*+]|\d+\.)\s+([\w-]+)").unwrap();

    for caps in item_re.captures_iter(content) {
        if let Some(id_match) = caps.get(1) {
            let id = id_match.as_str().to_string();
            if !id.is_empty() {
                tickets.push(id);
            }
        }
    }

    tickets
}

/// Extract plain text content from a node and its children
fn extract_text_content<'a>(node: &'a AstNode<'a>) -> String {
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
fn render_node_to_markdown<'a>(node: &'a AstNode<'a>, options: &Options) -> String {
    let mut output = Vec::new();
    comrak::format_commonmark(node, options, &mut output).unwrap_or_default();
    String::from_utf8_lossy(&output).to_string()
}

// ============================================================================
// Serialization Functions
// ============================================================================

/// Serialize a PlanMetadata back to markdown format for writing to disk.
///
/// The output format is:
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
/// ... (or ## Tickets for simple plans)
/// ```
///
/// Note: Exact whitespace/formatting from the original document is **not**
/// guaranteed to be preserved. The goal is **information preservation**.
pub fn serialize_plan(metadata: &PlanMetadata) -> String {
    let mut output = String::new();

    // 1. Generate YAML frontmatter
    output.push_str("---\n");
    if let Some(ref id) = metadata.id {
        output.push_str(&format!("id: {}\n", id));
    }
    if let Some(ref uuid) = metadata.uuid {
        output.push_str(&format!("uuid: {}\n", uuid));
    }
    if let Some(ref created) = metadata.created {
        output.push_str(&format!("created: {}\n", created));
    }
    output.push_str("---\n");

    // 2. Generate H1 title
    if let Some(ref title) = metadata.title {
        output.push_str(&format!("# {}\n", title));
    }

    // 3. Generate description (preamble)
    if let Some(ref description) = metadata.description {
        output.push('\n');
        output.push_str(description);
        output.push('\n');
    }

    // 4. Generate Acceptance Criteria section if present
    if !metadata.acceptance_criteria.is_empty() {
        output.push_str("\n## Acceptance Criteria\n\n");
        for criterion in &metadata.acceptance_criteria {
            output.push_str(&format!("- {}\n", criterion));
        }
    }

    // 5. Generate sections in stored order
    for section in &metadata.sections {
        output.push('\n');
        match section {
            PlanSection::Phase(phase) => {
                output.push_str(&serialize_phase(phase));
            }
            PlanSection::Tickets(tickets) => {
                output.push_str(&serialize_ticket_list(tickets));
            }
            PlanSection::FreeForm(freeform) => {
                output.push_str(&serialize_freeform(freeform));
            }
        }
    }

    output
}

/// Serialize a phase to markdown format.
///
/// Output format:
/// ```text
/// ## Phase N: Name
///
/// Description...
///
/// ### Success Criteria
///
/// - Criterion 1
///
/// ### Tickets
///
/// 1. ticket-id-1
/// 2. ticket-id-2
/// ```
fn serialize_phase(phase: &Phase) -> String {
    let mut output = String::new();

    // Phase header
    if phase.name.is_empty() {
        output.push_str(&format!("## Phase {}\n", phase.number));
    } else {
        output.push_str(&format!("## Phase {}: {}\n", phase.number, phase.name));
    }

    // Phase description
    if let Some(ref description) = phase.description {
        output.push('\n');
        output.push_str(description);
        output.push('\n');
    }

    // Success criteria
    if !phase.success_criteria.is_empty() {
        output.push_str("\n### Success Criteria\n\n");
        for criterion in &phase.success_criteria {
            output.push_str(&format!("- {}\n", criterion));
        }
    }

    // Tickets
    if !phase.tickets.is_empty() {
        output.push_str("\n### Tickets\n\n");
        for (i, ticket) in phase.tickets.iter().enumerate() {
            output.push_str(&format!("{}. {}\n", i + 1, ticket));
        }
    }

    output
}

/// Serialize a tickets section (for simple plans) to markdown format.
///
/// Output format:
/// ```text
/// ## Tickets
///
/// 1. ticket-id-1
/// 2. ticket-id-2
/// ```
fn serialize_ticket_list(tickets: &[String]) -> String {
    let mut output = String::new();

    output.push_str("## Tickets\n\n");
    for (i, ticket) in tickets.iter().enumerate() {
        output.push_str(&format!("{}. {}\n", i + 1, ticket));
    }

    output
}

/// Serialize a free-form section to markdown format.
///
/// The heading and content are output verbatim.
fn serialize_freeform(freeform: &FreeFormSection) -> String {
    let mut output = String::new();

    output.push_str(&format!("## {}\n", freeform.heading));
    if !freeform.content.is_empty() {
        output.push('\n');
        output.push_str(&freeform.content);
        // Ensure trailing newline
        if !freeform.content.ends_with('\n') {
            output.push('\n');
        }
    }

    output
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
---
No H1 heading, just content.

## Phase 1: Test

### Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(content).unwrap();
        assert!(metadata.title.is_none());
        // Content before first H2 is description
        assert!(
            metadata
                .description
                .as_ref()
                .unwrap()
                .contains("No H1 heading")
        );
    }

    #[test]
    fn test_parse_plan_empty_phase() {
        let content = r#"---
id: plan-empty
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
        assert!(
            metadata
                .description
                .as_ref()
                .unwrap()
                .contains("SQLite-based caching layer")
        );

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

    // ==================== Serialization Tests ====================

    #[test]
    fn test_serialize_simple_plan() {
        let mut metadata = PlanMetadata::default();
        metadata.id = Some("plan-a1b2".to_string());
        metadata.uuid = Some("550e8400-e29b-41d4-a716-446655440000".to_string());
        metadata.created = Some("2024-01-01T00:00:00Z".to_string());
        metadata.title = Some("Simple Plan Title".to_string());
        metadata.description = Some("This is the plan description.".to_string());
        metadata.acceptance_criteria = vec![
            "All tests pass".to_string(),
            "Documentation complete".to_string(),
        ];
        metadata.sections.push(PlanSection::Tickets(vec![
            "j-a1b2".to_string(),
            "j-c3d4".to_string(),
            "j-e5f6".to_string(),
        ]));

        let serialized = serialize_plan(&metadata);

        // Verify key components
        assert!(serialized.contains("---"));
        assert!(serialized.contains("id: plan-a1b2"));
        assert!(serialized.contains("uuid: 550e8400-e29b-41d4-a716-446655440000"));
        assert!(serialized.contains("created: 2024-01-01T00:00:00Z"));
        assert!(serialized.contains("# Simple Plan Title"));
        assert!(serialized.contains("This is the plan description."));
        assert!(serialized.contains("## Acceptance Criteria"));
        assert!(serialized.contains("- All tests pass"));
        assert!(serialized.contains("- Documentation complete"));
        assert!(serialized.contains("## Tickets"));
        assert!(serialized.contains("1. j-a1b2"));
        assert!(serialized.contains("2. j-c3d4"));
        assert!(serialized.contains("3. j-e5f6"));
    }

    #[test]
    fn test_serialize_phased_plan() {
        let mut metadata = PlanMetadata::default();
        metadata.id = Some("plan-b2c3".to_string());
        metadata.created = Some("2024-01-01T00:00:00Z".to_string());
        metadata.title = Some("Phased Plan".to_string());
        metadata.description = Some("Overview of the plan.".to_string());
        metadata.acceptance_criteria = vec!["Performance targets met".to_string()];

        let mut phase1 = Phase::new("1", "Infrastructure");
        phase1.description = Some("Set up the foundational components.".to_string());
        phase1.success_criteria = vec![
            "Database tables created".to_string(),
            "Helper functions work".to_string(),
        ];
        phase1.tickets = vec!["j-a1b2".to_string(), "j-c3d4".to_string()];

        let mut phase2 = Phase::new("2", "Implementation");
        phase2.tickets = vec!["j-e5f6".to_string()];

        metadata.sections.push(PlanSection::Phase(phase1));
        metadata.sections.push(PlanSection::Phase(phase2));

        let serialized = serialize_plan(&metadata);

        // Verify structure
        assert!(serialized.contains("# Phased Plan"));
        assert!(serialized.contains("Overview of the plan."));
        assert!(serialized.contains("## Phase 1: Infrastructure"));
        assert!(serialized.contains("Set up the foundational components."));
        assert!(serialized.contains("### Success Criteria"));
        assert!(serialized.contains("- Database tables created"));
        assert!(serialized.contains("### Tickets"));
        assert!(serialized.contains("1. j-a1b2"));
        assert!(serialized.contains("2. j-c3d4"));
        assert!(serialized.contains("## Phase 2: Implementation"));
        assert!(serialized.contains("1. j-e5f6"));
    }

    #[test]
    fn test_serialize_plan_with_freeform_sections() {
        let mut metadata = PlanMetadata::default();
        metadata.id = Some("plan-c3d4".to_string());
        metadata.created = Some("2024-01-01T00:00:00Z".to_string());
        metadata.title = Some("Plan with Free-form Content".to_string());
        metadata.description = Some("Description.".to_string());

        // Add free-form section
        metadata
            .sections
            .push(PlanSection::FreeForm(FreeFormSection::new(
                "Overview",
                "### Motivation\n\nThis section explains why we're doing this.",
            )));

        // Add phase
        let mut phase = Phase::new("1", "Setup");
        phase.tickets = vec!["j-a1b2".to_string()];
        metadata.sections.push(PlanSection::Phase(phase));

        // Add another free-form section
        metadata
            .sections
            .push(PlanSection::FreeForm(FreeFormSection::new(
                "Open Questions",
                "1. How often should we sync?",
            )));

        let serialized = serialize_plan(&metadata);

        // Verify ordering
        let overview_pos = serialized.find("## Overview").unwrap();
        let phase_pos = serialized.find("## Phase 1: Setup").unwrap();
        let questions_pos = serialized.find("## Open Questions").unwrap();

        assert!(overview_pos < phase_pos);
        assert!(phase_pos < questions_pos);

        // Verify content preserved
        assert!(serialized.contains("### Motivation"));
        assert!(serialized.contains("This section explains why we're doing this."));
        assert!(serialized.contains("How often should we sync?"));
    }

    #[test]
    fn test_serialize_phase_empty_name() {
        let phase = Phase::new("1", "");
        let serialized = serialize_phase(&phase);
        assert!(serialized.contains("## Phase 1\n"));
        assert!(!serialized.contains("## Phase 1:\n"));
    }

    #[test]
    fn test_serialize_phase_with_all_fields() {
        let mut phase = Phase::new("2a", "Sub-task");
        phase.description = Some("This is a description.".to_string());
        phase.success_criteria = vec!["Criterion one".to_string(), "Criterion two".to_string()];
        phase.tickets = vec!["j-t1".to_string(), "j-t2".to_string(), "j-t3".to_string()];

        let serialized = serialize_phase(&phase);

        assert!(serialized.contains("## Phase 2a: Sub-task"));
        assert!(serialized.contains("This is a description."));
        assert!(serialized.contains("### Success Criteria"));
        assert!(serialized.contains("- Criterion one"));
        assert!(serialized.contains("- Criterion two"));
        assert!(serialized.contains("### Tickets"));
        assert!(serialized.contains("1. j-t1"));
        assert!(serialized.contains("2. j-t2"));
        assert!(serialized.contains("3. j-t3"));
    }

    #[test]
    fn test_serialize_ticket_list_helper() {
        let tickets = vec![
            "j-a1b2".to_string(),
            "j-c3d4".to_string(),
            "j-e5f6".to_string(),
        ];
        let serialized = serialize_ticket_list(&tickets);

        assert!(serialized.contains("## Tickets"));
        assert!(serialized.contains("1. j-a1b2"));
        assert!(serialized.contains("2. j-c3d4"));
        assert!(serialized.contains("3. j-e5f6"));
    }

    #[test]
    fn test_serialize_freeform_helper() {
        let freeform = FreeFormSection::new(
            "SQLite Schema",
            "```sql\nCREATE TABLE tickets (id TEXT);\n```",
        );
        let serialized = serialize_freeform(&freeform);

        assert!(serialized.contains("## SQLite Schema"));
        assert!(serialized.contains("```sql"));
        assert!(serialized.contains("CREATE TABLE tickets"));
    }

    // ==================== Round-Trip Tests ====================

    #[test]
    fn test_roundtrip_simple_plan() {
        let original = r#"---
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

        // Parse
        let metadata = parse_plan_content(original).unwrap();

        // Serialize
        let serialized = serialize_plan(&metadata);

        // Parse again
        let reparsed = parse_plan_content(&serialized).unwrap();

        // Verify information is preserved
        assert_eq!(reparsed.id, metadata.id);
        assert_eq!(reparsed.uuid, metadata.uuid);
        assert_eq!(reparsed.created, metadata.created);
        assert_eq!(reparsed.title, metadata.title);
        assert_eq!(reparsed.description, metadata.description);
        assert_eq!(reparsed.acceptance_criteria, metadata.acceptance_criteria);
        assert_eq!(reparsed.all_tickets(), metadata.all_tickets());
        assert_eq!(reparsed.is_simple(), metadata.is_simple());
    }

    #[test]
    fn test_roundtrip_phased_plan() {
        let original = r#"---
id: plan-b2c3
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

        // Parse
        let metadata = parse_plan_content(original).unwrap();

        // Serialize
        let serialized = serialize_plan(&metadata);

        // Parse again
        let reparsed = parse_plan_content(&serialized).unwrap();

        // Verify information is preserved
        assert_eq!(reparsed.id, metadata.id);
        assert_eq!(reparsed.title, metadata.title);
        assert_eq!(reparsed.description, metadata.description);
        assert_eq!(reparsed.acceptance_criteria, metadata.acceptance_criteria);
        assert_eq!(reparsed.is_phased(), metadata.is_phased());

        let orig_phases = metadata.phases();
        let new_phases = reparsed.phases();
        assert_eq!(new_phases.len(), orig_phases.len());

        for (orig, new) in orig_phases.iter().zip(new_phases.iter()) {
            assert_eq!(new.number, orig.number);
            assert_eq!(new.name, orig.name);
            assert_eq!(new.success_criteria, orig.success_criteria);
            assert_eq!(new.tickets, orig.tickets);
        }
    }

    #[test]
    fn test_roundtrip_plan_with_freeform() {
        let original = r#"---
id: plan-c3d4
created: 2024-01-01T00:00:00Z
---
# Plan with Mixed Content

Description here.

## Acceptance Criteria

- Criteria 1

## Overview

This is the overview section.

### Nested Header

Some nested content.

## Phase 1: Setup

Setup phase.

### Tickets

1. j-a1b2

## Technical Details

```rust
fn example() {
    println!("code block");
}
```

## Open Questions

1. Question one
2. Question two
"#;

        // Parse
        let metadata = parse_plan_content(original).unwrap();

        // Serialize
        let serialized = serialize_plan(&metadata);

        // Parse again
        let reparsed = parse_plan_content(&serialized).unwrap();

        // Verify structure is preserved
        assert_eq!(reparsed.id, metadata.id);
        assert_eq!(reparsed.title, metadata.title);
        assert_eq!(reparsed.is_phased(), metadata.is_phased());

        // Verify phases
        let orig_phases = metadata.phases();
        let new_phases = reparsed.phases();
        assert_eq!(new_phases.len(), orig_phases.len());

        // Verify free-form sections
        let orig_freeform = metadata.free_form_sections();
        let new_freeform = reparsed.free_form_sections();
        assert_eq!(new_freeform.len(), orig_freeform.len());

        for (orig, new) in orig_freeform.iter().zip(new_freeform.iter()) {
            assert_eq!(new.heading, orig.heading);
            // Content may have minor whitespace differences, but key content should be there
            for line in orig.content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    assert!(
                        new.content.contains(trimmed),
                        "Missing content in {}: {}",
                        new.heading,
                        trimmed
                    );
                }
            }
        }
    }

    #[test]
    fn test_roundtrip_preserves_section_order() {
        let original = r#"---
id: plan-order
created: 2024-01-01T00:00:00Z
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

        let metadata = parse_plan_content(original).unwrap();
        let serialized = serialize_plan(&metadata);
        let reparsed = parse_plan_content(&serialized).unwrap();

        // Verify section types match in order
        assert_eq!(reparsed.sections.len(), metadata.sections.len());

        for (orig, new) in metadata.sections.iter().zip(reparsed.sections.iter()) {
            match (orig, new) {
                (PlanSection::FreeForm(o), PlanSection::FreeForm(n)) => {
                    assert_eq!(o.heading, n.heading);
                }
                (PlanSection::Phase(o), PlanSection::Phase(n)) => {
                    assert_eq!(o.number, n.number);
                    assert_eq!(o.name, n.name);
                }
                (PlanSection::Tickets(o), PlanSection::Tickets(n)) => {
                    assert_eq!(o, n);
                }
                _ => panic!("Section type mismatch"),
            }
        }
    }

    #[test]
    fn test_roundtrip_minimal_plan() {
        let original = r#"---
id: plan-min
---
# Minimal Plan
"#;

        let metadata = parse_plan_content(original).unwrap();
        let serialized = serialize_plan(&metadata);
        let reparsed = parse_plan_content(&serialized).unwrap();

        assert_eq!(reparsed.id, metadata.id);
        assert_eq!(reparsed.title, metadata.title);
        assert!(reparsed.description.is_none());
        assert!(reparsed.acceptance_criteria.is_empty());
        assert!(reparsed.sections.is_empty());
    }

    #[test]
    fn test_roundtrip_complex_plan() {
        // A comprehensive test similar to the complex parsing test
        let original = r#"---
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

**Current performance characteristics:**
- Single ticket lookup: ~500ms

**Target performance:**
- Single ticket lookup: <5ms

## SQLite Schema

```sql
CREATE TABLE IF NOT EXISTS tickets (
    ticket_id TEXT PRIMARY KEY,
    mtime_ns INTEGER NOT NULL
);
```

## Phase 1: Infrastructure

Set up the foundational components.

### Success Criteria

- Database tables created correctly
- Helper functions work

### Tickets

1. j-dep1
2. j-mod2

## Phase 2: Sync Algorithm

Implement sync logic.

### Tickets

1. j-scan3
2. j-sync4

## Open Questions

1. How often should we sync?
"#;

        let metadata = parse_plan_content(original).unwrap();
        let serialized = serialize_plan(&metadata);
        let reparsed = parse_plan_content(&serialized).unwrap();

        // Core fields
        assert_eq!(reparsed.id, metadata.id);
        assert_eq!(reparsed.uuid, metadata.uuid);
        assert_eq!(reparsed.created, metadata.created);
        assert_eq!(reparsed.title, metadata.title);

        // Acceptance criteria
        assert_eq!(
            reparsed.acceptance_criteria.len(),
            metadata.acceptance_criteria.len()
        );

        // Phases
        assert_eq!(reparsed.phases().len(), metadata.phases().len());
        for (orig, new) in metadata.phases().iter().zip(reparsed.phases().iter()) {
            assert_eq!(new.number, orig.number);
            assert_eq!(new.name, orig.name);
            assert_eq!(new.tickets, orig.tickets);
        }

        // Free-form sections
        assert_eq!(
            reparsed.free_form_sections().len(),
            metadata.free_form_sections().len()
        );

        // Total tickets
        assert_eq!(reparsed.all_tickets(), metadata.all_tickets());
    }
}
