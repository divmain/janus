//! Importable plan parser
//!
//! Parses AI-generated plan documents into `ImportablePlan` structures
//! for creating tickets and plans from external documents.

use std::sync::LazyLock;

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};
use regex::Regex;

use crate::error::{JanusError, Result};
use crate::plan::types::{
    ImportValidationError, ImportablePhase, ImportablePlan, ImportableTask,
    display_import_validation_error,
};

use super::{extract_text_content, render_node_to_markdown};

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

/// Compiled regex for matching phase headers.
/// This is the shared source of truth used by both plan parsing and import parsing.
pub static PHASE_HEADER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(PHASE_PATTERN).expect("phase regex should be valid"));

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
    let normalized = content.replace("\r\n", "\n");
    let arena = Arena::new();
    let options = Options::default();
    let root = parse_document(&arena, &normalized, &options);

    let mut errors: Vec<ImportValidationError> = Vec::new();

    // 1. Extract title
    let title = match extract_title(root) {
        Some(t) => t,
        None => {
            errors.push(ImportValidationError {
                line: Some(1),
                message: "Missing plan title (expected H1 heading)".to_string(),
                hint: Some("Add \"# Your Plan Title\" at the start of the document".to_string()),
            });
            String::new()
        }
    };

    // 2. Extract description (content between H1 and first H2)
    let description = extract_import_description(root, &options);

    // 3. Extract Design section (required)
    let design = extract_h2_section_content(root, DESIGN_SECTION_NAME, &options);
    if design.is_none() {
        errors.push(ImportValidationError {
            line: find_last_heading_line(root),
            message: "Missing required \"## Design\" section".to_string(),
            hint: Some(
                "Add a \"## Design\" section with design details, architecture, and reasoning"
                    .to_string(),
            ),
        });
    }

    // 4. Extract optional Acceptance Criteria section
    let acceptance_criteria = extract_import_acceptance_criteria(root);

    // 5. Validate Implementation section exists
    let impl_section_line = find_h2_section_line(root, IMPLEMENTATION_SECTION_NAME);
    let has_implementation = impl_section_line.is_some();
    if !has_implementation {
        errors.push(ImportValidationError {
            line: find_last_heading_line(root),
            message: "Missing required \"## Implementation\" section".to_string(),
            hint: Some(
                "Add a \"## Implementation\" section containing \"### Phase N:\" subsections"
                    .to_string(),
            ),
        });
    }

    // 6. Parse phases from within Implementation section (H3 headers)
    let (phases, implementation_preamble) = if has_implementation {
        parse_phases_from_implementation(root, &options)
    } else {
        (Vec::new(), None)
    };

    // 7. Validate at least one phase exists
    if has_implementation && phases.is_empty() {
        errors.push(ImportValidationError {
            line: impl_section_line,
            message: "Implementation section has no phases".to_string(),
            hint: Some("Add \"### Phase 1: Name\" subsections under ## Implementation".to_string()),
        });
    }

    // If there are errors, return them
    if !errors.is_empty() {
        let issues: Vec<String> = errors.iter().map(display_import_validation_error).collect();
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
        implementation_preamble,
    })
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

/// Find the line number of the last heading in the document.
///
/// Useful for reporting approximate locations of missing required sections,
/// since a missing section would logically be expected after existing headings.
fn find_last_heading_line<'a>(root: &'a AstNode<'a>) -> Option<usize> {
    let mut last_line = None;
    for node in root.children() {
        if let NodeValue::Heading(_) = &node.data.borrow().value {
            last_line = Some(node.data.borrow().sourcepos.start.line);
        }
    }
    last_line
}

/// Check if a document has an H2 section with the given name (case-insensitive).
///
/// Returns the source line number of the section heading if found, `None` otherwise.
fn find_h2_section_line<'a>(root: &'a AstNode<'a>, section_name: &str) -> Option<usize> {
    let section_lower = section_name.to_lowercase();
    for node in root.children() {
        let data = node.data.borrow();
        if let NodeValue::Heading(heading) = &data.value
            && heading.level == 2
        {
            drop(data);
            let text = extract_text_content(node);
            if text.trim().to_lowercase() == section_lower {
                return Some(node.data.borrow().sourcepos.start.line);
            }
        }
    }
    None
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
///
/// Returns `(phases, implementation_preamble)` where `implementation_preamble` contains
/// any content between the `## Implementation` heading and the first phase header.
fn parse_phases_from_implementation<'a>(
    root: &'a AstNode<'a>,
    options: &Options,
) -> (Vec<ImportablePhase>, Option<String>) {
    let nodes: Vec<_> = root.children().collect();

    // Find the Implementation section boundaries
    let Some((impl_start, impl_end)) = find_implementation_section_bounds(&nodes) else {
        return (Vec::new(), None);
    };

    // Collect phase header positions
    let phase_headers = collect_phase_headers(&nodes, impl_start, impl_end);
    if phase_headers.is_empty() {
        return (Vec::new(), None);
    }

    // Extract any preamble content between the Implementation H2 and the first phase header
    let preamble = extract_implementation_preamble(&nodes, impl_start, &phase_headers, options);

    // Build phases from the collected headers
    let phases = build_phases_from_headers(&nodes, &phase_headers, impl_end, options);
    (phases, preamble)
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

/// Extract any preamble content between the `## Implementation` heading and the first phase header.
///
/// This captures non-phase H3 headings, paragraphs, and other content that appears
/// before the first `### Phase N: Name` header within the Implementation section.
fn extract_implementation_preamble<'a>(
    nodes: &[&'a AstNode<'a>],
    impl_start: usize,
    phase_headers: &[PhaseHeader],
    options: &Options,
) -> Option<String> {
    let first_phase_idx = phase_headers[0].index;

    // Content between impl_start+1 (after the ## Implementation heading) and the first phase header
    if impl_start + 1 >= first_phase_idx {
        return None;
    }

    let preamble: String = nodes[impl_start + 1..first_phase_idx]
        .iter()
        .map(|node| render_node_to_markdown(node, options))
        .collect();

    let trimmed = preamble.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
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
        format!("Implement Phase {number}")
    } else {
        format!("Implement Phase {number}: {name}")
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

    // ==================== Section Alias Tests ====================

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

        // Non-matches (aliases no longer supported)
        assert!(!is_section_alias("goals", ACCEPTANCE_CRITERIA_ALIASES));
        assert!(!is_section_alias("tasks", ACCEPTANCE_CRITERIA_ALIASES));
        assert!(!is_section_alias("acceptance", ACCEPTANCE_CRITERIA_ALIASES));
        assert!(!is_section_alias("criteria", ACCEPTANCE_CRITERIA_ALIASES));
    }

    #[test]
    fn test_design_and_implementation_section_names() {
        assert_eq!(DESIGN_SECTION_NAME, "design");
        assert_eq!(IMPLEMENTATION_SECTION_NAME, "implementation");
    }

    // ==================== Phase Header Tests ====================

    #[test]
    fn test_is_phase_header_standard() {
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
        let result = is_phase_header("Phase 1 - Setup");
        assert_eq!(result, Some(("1".to_string(), "Setup".to_string())));
    }

    #[test]
    fn test_is_phase_header_alphanumeric() {
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
        assert!(is_phase_header("Phase Diagrams").is_none());
        assert!(is_phase_header("Phase without number").is_none());
        assert!(is_phase_header("Overview").is_none());
        assert!(is_phase_header("Tasks").is_none());
    }

    // ==================== Completed Task Tests ====================

    #[test]
    fn test_is_completed_task_h3_style() {
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

    // ==================== Import Tests ====================

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
        assert!(body.contains("**bold**") || body.contains("bold"));
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

    #[test]
    fn test_parse_importable_plan_with_crlf() {
        let content = "# CRLF Importable Plan\r\n\
\r\n\
Introduction with CRLF line endings.\r\n\
\r\n\
## Design\r\n\
\r\n\
Design details on Windows.\r\n\
\r\n\
## Implementation\r\n\
\r\n\
### Phase 1: Setup\r\n\
\r\n\
Setup description.\r\n\
\r\n\
#### Task 1\r\n\
\r\n\
First task description.\r\n\
";

        let plan = parse_importable_plan(content).unwrap();
        assert_eq!(plan.title, "CRLF Importable Plan");
        assert!(plan.description.unwrap().contains("Introduction"));
        assert!(plan.design.unwrap().contains("Design details"));
        assert_eq!(plan.phases.len(), 1);
        assert_eq!(plan.phases[0].number, "1");
        assert_eq!(plan.phases[0].tasks.len(), 1);
        assert_eq!(plan.phases[0].tasks[0].title, "Task 1");
    }

    // ==================== Implementation Preamble Tests ====================

    #[test]
    fn test_parse_importable_plan_with_implementation_preamble() {
        let content = r#"# Plan with Implementation Preamble

## Design

Design details.

## Implementation

### Architecture Overview

This section describes the high-level architecture of the implementation.

Key points:
- Modular design
- Event-driven

### Phase 1: Infrastructure

#### Set up database

Create the database schema.

### Phase 2: Core Logic

#### Implement handlers

Handler implementation.
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert_eq!(plan.phases.len(), 2);
        assert_eq!(plan.phases[0].number, "1");
        assert_eq!(plan.phases[0].name, "Infrastructure");
        assert_eq!(plan.phases[1].number, "2");
        assert_eq!(plan.phases[1].name, "Core Logic");

        // Preamble should capture the Architecture Overview H3 and its content
        let preamble = plan.implementation_preamble.as_ref().unwrap();
        assert!(
            preamble.contains("Architecture Overview"),
            "Preamble should contain the H3 heading"
        );
        assert!(
            preamble.contains("high-level architecture"),
            "Preamble should contain the paragraph content"
        );
        assert!(
            preamble.contains("Modular design"),
            "Preamble should contain list items"
        );
    }

    #[test]
    fn test_parse_importable_plan_without_implementation_preamble() {
        let content = r#"# Plan without Preamble

## Design

Design details.

## Implementation

### Phase 1: Setup

#### Task One

Description.
"#;

        let plan = parse_importable_plan(content).unwrap();
        assert!(
            plan.implementation_preamble.is_none(),
            "Should have no preamble when phases start immediately"
        );
    }

    #[test]
    fn test_parse_importable_plan_preamble_with_plain_text() {
        let content = r#"# Plan with Plain Preamble

## Design

Design details.

## Implementation

This section covers the implementation approach.
We will proceed in two phases.

### Phase 1: Foundation

#### Create base

Base task.
"#;

        let plan = parse_importable_plan(content).unwrap();
        let preamble = plan.implementation_preamble.as_ref().unwrap();
        assert!(preamble.contains("implementation approach"));
        assert!(preamble.contains("two phases"));
    }
}
