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
use serde::Deserialize;

use crate::error::{JanusError, Result};
use crate::plan::types::{
    FreeFormSection, ImportValidationError, ImportablePhase, ImportablePlan, ImportableTask, Phase,
    PlanMetadata, PlanSection,
};

/// Plan frontmatter struct for YAML deserialization
#[derive(Debug, Deserialize, Default)]
struct PlanFrontmatter {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created: Option<String>,
}

// ============================================================================
// Section Alias Constants (for plan import)
// ============================================================================

/// Recognized section names for acceptance criteria (case-insensitive)
pub const ACCEPTANCE_CRITERIA_ALIASES: &[&str] = &["acceptance criteria"];

/// Required section name for design (case-insensitive)
pub const DESIGN_SECTION_NAME: &str = "design";

/// Required section name for implementation wrapper (case-insensitive)
pub const IMPLEMENTATION_SECTION_NAME: &str = "implementation";

/// Regex pattern for matching phase headers in importable plans (at H3 level)
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
    let frontmatter_re =
        Regex::new(r"(?s)^---\n(.*?)\n---\n(.*)$").expect("frontmatter regex should be valid");

    let captures = frontmatter_re
        .captures(content)
        .ok_or_else(|| JanusError::InvalidFormat("missing YAML frontmatter".to_string()))?;

    let yaml = captures.get(1).map(|m| m.as_str()).unwrap_or("");
    let body = captures.get(2).map(|m| m.as_str()).unwrap_or("");

    Ok((yaml, body))
}

/// Parse YAML frontmatter into PlanMetadata fields
fn parse_yaml_frontmatter(yaml: &str) -> Result<PlanMetadata> {
    let frontmatter: PlanFrontmatter = serde_yaml_ng::from_str(yaml)
        .map_err(|e| JanusError::InvalidFormat(format!("YAML parsing error: {}", e)))?;

    let metadata = PlanMetadata {
        id: frontmatter.id,
        uuid: frontmatter.uuid,
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

    // Process H2 sections
    let mut found_acceptance_criteria = false;
    let mut found_tickets_section = false;

    for section in h2_sections {
        classify_and_add_section(
            section,
            metadata,
            &mut found_acceptance_criteria,
            &mut found_tickets_section,
        );
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

/// Try to parse a heading as a phase header
/// Matches: "Phase 1: Name", "Phase 2a - Name", "Phase 10:", "Phase 1" (no separator)
fn try_parse_phase_header(heading: &str) -> Option<(String, String)> {
    // Pattern: "Phase" followed by number/letter combo, optional separator and name
    let phase_re = Regex::new(r"(?i)^phase\s+(\d+[a-z]?)\s*(?:[-:]\s*)?(.*)$")
        .expect("phase regex should be valid");

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
    let item_re = Regex::new(r"(?m)^[\s]*[-*+][\s]+(.+)$|^[\s]*\d+\.[\s]+(.+)$")
        .expect("item list regex should be valid");

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
    let item_re = Regex::new(r"(?m)^[\s]*(?:[-*+]|\d+\.)\s+([\w-]+)")
        .expect("ticket item regex should be valid");

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
    let phase_re = Regex::new(PHASE_PATTERN).expect("phase regex should be valid");

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

/// Parse an importable plan document.
///
/// This is the main entry point for parsing AI-generated plan documents.
///
/// # Expected Format
///
/// ```markdown
/// # Plan Title (required)
///
/// Introductory paragraph(s).
///
/// ## Design (required)
///
/// Design details, architecture, reasoning.
///
/// ## Acceptance Criteria (optional)
///
/// - Criterion 1
/// - Criterion 2
///
/// ## Implementation (required)
///
/// ### Phase 1: Phase Name
///
/// Phase description.
///
/// #### Task Title
///
/// Task description.
/// ```
///
/// # Arguments
/// * `content` - The raw markdown content
///
/// # Returns
/// `Ok(ImportablePlan)` if parsing succeeds, `Err(JanusError::ImportFailed)` if validation fails.
///
/// # Validation Rules
/// 1. Document must have an H1 title
/// 2. Document must have a `## Design` section
/// 3. Document must have a `## Implementation` section
/// 4. Implementation section must contain at least one `### Phase N:` section
/// 5. Each phase must contain at least one `#### Task` header
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

    // 2. Extract description (content between H1 and first H2)
    let description = extract_import_description(root, &options);

    // 3. Extract Design section (required)
    let design = extract_h2_section_content(root, DESIGN_SECTION_NAME, &options);
    if design.is_none() {
        errors.push(
            ImportValidationError::new("Missing required \"## Design\" section").with_hint(
                "Add a \"## Design\" section with design details, architecture, and reasoning",
            ),
        );
    }

    // 4. Extract optional Acceptance Criteria section
    let acceptance_criteria = extract_import_acceptance_criteria(root);

    // 5. Validate Implementation section exists
    let has_implementation = has_h2_section(root, IMPLEMENTATION_SECTION_NAME);
    if !has_implementation {
        errors.push(
            ImportValidationError::new("Missing required \"## Implementation\" section").with_hint(
                "Add a \"## Implementation\" section containing \"### Phase N:\" subsections",
            ),
        );
    }

    // 6. Parse phases from within Implementation section (H3 headers)
    let phases = if has_implementation {
        parse_phases_from_implementation(root, &options)
    } else {
        Vec::new()
    };

    // 7. Validate at least one phase exists
    if has_implementation && phases.is_empty() {
        errors.push(
            ImportValidationError::new("Implementation section has no phases")
                .with_hint("Add \"### Phase 1: Name\" subsections under ## Implementation"),
        );
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
        design,
        acceptance_criteria,
        phases,
    })
}

/// Check if a document has an H2 section with the given name (case-insensitive)
fn has_h2_section<'a>(root: &'a AstNode<'a>, section_name: &str) -> bool {
    let section_lower = section_name.to_lowercase();
    for node in root.children() {
        if let NodeValue::Heading(heading) = &node.data.borrow().value
            && heading.level == 2
        {
            let text = extract_text_content(node);
            if text.trim().to_lowercase() == section_lower {
                return true;
            }
        }
    }
    false
}

/// Extract the content of an H2 section by name (case-insensitive)
fn extract_h2_section_content<'a>(
    root: &'a AstNode<'a>,
    section_name: &str,
    options: &Options,
) -> Option<String> {
    let section_lower = section_name.to_lowercase();
    let mut in_section = false;
    let mut content = String::new();

    for node in root.children() {
        match &node.data.borrow().value {
            NodeValue::Heading(heading) => {
                if heading.level == 2 {
                    let text = extract_text_content(node);
                    if in_section {
                        // Hit another H2, end of our section
                        break;
                    }
                    if text.trim().to_lowercase() == section_lower {
                        in_section = true;
                    }
                } else if in_section {
                    // H3+ headers are part of section content
                    content.push_str(&render_node_to_markdown(node, options));
                }
            }
            _ => {
                if in_section {
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

/// Parse phases from within the Implementation section.
///
/// Looks for H3 headers matching the phase pattern under the `## Implementation` section.
/// Tasks are H4 headers within each phase.
fn parse_phases_from_implementation<'a>(
    root: &'a AstNode<'a>,
    options: &Options,
) -> Vec<ImportablePhase> {
    let nodes: Vec<_> = root.children().collect();

    // Find the Implementation section boundaries
    let Some((impl_start, impl_end)) = find_implementation_section_bounds(&nodes) else {
        return Vec::new();
    };

    // Collect phase header positions
    let phase_headers = collect_phase_headers(&nodes, impl_start, impl_end);
    if phase_headers.is_empty() {
        return Vec::new();
    }

    // Build phases from the collected headers
    build_phases_from_headers(&nodes, &phase_headers, impl_end, options)
}

/// Find the start and end indices of the Implementation section.
///
/// Returns `Some((start, end))` where:
/// - `start` is the index of the `## Implementation` heading
/// - `end` is the index of the next H2 heading (or end of document)
fn find_implementation_section_bounds<'a>(nodes: &[&'a AstNode<'a>]) -> Option<(usize, usize)> {
    let mut impl_start = None;

    for (idx, node) in nodes.iter().enumerate() {
        let NodeValue::Heading(heading) = &node.data.borrow().value else {
            continue;
        };
        if heading.level != 2 {
            continue;
        }

        let text = extract_text_content(node);
        if text.trim().to_lowercase() == IMPLEMENTATION_SECTION_NAME {
            impl_start = Some(idx);
        } else if let Some(start) = impl_start {
            // Found another H2 after Implementation
            return Some((start, idx));
        }
    }

    // Implementation section extends to end of document
    impl_start.map(|start| (start, nodes.len()))
}

/// A phase header with its position and parsed info
struct PhaseHeader {
    index: usize,
    number: String,
    name: String,
}

/// Collect all H3 phase headers within the Implementation section.
fn collect_phase_headers<'a>(
    nodes: &[&'a AstNode<'a>],
    impl_start: usize,
    impl_end: usize,
) -> Vec<PhaseHeader> {
    nodes[impl_start + 1..impl_end]
        .iter()
        .enumerate()
        .filter_map(|(relative_idx, node)| {
            let NodeValue::Heading(heading) = &node.data.borrow().value else {
                return None;
            };
            if heading.level != 3 {
                return None;
            }

            let text = extract_text_content(node);
            let (number, name) = is_phase_header(&text)?;

            Some(PhaseHeader {
                index: impl_start + 1 + relative_idx,
                number,
                name,
            })
        })
        .collect()
}

/// Build ImportablePhase structs from collected phase headers.
fn build_phases_from_headers<'a>(
    nodes: &[&'a AstNode<'a>],
    phase_headers: &[PhaseHeader],
    impl_end: usize,
    options: &Options,
) -> Vec<ImportablePhase> {
    phase_headers
        .iter()
        .enumerate()
        .map(|(i, header)| {
            // Phase content ends at the next phase header or end of Implementation section
            let end_idx = phase_headers
                .get(i + 1)
                .map(|h| h.index)
                .unwrap_or(impl_end);

            build_single_phase(nodes, header, end_idx, options)
        })
        .collect()
}

/// Build a single ImportablePhase from a phase header.
fn build_single_phase<'a>(
    nodes: &[&'a AstNode<'a>],
    header: &PhaseHeader,
    end_idx: usize,
    options: &Options,
) -> ImportablePhase {
    let phase_nodes = &nodes[header.index + 1..end_idx];
    let description = extract_phase_description(phase_nodes, options);
    let mut tasks = parse_tasks_from_phase_h4(nodes, header.index + 1, end_idx, options);

    // Create fallback task if no H4 tasks found
    if tasks.is_empty() {
        tasks.push(create_fallback_task(
            &header.number,
            &header.name,
            &description,
        ));
    }

    ImportablePhase {
        number: header.number.clone(),
        name: header.name.clone(),
        description,
        tasks,
    }
}

/// Extract phase description (content between H3 header and first H4).
fn extract_phase_description<'a>(nodes: &[&'a AstNode<'a>], options: &Options) -> Option<String> {
    let description: String = nodes
        .iter()
        .take_while(|node| {
            !matches!(
                &node.data.borrow().value,
                NodeValue::Heading(h) if h.level == 4
            )
        })
        .map(|node| render_node_to_markdown(node, options))
        .collect();

    let trimmed = description.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Create a fallback task when a phase has no H4 tasks.
fn create_fallback_task(number: &str, name: &str, description: &Option<String>) -> ImportableTask {
    let title = if name.is_empty() {
        format!("Implement Phase {}", number)
    } else {
        format!("Implement Phase {}: {}", number, name)
    };

    ImportableTask {
        title,
        body: description.clone(),
        is_complete: false,
    }
}

/// Parse tasks from H4 headers within a phase section.
fn parse_tasks_from_phase_h4<'a>(
    nodes: &[&'a AstNode<'a>],
    start_idx: usize,
    end_idx: usize,
    options: &Options,
) -> Vec<ImportableTask> {
    let mut tasks = Vec::new();
    let mut current_h4: Option<(String, bool)> = None; // (title, is_complete)
    let mut current_h4_content = String::new();

    for node in &nodes[start_idx..end_idx] {
        match &node.data.borrow().value {
            NodeValue::Heading(heading) => {
                if heading.level == 4 {
                    // Finalize any pending H4 task
                    if let Some((title, is_complete)) = current_h4.take() {
                        tasks.push(ImportableTask {
                            title,
                            body: if current_h4_content.trim().is_empty() {
                                None
                            } else {
                                Some(current_h4_content.trim().to_string())
                            },
                            is_complete,
                        });
                    }

                    let text = extract_text_content(node);
                    let (title, is_complete) = is_completed_task(&text);
                    current_h4 = Some((title, is_complete));
                    current_h4_content = String::new();
                } else if heading.level >= 5 {
                    // H5+ content within an H4 task
                    if current_h4.is_some() {
                        current_h4_content.push_str(&render_node_to_markdown(node, options));
                    }
                }
                // H3 shouldn't appear here (would be a new phase), ignore
            }
            _ => {
                if current_h4.is_some() {
                    // Content within an H4 task
                    current_h4_content.push_str(&render_node_to_markdown(node, options));
                }
            }
        }
    }

    // Finalize any pending H4 task
    if let Some((title, is_complete)) = current_h4.take() {
        tasks.push(ImportableTask {
            title,
            body: if current_h4_content.trim().is_empty() {
                None
            } else {
                Some(current_h4_content.trim().to_string())
            },
            is_complete,
        });
    }

    tasks
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
        // The only accepted alias is now "acceptance criteria"
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

        // Non-matches (aliases no longer supported)
        assert!(!is_section_alias("goals", ACCEPTANCE_CRITERIA_ALIASES));
        assert!(!is_section_alias("tasks", ACCEPTANCE_CRITERIA_ALIASES));
        assert!(!is_section_alias("acceptance", ACCEPTANCE_CRITERIA_ALIASES));
        assert!(!is_section_alias("criteria", ACCEPTANCE_CRITERIA_ALIASES));
    }

    #[test]
    fn test_design_and_implementation_section_names() {
        // Test the constant values
        assert_eq!(DESIGN_SECTION_NAME, "design");
        assert_eq!(IMPLEMENTATION_SECTION_NAME, "implementation");
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

    // ==================== New Format Import Tests ====================

    #[test]
    fn test_parse_importable_plan_basic() {
        let content = r#"# Implementation Plan

Overview of the implementation.

## Design

This is the design section with architecture details.

### Architecture

The system uses a modular design.

## Implementation

### Phase 1: Infrastructure

Set up the foundational components.

#### Add Dependencies

Add the required dependencies to Cargo.toml.

#### Create Module Structure

Create the basic module structure.

### Phase 2: Core Logic

Implement the core logic.

#### Implement Core Function

The main implementation task.
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert_eq!(plan.title, "Implementation Plan");
        assert_eq!(
            plan.description,
            Some("Overview of the implementation.".to_string())
        );
        assert!(plan.design.is_some());
        assert!(plan.design.as_ref().unwrap().contains("design section"));
        assert!(plan.is_phased());
        assert_eq!(plan.phases.len(), 2);

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
        assert_eq!(plan.phases[1].name, "Core Logic");
        assert_eq!(plan.phases[1].tasks.len(), 1);
        assert_eq!(plan.phases[1].tasks[0].title, "Implement Core Function");
    }

    #[test]
    fn test_parse_importable_plan_with_acceptance_criteria() {
        let content = r#"# Plan with Criteria

## Design

Design details.

## Acceptance Criteria

- All tests pass
- Documentation complete

## Implementation

### Phase 1: Setup

#### Task One

Description.
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert_eq!(plan.acceptance_criteria.len(), 2);
        assert_eq!(plan.acceptance_criteria[0], "All tests pass");
        assert_eq!(plan.acceptance_criteria[1], "Documentation complete");
    }

    #[test]
    fn test_parse_importable_plan_completed_h4_tasks() {
        let content = r#"# Plan with Completed Tasks

## Design

Design info.

## Implementation

### Phase 1: Tasks

#### Task One [x]

This task is done.

#### Task Two

This task is pending.
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert_eq!(plan.phases[0].tasks.len(), 2);
        assert_eq!(plan.phases[0].tasks[0].title, "Task One");
        assert!(plan.phases[0].tasks[0].is_complete);
        assert_eq!(plan.phases[0].tasks[1].title, "Task Two");
        assert!(!plan.phases[0].tasks[1].is_complete);
    }

    #[test]
    fn test_parse_importable_plan_with_code_blocks() {
        let content = r#"# Plan with Code

## Design

Technical design.

## Implementation

### Phase 1: Coding

#### Add Cache Support

Implement caching in the service.

```rust
let cache = HashMap::new();
```

Key changes:
- Add cache data structure
- Modify speak() method
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert_eq!(plan.phases[0].tasks.len(), 1);
        assert_eq!(plan.phases[0].tasks[0].title, "Add Cache Support");
        let body = plan.phases[0].tasks[0].body.as_ref().unwrap();
        assert!(body.contains("Implement caching"));
        assert!(body.contains("HashMap::new()"));
        assert!(body.contains("Key changes:"));
    }

    #[test]
    fn test_parse_importable_plan_missing_title() {
        let content = r#"Just some content without H1.

## Design

Design.

## Implementation

### Phase 1: Test

#### Task one
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
    fn test_parse_importable_plan_missing_design() {
        let content = r#"# Plan without Design

## Implementation

### Phase 1: Test

#### Task one

Description.
"#;

        let result = parse_importable_plan(content);
        assert!(result.is_err());
        if let Err(crate::error::JanusError::ImportFailed { issues, .. }) = result {
            assert!(issues.iter().any(|s| s.contains("Design")));
        } else {
            panic!("Expected ImportFailed error");
        }
    }

    #[test]
    fn test_parse_importable_plan_missing_implementation() {
        let content = r#"# Plan without Implementation

## Design

Design details.
"#;

        let result = parse_importable_plan(content);
        assert!(result.is_err());
        if let Err(crate::error::JanusError::ImportFailed { issues, .. }) = result {
            assert!(issues.iter().any(|s| s.contains("Implementation")));
        } else {
            panic!("Expected ImportFailed error");
        }
    }

    #[test]
    fn test_parse_importable_plan_phase_without_h4_creates_fallback_task() {
        let content = r#"# Plan with Phase Without H4 Tasks

## Design

Design details.

## Implementation

### Phase 1: Setup

This phase sets up the infrastructure.
It has no explicit H4 task headers.
"#;

        let result = parse_importable_plan(content);
        assert!(result.is_ok(), "Should succeed with fallback task");

        let plan = result.unwrap();
        assert_eq!(plan.phases.len(), 1);

        let phase = &plan.phases[0];
        assert_eq!(phase.number, "1");
        assert_eq!(phase.name, "Setup");
        assert_eq!(phase.tasks.len(), 1, "Should have one fallback task");

        let task = &phase.tasks[0];
        assert_eq!(task.title, "Implement Phase 1: Setup");
        assert!(task.body.is_some());
        assert!(
            task.body
                .as_ref()
                .unwrap()
                .contains("sets up the infrastructure")
        );
        assert!(!task.is_complete);
    }

    #[test]
    fn test_parse_importable_plan_mixed_phases_with_and_without_h4s() {
        let content = r#"# Mixed Phase Plan

## Design

A plan with some phases having explicit tasks and others without.

## Implementation

### Phase 1: Foundation

This phase has explicit H4 tasks.

#### Set Up Database

Create the database schema.

#### Configure Environment

Set up environment variables.

### Phase 2: Integration

This phase has no H4 tasks, just a description.
The implementation details are described here in prose.

### Phase 3: Testing

Another phase with explicit tasks.

#### Write Unit Tests

Cover all core functions.

#### Write Integration Tests

Test the full workflow.
"#;

        let result = parse_importable_plan(content);
        assert!(result.is_ok(), "Should parse successfully");

        let plan = result.unwrap();
        assert_eq!(plan.phases.len(), 3);

        // Phase 1: Has explicit H4 tasks
        let phase1 = &plan.phases[0];
        assert_eq!(phase1.number, "1");
        assert_eq!(phase1.name, "Foundation");
        assert_eq!(phase1.tasks.len(), 2);
        assert_eq!(phase1.tasks[0].title, "Set Up Database");
        assert_eq!(phase1.tasks[1].title, "Configure Environment");

        // Phase 2: No H4 tasks, should have fallback
        let phase2 = &plan.phases[1];
        assert_eq!(phase2.number, "2");
        assert_eq!(phase2.name, "Integration");
        assert_eq!(phase2.tasks.len(), 1, "Should have one fallback task");
        assert_eq!(phase2.tasks[0].title, "Implement Phase 2: Integration");
        assert!(phase2.tasks[0].body.is_some());
        assert!(
            phase2.tasks[0]
                .body
                .as_ref()
                .unwrap()
                .contains("no H4 tasks")
        );

        // Phase 3: Has explicit H4 tasks
        let phase3 = &plan.phases[2];
        assert_eq!(phase3.number, "3");
        assert_eq!(phase3.name, "Testing");
        assert_eq!(phase3.tasks.len(), 2);
        assert_eq!(phase3.tasks[0].title, "Write Unit Tests");
        assert_eq!(phase3.tasks[1].title, "Write Integration Tests");

        // Total task count should include fallback
        assert_eq!(plan.task_count(), 5);
    }

    #[test]
    fn test_parse_importable_plan_no_phases_in_implementation() {
        let content = r#"# Plan with No Phases

## Design

Design details.

## Implementation

Just content, no phases.
"#;

        let result = parse_importable_plan(content);
        assert!(result.is_err());
        if let Err(crate::error::JanusError::ImportFailed { issues, .. }) = result {
            assert!(issues.iter().any(|s| s.contains("no phases")));
        } else {
            panic!("Expected ImportFailed error");
        }
    }

    #[test]
    fn test_parse_importable_plan_task_count() {
        let content = r#"# Multi-Phase Plan

## Design

Design.

## Implementation

### Phase 1: First

#### Task 1

Description.

#### Task 2

Description.

### Phase 2: Second

#### Task 3

Description.

#### Task 4

Description.

#### Task 5

Description.
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert_eq!(plan.task_count(), 5);
        assert_eq!(plan.all_tasks().len(), 5);
    }

    #[test]
    fn test_parse_importable_plan_stage_alias() {
        let content = r#"# Plan with Stages

## Design

Design.

## Implementation

### Stage 1: Setup

#### Configure

Config task.

#### Initialize

Init task.
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

## Design

Design details.

## Implementation

### Phase 1: Work

#### Complex Task

This is the first paragraph.

This is the second paragraph with **bold** text.

- A bullet point
- Another bullet point

##### Sub-heading in task

More content under sub-heading.
"#;

        let plan = parse_importable_plan(content).unwrap();
        let body = plan.phases[0].tasks[0].body.as_ref().unwrap();
        assert!(body.contains("first paragraph"));
        assert!(body.contains("second paragraph"));
        assert!(body.contains("**bold**") || body.contains("bold")); // Depends on rendering
        assert!(body.contains("bullet point"));
        assert!(body.contains("Sub-heading"));
    }

    #[test]
    fn test_parse_importable_plan_design_with_nested_headers() {
        let content = r#"# Complex Plan

## Design

### Architecture

The system architecture.

### Key Decisions

1. Decision one
2. Decision two

## Implementation

### Phase 1: Setup

#### First Task

Task description.
"#;

        let plan = parse_importable_plan(content).unwrap();
        let design = plan.design.as_ref().unwrap();
        assert!(design.contains("Architecture"));
        assert!(design.contains("Key Decisions"));
        assert!(design.contains("Decision one"));
    }
}
