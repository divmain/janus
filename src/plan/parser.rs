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
use crate::plan::types::{
    FreeFormSection, ImportValidationError, ImportablePhase, ImportablePlan, ImportableTask, Phase,
    PlanMetadata, PlanSection,
};

// ============================================================================
// Section Alias Constants (for plan import)
// ============================================================================

/// Recognized section names for acceptance criteria (case-insensitive)
pub const ACCEPTANCE_CRITERIA_ALIASES: &[&str] = &[
    "acceptance criteria",
    "goals",
    "success criteria",
    "deliverables",
    "requirements",
    "objectives",
];

/// Recognized section names for tasks in simple plans (case-insensitive)
pub const TASKS_SECTION_ALIASES: &[&str] =
    &["tasks", "tickets", "work items", "items", "checklist"];

/// Regex pattern for matching phase headers in importable plans
/// Matches: "Phase N: Name", "Stage N - Name", "Part N: Name", "Step N: Name"
/// where N can be numeric (1, 2, 10) or alphanumeric (1a, 2b)
pub const PHASE_PATTERN: &str = r"(?i)^(phase|stage|part|step)\s+(\d+[a-z]?)\s*[-:]?\s*(.*)$";

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

// ============================================================================
// Importable Plan Parser Functions
// ============================================================================

/// Check if a heading text matches any alias in a given list (case-insensitive).
///
/// # Arguments
/// * `heading` - The heading text to check
/// * `aliases` - List of accepted aliases
///
/// # Returns
/// `true` if the heading matches any alias (case-insensitive)
pub fn is_section_alias(heading: &str, aliases: &[&str]) -> bool {
    let heading_lower = heading.to_lowercase();
    aliases.iter().any(|&alias| heading_lower == alias)
}

/// Check if a heading matches the phase pattern and extract phase info.
///
/// Matches headers like:
/// - "Phase 1: Infrastructure"
/// - "Stage 2a - Implementation"
/// - "Part 3: Testing"
/// - "Step 1: Setup"
///
/// # Returns
/// `Some((number, name))` if the heading is a valid phase header, `None` otherwise.
pub fn is_phase_header(heading: &str) -> Option<(String, String)> {
    let phase_re = Regex::new(PHASE_PATTERN).unwrap();

    phase_re.captures(heading).map(|caps| {
        let number = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
        let name = caps
            .get(3)
            .map(|m| m.as_str().trim())
            .unwrap_or("")
            .to_string();
        (number, name)
    })
}

/// Detect completion markers in task titles.
///
/// Handles both H3 style (`### Task Title [x]`) and extracts the clean title.
///
/// # Arguments
/// * `text` - The task title text (without the `### ` prefix)
///
/// # Returns
/// `(cleaned_title, is_complete)` tuple
pub fn is_completed_task(text: &str) -> (String, bool) {
    let text = text.trim();

    // H3 style: "Task Title [x]" or "Task Title [X]"
    if let Some(title) = text
        .strip_suffix("[x]")
        .or_else(|| text.strip_suffix("[X]"))
    {
        return (title.trim().to_string(), true);
    }

    (text.to_string(), false)
}

/// Parse a checkbox item from a list item text.
///
/// Handles formats like:
/// - "[ ] Unchecked task"
/// - "[x] Completed task"
/// - "[X] Completed task"
/// - "Task without checkbox" (treated as unchecked)
///
/// # Returns
/// `(title, is_complete)` tuple
fn parse_checkbox_item(text: &str) -> (String, bool) {
    let text = text.trim();

    // Check for checkbox markers
    if let Some(rest) = text.strip_prefix("[ ] ") {
        return (rest.trim().to_string(), false);
    }
    if let Some(rest) = text
        .strip_prefix("[x] ")
        .or_else(|| text.strip_prefix("[X] "))
    {
        return (rest.trim().to_string(), true);
    }

    // No checkbox, treat as unchecked
    (text.to_string(), false)
}

/// Extract the H1 title from a parsed markdown document.
///
/// # Arguments
/// * `root` - The root AST node of the parsed markdown
///
/// # Returns
/// The title text if an H1 heading is found, `None` otherwise.
fn extract_title<'a>(root: &'a AstNode<'a>) -> Option<String> {
    for node in root.children() {
        if let NodeValue::Heading(heading) = &node.data.borrow().value
            && heading.level == 1
        {
            let text = extract_text_content(node);
            return Some(text.trim().to_string());
        }
    }
    None
}

/// Extract the description (preamble paragraphs) from a parsed markdown document.
///
/// Returns content between the H1 title and the first H2 section.
///
/// # Arguments
/// * `root` - The root AST node of the parsed markdown
/// * `options` - Comrak options for rendering nodes back to markdown
///
/// # Returns
/// The description text if any content exists before the first H2, `None` otherwise.
fn extract_import_description<'a>(root: &'a AstNode<'a>, options: &Options) -> Option<String> {
    let mut content = String::new();
    let mut found_title = false;

    for node in root.children() {
        match &node.data.borrow().value {
            NodeValue::Heading(heading) => {
                if heading.level == 1 {
                    found_title = true;
                } else if heading.level == 2 {
                    // Hit first H2, stop collecting
                    break;
                } else if found_title {
                    // H3+ in preamble (unusual but handle it)
                    content.push_str(&render_node_to_markdown(node, options));
                }
            }
            _ => {
                if found_title {
                    content.push_str(&render_node_to_markdown(node, options));
                }
            }
        }
    }

    let trimmed = content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Extract acceptance criteria from a parsed markdown document.
///
/// Looks for an H2 section matching acceptance criteria aliases and extracts list items.
///
/// # Arguments
/// * `root` - The root AST node of the parsed markdown
///
/// # Returns
/// Vector of criteria strings.
fn extract_import_acceptance_criteria<'a>(root: &'a AstNode<'a>) -> Vec<String> {
    let mut in_criteria_section = false;
    let mut criteria = Vec::new();

    for node in root.children() {
        match &node.data.borrow().value {
            NodeValue::Heading(heading) => {
                if heading.level == 2 {
                    let text = extract_text_content(node);
                    in_criteria_section = is_section_alias(&text, ACCEPTANCE_CRITERIA_ALIASES);
                }
            }
            NodeValue::List(_) => {
                if in_criteria_section {
                    // Extract list items
                    for item in node.children() {
                        if let NodeValue::Item(_) = &item.data.borrow().value {
                            let item_text = extract_text_content(item);
                            let trimmed = item_text.trim();
                            if !trimmed.is_empty() {
                                criteria.push(trimmed.to_string());
                            }
                        }
                    }
                    // Only take the first list in the criteria section
                    in_criteria_section = false;
                }
            }
            _ => {}
        }
    }

    criteria
}

/// Parse tasks from a section, supporting both H3 headers and checklist items.
///
/// Priority:
/// 1. If H3 headers exist, each H3 becomes a task
/// 2. If no H3s, look for bullet/numbered lists (checklist items)
///
/// # Arguments
/// * `root` - The root AST node
/// * `section_start` - Index of the H2 heading node
/// * `section_end` - Index of the next H2 heading node (or end of document)
/// * `options` - Comrak options for rendering
///
/// # Returns
/// Vector of `ImportableTask` structs.
fn parse_tasks_from_section<'a>(
    root: &'a AstNode<'a>,
    section_heading: &str,
    options: &Options,
) -> Vec<ImportableTask> {
    let mut tasks = Vec::new();
    let mut in_section = false;
    let mut current_h3: Option<(String, bool)> = None; // (title, is_complete)
    let mut current_h3_content = String::new();
    let mut list_items: Vec<ImportableTask> = Vec::new();
    let mut has_h3_tasks = false;

    let nodes: Vec<_> = root.children().collect();

    for node in &nodes {
        match &node.data.borrow().value {
            NodeValue::Heading(heading) => {
                if heading.level == 2 {
                    let text = extract_text_content(node);
                    if in_section {
                        // End of our section
                        // Finalize any pending H3 task
                        if let Some((title, is_complete)) = current_h3.take() {
                            tasks.push(ImportableTask {
                                title,
                                body: if current_h3_content.trim().is_empty() {
                                    None
                                } else {
                                    Some(current_h3_content.trim().to_string())
                                },
                                is_complete,
                            });
                        }
                        break;
                    }

                    // Check if this is our target section
                    if text.trim().eq_ignore_ascii_case(section_heading) {
                        in_section = true;
                    }
                } else if heading.level == 3 && in_section {
                    // H3 task header
                    // Finalize any pending H3 task
                    if let Some((title, is_complete)) = current_h3.take() {
                        tasks.push(ImportableTask {
                            title,
                            body: if current_h3_content.trim().is_empty() {
                                None
                            } else {
                                Some(current_h3_content.trim().to_string())
                            },
                            is_complete,
                        });
                    }

                    let text = extract_text_content(node);
                    let (title, is_complete) = is_completed_task(&text);
                    current_h3 = Some((title, is_complete));
                    current_h3_content = String::new();
                    has_h3_tasks = true;
                } else if in_section {
                    // H4+ content within an H3
                    if current_h3.is_some() {
                        current_h3_content.push_str(&render_node_to_markdown(node, options));
                    }
                }
            }
            NodeValue::List(_) => {
                if in_section {
                    if current_h3.is_some() {
                        // List content within an H3 task
                        current_h3_content.push_str(&render_node_to_markdown(node, options));
                    } else if !has_h3_tasks {
                        // Checklist items (only if no H3 tasks found yet)
                        for item in node.children() {
                            if let NodeValue::Item(_) = &item.data.borrow().value {
                                let item_text = extract_text_content(item);
                                let (title, is_complete) = parse_checkbox_item(&item_text);
                                if !title.is_empty() {
                                    list_items.push(ImportableTask {
                                        title,
                                        body: None,
                                        is_complete,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            _ => {
                if in_section && current_h3.is_some() {
                    // Content within an H3 task
                    current_h3_content.push_str(&render_node_to_markdown(node, options));
                }
            }
        }
    }

    // Finalize any pending H3 task
    if let Some((title, is_complete)) = current_h3.take() {
        tasks.push(ImportableTask {
            title,
            body: if current_h3_content.trim().is_empty() {
                None
            } else {
                Some(current_h3_content.trim().to_string())
            },
            is_complete,
        });
    }

    // If we have H3 tasks, use those; otherwise use list items
    if !tasks.is_empty() { tasks } else { list_items }
}

/// Parse tasks from a phase section.
///
/// This is similar to `parse_tasks_from_section` but operates within a phase's boundaries.
fn parse_tasks_from_phase<'a>(
    nodes: &[&'a AstNode<'a>],
    start_idx: usize,
    end_idx: usize,
    options: &Options,
) -> Vec<ImportableTask> {
    let mut tasks = Vec::new();
    let mut current_h3: Option<(String, bool)> = None;
    let mut current_h3_content = String::new();
    let mut list_items: Vec<ImportableTask> = Vec::new();
    let mut has_h3_tasks = false;

    for node in &nodes[start_idx..end_idx] {
        match &node.data.borrow().value {
            NodeValue::Heading(heading) => {
                if heading.level == 3 {
                    // H3 task header
                    // Finalize any pending H3 task
                    if let Some((title, is_complete)) = current_h3.take() {
                        tasks.push(ImportableTask {
                            title,
                            body: if current_h3_content.trim().is_empty() {
                                None
                            } else {
                                Some(current_h3_content.trim().to_string())
                            },
                            is_complete,
                        });
                    }

                    let text = extract_text_content(node);
                    let (title, is_complete) = is_completed_task(&text);
                    current_h3 = Some((title, is_complete));
                    current_h3_content = String::new();
                    has_h3_tasks = true;
                } else if heading.level >= 4 {
                    // H4+ content within an H3
                    if current_h3.is_some() {
                        current_h3_content.push_str(&render_node_to_markdown(node, options));
                    }
                }
            }
            NodeValue::List(_) => {
                if current_h3.is_some() {
                    // List content within an H3 task
                    current_h3_content.push_str(&render_node_to_markdown(node, options));
                } else if !has_h3_tasks {
                    // Checklist items (only if no H3 tasks found yet)
                    for item in node.children() {
                        if let NodeValue::Item(_) = &item.data.borrow().value {
                            let item_text = extract_text_content(item);
                            let (title, is_complete) = parse_checkbox_item(&item_text);
                            if !title.is_empty() {
                                list_items.push(ImportableTask {
                                    title,
                                    body: None,
                                    is_complete,
                                });
                            }
                        }
                    }
                }
            }
            _ => {
                if current_h3.is_some() {
                    // Content within an H3 task
                    current_h3_content.push_str(&render_node_to_markdown(node, options));
                }
            }
        }
    }

    // Finalize any pending H3 task
    if let Some((title, is_complete)) = current_h3.take() {
        tasks.push(ImportableTask {
            title,
            body: if current_h3_content.trim().is_empty() {
                None
            } else {
                Some(current_h3_content.trim().to_string())
            },
            is_complete,
        });
    }

    // If we have H3 tasks, use those; otherwise use list items
    if !tasks.is_empty() { tasks } else { list_items }
}

/// Parse phases from a markdown document.
///
/// Iterates through H2 sections, identifies phase headers, extracts phase description
/// and tasks for each phase.
///
/// # Arguments
/// * `root` - The root AST node
/// * `options` - Comrak options for rendering
///
/// # Returns
/// Vector of `ImportablePhase` structs. Empty if no phases found.
fn parse_phases<'a>(root: &'a AstNode<'a>, options: &Options) -> Vec<ImportablePhase> {
    let mut phases = Vec::new();
    let nodes: Vec<_> = root.children().collect();

    // Find all H2 sections that are phases
    let mut phase_indices: Vec<(usize, String, String)> = Vec::new(); // (index, number, name)

    for (idx, node) in nodes.iter().enumerate() {
        if let NodeValue::Heading(heading) = &node.data.borrow().value
            && heading.level == 2
        {
            let text = extract_text_content(node);
            if let Some((number, name)) = is_phase_header(&text) {
                phase_indices.push((idx, number, name));
            }
        }
    }

    // Process each phase
    for (i, (start_idx, number, name)) in phase_indices.iter().enumerate() {
        // Find the end of this phase section (next H2 or end of document)
        let end_idx = if i + 1 < phase_indices.len() {
            phase_indices[i + 1].0
        } else {
            // Find next non-phase H2 or end of document
            nodes
                .iter()
                .enumerate()
                .skip(start_idx + 1)
                .find(|(_, node)| {
                    if let NodeValue::Heading(h) = &node.data.borrow().value {
                        h.level == 2
                    } else {
                        false
                    }
                })
                .map(|(idx, _)| idx)
                .unwrap_or(nodes.len())
        };

        // Extract phase description (content between H2 and first H3)
        let mut description_parts = Vec::new();
        for node in &nodes[start_idx + 1..end_idx] {
            if let NodeValue::Heading(h) = &node.data.borrow().value
                && h.level == 3
            {
                break;
            }
            description_parts.push(render_node_to_markdown(node, options));
        }

        let description = description_parts.join("").trim().to_string();
        let description = if description.is_empty() {
            None
        } else {
            Some(description)
        };

        // Parse tasks from this phase
        let tasks = parse_tasks_from_phase(&nodes, start_idx + 1, end_idx, options);

        phases.push(ImportablePhase {
            number: number.clone(),
            name: name.clone(),
            description,
            tasks,
        });
    }

    phases
}

/// Parse simple tasks from a document (for plans without phases).
///
/// Looks for a Tasks section (using `TASKS_SECTION_ALIASES`) and extracts tasks from it.
///
/// # Arguments
/// * `root` - The root AST node
/// * `options` - Comrak options for rendering
///
/// # Returns
/// Vector of `ImportableTask` structs. Empty if no Tasks section found.
fn parse_simple_tasks<'a>(root: &'a AstNode<'a>, options: &Options) -> Vec<ImportableTask> {
    // Find the Tasks section heading
    for node in root.children() {
        if let NodeValue::Heading(heading) = &node.data.borrow().value
            && heading.level == 2
        {
            let text = extract_text_content(node);
            if is_section_alias(&text, TASKS_SECTION_ALIASES) {
                return parse_tasks_from_section(root, &text, options);
            }
        }
    }

    Vec::new()
}

/// Parse an importable plan document.
///
/// This is the main entry point for parsing AI-generated plan documents.
///
/// # Arguments
/// * `content` - The raw markdown content
///
/// # Returns
/// `Ok(ImportablePlan)` if parsing succeeds, `Err(JanusError::ImportFailed)` if validation fails.
///
/// # Validation Rules
/// 1. Document must have an H1 title
/// 2. Document must have either phases with tasks OR a Tasks section
pub fn parse_importable_plan(content: &str) -> Result<ImportablePlan> {
    let arena = Arena::new();
    let options = Options::default();
    let root = parse_document(&arena, content, &options);

    let mut errors: Vec<ImportValidationError> = Vec::new();

    // 1. Extract title
    let title = match extract_title(root) {
        Some(t) => t,
        None => {
            errors.push(
                ImportValidationError::new("Missing plan title (expected H1 heading)")
                    .with_hint("Add \"# Your Plan Title\" at the start of the document"),
            );
            String::new()
        }
    };

    // 2. Extract description
    let description = extract_import_description(root, &options);

    // 3. Extract acceptance criteria
    let acceptance_criteria = extract_import_acceptance_criteria(root);

    // 4. Try to parse as phased plan first
    let phases = parse_phases(root, &options);

    // 5. If no phases, try to parse as simple plan
    let tasks = if phases.is_empty() {
        parse_simple_tasks(root, &options)
    } else {
        Vec::new()
    };

    // 6. Validation: must have either phases with tasks OR simple tasks
    if phases.is_empty() && tasks.is_empty() {
        errors.push(
            ImportValidationError::new("Document has no phases or tasks section")
                .with_hint("Structure your document with \"## Phase N: Name\" sections or a \"## Tasks\" section"),
        );
    }

    // Check for empty phases
    for phase in &phases {
        if phase.tasks.is_empty() {
            errors.push(
                ImportValidationError::new(format!(
                    "Phase \"{}\" has no tasks",
                    if phase.name.is_empty() {
                        format!("Phase {}", phase.number)
                    } else {
                        format!("Phase {}: {}", phase.number, phase.name)
                    }
                ))
                .with_hint("Add ### Task headers or a checklist under the phase"),
            );
        }
    }

    // If there are errors, return them
    if !errors.is_empty() {
        let issues: Vec<String> = errors.iter().map(|e| e.to_display_string()).collect();
        return Err(JanusError::ImportFailed {
            message: "Validation failed".to_string(),
            issues,
        });
    }

    Ok(ImportablePlan {
        title,
        description,
        acceptance_criteria,
        phases,
        tasks,
    })
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

    // ==================== Importable Plan Parser Tests ====================

    #[test]
    fn test_is_section_alias_acceptance_criteria() {
        // Exact matches (case-insensitive)
        assert!(is_section_alias(
            "acceptance criteria",
            ACCEPTANCE_CRITERIA_ALIASES
        ));
        assert!(is_section_alias(
            "Acceptance Criteria",
            ACCEPTANCE_CRITERIA_ALIASES
        ));
        assert!(is_section_alias(
            "ACCEPTANCE CRITERIA",
            ACCEPTANCE_CRITERIA_ALIASES
        ));
        assert!(is_section_alias("goals", ACCEPTANCE_CRITERIA_ALIASES));
        assert!(is_section_alias("Goals", ACCEPTANCE_CRITERIA_ALIASES));
        assert!(is_section_alias(
            "success criteria",
            ACCEPTANCE_CRITERIA_ALIASES
        ));
        assert!(is_section_alias(
            "deliverables",
            ACCEPTANCE_CRITERIA_ALIASES
        ));
        assert!(is_section_alias(
            "requirements",
            ACCEPTANCE_CRITERIA_ALIASES
        ));
        assert!(is_section_alias("objectives", ACCEPTANCE_CRITERIA_ALIASES));

        // Non-matches
        assert!(!is_section_alias("tasks", ACCEPTANCE_CRITERIA_ALIASES));
        assert!(!is_section_alias("acceptance", ACCEPTANCE_CRITERIA_ALIASES));
        assert!(!is_section_alias("criteria", ACCEPTANCE_CRITERIA_ALIASES));
    }

    #[test]
    fn test_is_section_alias_tasks() {
        // Exact matches (case-insensitive)
        assert!(is_section_alias("tasks", TASKS_SECTION_ALIASES));
        assert!(is_section_alias("Tasks", TASKS_SECTION_ALIASES));
        assert!(is_section_alias("TASKS", TASKS_SECTION_ALIASES));
        assert!(is_section_alias("tickets", TASKS_SECTION_ALIASES));
        assert!(is_section_alias("work items", TASKS_SECTION_ALIASES));
        assert!(is_section_alias("items", TASKS_SECTION_ALIASES));
        assert!(is_section_alias("checklist", TASKS_SECTION_ALIASES));

        // Non-matches
        assert!(!is_section_alias("goal", TASKS_SECTION_ALIASES));
        assert!(!is_section_alias("task list", TASKS_SECTION_ALIASES));
    }

    #[test]
    fn test_is_phase_header_standard() {
        // Standard format: "Phase N: Name"
        let result = is_phase_header("Phase 1: Infrastructure");
        assert_eq!(
            result,
            Some(("1".to_string(), "Infrastructure".to_string()))
        );

        let result = is_phase_header("Phase 2: Implementation");
        assert_eq!(
            result,
            Some(("2".to_string(), "Implementation".to_string()))
        );
    }

    #[test]
    fn test_is_phase_header_dash_separator() {
        // Dash separator: "Phase N - Name"
        let result = is_phase_header("Phase 1 - Setup");
        assert_eq!(result, Some(("1".to_string(), "Setup".to_string())));
    }

    #[test]
    fn test_is_phase_header_alphanumeric() {
        // Alphanumeric phase numbers: "Phase 1a", "Phase 2b"
        let result = is_phase_header("Phase 1a: Sub-phase A");
        assert_eq!(result, Some(("1a".to_string(), "Sub-phase A".to_string())));

        let result = is_phase_header("Phase 2b - Sub-phase B");
        assert_eq!(result, Some(("2b".to_string(), "Sub-phase B".to_string())));
    }

    #[test]
    fn test_is_phase_header_multi_digit() {
        let result = is_phase_header("Phase 10: Final Phase");
        assert_eq!(result, Some(("10".to_string(), "Final Phase".to_string())));
    }

    #[test]
    fn test_is_phase_header_no_name() {
        let result = is_phase_header("Phase 1:");
        assert_eq!(result, Some(("1".to_string(), "".to_string())));

        let result = is_phase_header("Phase 1");
        assert_eq!(result, Some(("1".to_string(), "".to_string())));
    }

    #[test]
    fn test_is_phase_header_case_insensitive() {
        let result = is_phase_header("PHASE 1: Test");
        assert_eq!(result, Some(("1".to_string(), "Test".to_string())));

        let result = is_phase_header("phase 1: test");
        assert_eq!(result, Some(("1".to_string(), "test".to_string())));
    }

    #[test]
    fn test_is_phase_header_stage_part_step() {
        // "Stage N", "Part N", "Step N" should also work
        let result = is_phase_header("Stage 1: Setup");
        assert_eq!(result, Some(("1".to_string(), "Setup".to_string())));

        let result = is_phase_header("Part 2: Implementation");
        assert_eq!(
            result,
            Some(("2".to_string(), "Implementation".to_string()))
        );

        let result = is_phase_header("Step 3: Testing");
        assert_eq!(result, Some(("3".to_string(), "Testing".to_string())));
    }

    #[test]
    fn test_is_phase_header_not_a_phase() {
        // These should NOT match
        assert!(is_phase_header("Phase Diagrams").is_none());
        assert!(is_phase_header("Phase without number").is_none());
        assert!(is_phase_header("Overview").is_none());
        assert!(is_phase_header("Tasks").is_none());
    }

    #[test]
    fn test_is_completed_task_h3_style() {
        // H3 style: "Task Title [x]"
        let (title, is_complete) = is_completed_task("Add Caching Support [x]");
        assert_eq!(title, "Add Caching Support");
        assert!(is_complete);

        let (title, is_complete) = is_completed_task("Add Caching Support [X]");
        assert_eq!(title, "Add Caching Support");
        assert!(is_complete);
    }

    #[test]
    fn test_is_completed_task_unchecked() {
        let (title, is_complete) = is_completed_task("Add Caching Support");
        assert_eq!(title, "Add Caching Support");
        assert!(!is_complete);
    }

    #[test]
    fn test_is_completed_task_with_whitespace() {
        let (title, is_complete) = is_completed_task("  Add Caching Support [x]  ");
        assert_eq!(title, "Add Caching Support");
        assert!(is_complete);
    }

    #[test]
    fn test_parse_importable_plan_simple_h3_tasks() {
        let content = r#"# Simple Plan Title

This is the plan description.

## Tasks

### Task One

Task one description.

### Task Two

Task two description.
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert_eq!(plan.title, "Simple Plan Title");
        assert_eq!(
            plan.description,
            Some("This is the plan description.".to_string())
        );
        assert!(!plan.is_phased());
        assert!(plan.is_simple());
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[0].title, "Task One");
        assert_eq!(
            plan.tasks[0].body,
            Some("Task one description.".to_string())
        );
        assert_eq!(plan.tasks[1].title, "Task Two");
    }

    #[test]
    fn test_parse_importable_plan_simple_checklist() {
        let content = r#"# Checklist Plan

## Tasks

- [ ] Unchecked task one
- [x] Completed task two
- Task without checkbox
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert_eq!(plan.title, "Checklist Plan");
        assert!(plan.is_simple());
        assert_eq!(plan.tasks.len(), 3);
        assert_eq!(plan.tasks[0].title, "Unchecked task one");
        assert!(!plan.tasks[0].is_complete);
        assert_eq!(plan.tasks[1].title, "Completed task two");
        assert!(plan.tasks[1].is_complete);
        assert_eq!(plan.tasks[2].title, "Task without checkbox");
        assert!(!plan.tasks[2].is_complete);
    }

    #[test]
    fn test_parse_importable_plan_phased() {
        let content = r#"# Phased Implementation Plan

Overview of the implementation.

## Acceptance Criteria

- All tests pass
- Documentation complete

## Phase 1: Infrastructure

Set up the foundational components.

### Add Dependencies

Add the required dependencies to Cargo.toml.

### Create Module Structure

Create the basic module structure.

## Phase 2: Implementation

Implement the core logic.

### Implement Core Function

The main implementation task.
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert_eq!(plan.title, "Phased Implementation Plan");
        assert_eq!(
            plan.description,
            Some("Overview of the implementation.".to_string())
        );
        assert!(plan.is_phased());
        assert!(!plan.is_simple());
        assert_eq!(plan.phases.len(), 2);
        assert_eq!(plan.acceptance_criteria.len(), 2);
        assert_eq!(plan.acceptance_criteria[0], "All tests pass");
        assert_eq!(plan.acceptance_criteria[1], "Documentation complete");

        // Phase 1
        assert_eq!(plan.phases[0].number, "1");
        assert_eq!(plan.phases[0].name, "Infrastructure");
        assert_eq!(
            plan.phases[0].description,
            Some("Set up the foundational components.".to_string())
        );
        assert_eq!(plan.phases[0].tasks.len(), 2);
        assert_eq!(plan.phases[0].tasks[0].title, "Add Dependencies");
        assert_eq!(plan.phases[0].tasks[1].title, "Create Module Structure");

        // Phase 2
        assert_eq!(plan.phases[1].number, "2");
        assert_eq!(plan.phases[1].name, "Implementation");
        assert_eq!(plan.phases[1].tasks.len(), 1);
        assert_eq!(plan.phases[1].tasks[0].title, "Implement Core Function");
    }

    #[test]
    fn test_parse_importable_plan_phased_checklist() {
        let content = r#"# Phased Checklist Plan

## Phase 1: Setup

- [x] Task A completed
- [ ] Task B pending
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert!(plan.is_phased());
        assert_eq!(plan.phases[0].tasks.len(), 2);
        assert_eq!(plan.phases[0].tasks[0].title, "Task A completed");
        assert!(plan.phases[0].tasks[0].is_complete);
        assert_eq!(plan.phases[0].tasks[1].title, "Task B pending");
        assert!(!plan.phases[0].tasks[1].is_complete);
    }

    #[test]
    fn test_parse_importable_plan_completed_h3_tasks() {
        let content = r#"# Plan with Completed Tasks

## Tasks

### Task One [x]

This task is done.

### Task Two

This task is pending.
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[0].title, "Task One");
        assert!(plan.tasks[0].is_complete);
        assert_eq!(plan.tasks[1].title, "Task Two");
        assert!(!plan.tasks[1].is_complete);
    }

    #[test]
    fn test_parse_importable_plan_with_code_blocks() {
        let content = r#"# Plan with Code

## Tasks

### Add Cache Support

Implement caching in the service.

```rust
let cache = HashMap::new();
```

Key changes:
- Add cache data structure
- Modify speak() method
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert_eq!(plan.tasks.len(), 1);
        assert_eq!(plan.tasks[0].title, "Add Cache Support");
        let body = plan.tasks[0].body.as_ref().unwrap();
        assert!(body.contains("Implement caching"));
        assert!(body.contains("HashMap::new()"));
        assert!(body.contains("Key changes:"));
    }

    #[test]
    fn test_parse_importable_plan_acceptance_criteria_aliases() {
        // Test "Goals" alias
        let content = r#"# Plan with Goals

## Goals

- Goal one
- Goal two

## Tasks

### Do something
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert_eq!(plan.acceptance_criteria.len(), 2);
        assert_eq!(plan.acceptance_criteria[0], "Goal one");
    }

    #[test]
    fn test_parse_importable_plan_tasks_aliases() {
        // Test "Checklist" alias
        let content = r#"# Plan with Checklist

## Checklist

### Item one

Description.
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert!(plan.is_simple());
        assert_eq!(plan.tasks.len(), 1);
        assert_eq!(plan.tasks[0].title, "Item one");
    }

    #[test]
    fn test_parse_importable_plan_missing_title() {
        let content = r#"Just some content without H1.

## Tasks

### Task one
"#;

        let result = parse_importable_plan(content);
        assert!(result.is_err());
        if let Err(crate::error::JanusError::ImportFailed { issues, .. }) = result {
            assert!(issues.iter().any(|s| s.contains("Missing plan title")));
        } else {
            panic!("Expected ImportFailed error");
        }
    }

    #[test]
    fn test_parse_importable_plan_no_tasks() {
        let content = r#"# Plan with No Tasks

Just a description with no tasks or phases.
"#;

        let result = parse_importable_plan(content);
        assert!(result.is_err());
        if let Err(crate::error::JanusError::ImportFailed { issues, .. }) = result {
            assert!(
                issues
                    .iter()
                    .any(|s| s.contains("no phases or tasks section"))
            );
        } else {
            panic!("Expected ImportFailed error");
        }
    }

    #[test]
    fn test_parse_importable_plan_empty_phase() {
        let content = r#"# Plan with Empty Phase

## Phase 1: Empty

No tasks here.
"#;

        let result = parse_importable_plan(content);
        assert!(result.is_err());
        if let Err(crate::error::JanusError::ImportFailed { issues, .. }) = result {
            assert!(issues.iter().any(|s| s.contains("has no tasks")));
        } else {
            panic!("Expected ImportFailed error");
        }
    }

    #[test]
    fn test_parse_importable_plan_task_count() {
        let content = r#"# Multi-Phase Plan

## Phase 1: First

### Task 1

### Task 2

## Phase 2: Second

### Task 3

### Task 4

### Task 5
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert_eq!(plan.task_count(), 5);
        assert_eq!(plan.all_tasks().len(), 5);
    }

    #[test]
    fn test_parse_importable_plan_phases_take_priority() {
        // If a document has both phases and a Tasks section,
        // phases should be used (since phases contain the tasks)
        let content = r#"# Mixed Document

## Phase 1: Implementation

### Task in phase

## Tasks

### Task in tasks section
"#;

        let plan = parse_importable_plan(content).unwrap();
        // The document is treated as phased because it has a Phase header
        assert!(plan.is_phased());
        assert_eq!(plan.phases.len(), 1);
        // The Tasks section is NOT treated as a simple plan tasks section
        // because we already have phases
        assert!(plan.tasks.is_empty());
    }

    #[test]
    fn test_parse_importable_plan_stage_alias() {
        let content = r#"# Plan with Stages

## Stage 1: Setup

### Configure

### Initialize
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert!(plan.is_phased());
        assert_eq!(plan.phases[0].number, "1");
        assert_eq!(plan.phases[0].name, "Setup");
        assert_eq!(plan.phases[0].tasks.len(), 2);
    }

    #[test]
    fn test_parse_importable_plan_multiline_task_body() {
        let content = r#"# Plan

## Tasks

### Complex Task

This is the first paragraph.

This is the second paragraph with **bold** text.

- A bullet point
- Another bullet point

#### Sub-heading in task

More content under sub-heading.
"#;

        let plan = parse_importable_plan(content).unwrap();
        let body = plan.tasks[0].body.as_ref().unwrap();
        assert!(body.contains("first paragraph"));
        assert!(body.contains("second paragraph"));
        assert!(body.contains("**bold**") || body.contains("bold")); // Depends on rendering
        assert!(body.contains("bullet point"));
        assert!(body.contains("Sub-heading"));
    }
}
