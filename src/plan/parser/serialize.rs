//! Plan serialization functions
//!
//! Serializes `PlanMetadata` structures back to markdown format for writing to disk.

use crate::plan::types::{FreeFormSection, Phase, PlanMetadata, PlanSection, TicketsSection};

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
        output.push_str(&format!("id: {id}\n"));
    }
    if let Some(ref uuid) = metadata.uuid {
        output.push_str(&format!("uuid: {uuid}\n"));
    }
    if let Some(ref created) = metadata.created {
        output.push_str(&format!("created: {created}\n"));
    }
    // Write any extra/unknown frontmatter fields for round-trip preservation
    if let Some(ref extra) = metadata.extra_frontmatter {
        let mut keys: Vec<&String> = extra.keys().collect();
        keys.sort(); // deterministic output order
        for key in keys {
            let value = &extra[key];
            let yaml_str = serde_yaml_ng::to_string(value).unwrap_or_default();
            let yaml_str = yaml_str.trim_end();
            output.push_str(&format!("{key}: {yaml_str}\n"));
        }
    }
    output.push_str("---\n");

    // 2. Generate H1 title
    if let Some(ref title) = metadata.title {
        output.push_str(&format!("# {title}\n"));
    }

    // 3. Generate description (preamble)
    if let Some(ref description) = metadata.description {
        output.push('\n');
        output.push_str(description);
        output.push('\n');
    }

    // 4. Generate Acceptance Criteria section if present
    if !metadata.acceptance_criteria.is_empty()
        || metadata.acceptance_criteria_raw.is_some()
        || !metadata.acceptance_criteria_extra.is_empty()
    {
        output.push_str("\n## Acceptance Criteria\n\n");
        if let Some(ref raw) = metadata.acceptance_criteria_raw {
            // Use raw content verbatim for round-trip fidelity — preserves
            // non-list prose (paragraphs, code blocks, etc.)
            output.push_str(raw);
            if !raw.ends_with('\n') {
                output.push('\n');
            }
        } else {
            // Fallback: generate from parsed list items (programmatically constructed plans)
            for criterion in &metadata.acceptance_criteria {
                output.push_str(&format!("- {criterion}\n"));
            }
        }
        for extra in &metadata.acceptance_criteria_extra {
            output.push_str(&format!("\n### {}\n", extra.heading));
            if !extra.content.is_empty() {
                output.push('\n');
                output.push_str(&extra.content);
                if !extra.content.ends_with('\n') {
                    output.push('\n');
                }
            }
        }
    }

    // 5. Generate sections in stored order
    for section in &metadata.sections {
        output.push('\n');
        match section {
            PlanSection::Phase(phase) => {
                output.push_str(&serialize_phase(phase));
            }
            PlanSection::Tickets(ts) => {
                output.push_str(&serialize_tickets_section(ts));
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
///
/// ### Custom Section
///
/// Preserved verbatim...
/// ```
///
/// When `subsection_order` is present, subsections are emitted in their
/// original order. Otherwise, the default order is: Success Criteria, Tickets,
/// then any extra subsections.
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

    if phase.subsection_order.is_empty() {
        // No subsection order recorded — use default order (backwards-compatible)
        serialize_phase_success_criteria(&mut output, phase);
        serialize_phase_tickets(&mut output, phase);
        serialize_phase_extra_subsections(&mut output, phase);
    } else {
        // Emit subsections in recorded order
        let mut extra_index = 0;
        for key in &phase.subsection_order {
            match key.as_str() {
                "success criteria" => serialize_phase_success_criteria(&mut output, phase),
                "tickets" => serialize_phase_tickets(&mut output, phase),
                _ => {
                    // Emit the next extra subsection
                    if let Some(extra) = phase.extra_subsections.get(extra_index) {
                        output.push_str(&format!("\n### {}\n", extra.heading));
                        if !extra.content.is_empty() {
                            output.push('\n');
                            output.push_str(&extra.content);
                            if !extra.content.ends_with('\n') {
                                output.push('\n');
                            }
                        }
                        extra_index += 1;
                    }
                }
            }
        }
    }

    output
}

/// Serialize success criteria subsection for a phase.
fn serialize_phase_success_criteria(output: &mut String, phase: &Phase) {
    if phase.success_criteria_raw.is_some() || !phase.success_criteria.is_empty() {
        output.push_str("\n### Success Criteria\n\n");
        if let Some(ref raw) = phase.success_criteria_raw {
            // Use raw content verbatim for round-trip fidelity — preserves
            // non-list prose (paragraphs, code blocks, etc.)
            output.push_str(raw);
            if !raw.ends_with('\n') {
                output.push('\n');
            }
        } else {
            // Fallback: generate from parsed list items (programmatically constructed phases)
            for criterion in &phase.success_criteria {
                output.push_str(&format!("- {criterion}\n"));
            }
        }
    }
}

/// Serialize tickets subsection for a phase.
fn serialize_phase_tickets(output: &mut String, phase: &Phase) {
    if phase.tickets_raw.is_some() || !phase.tickets.is_empty() {
        output.push_str("\n### Tickets\n\n");
        if let Some(ref raw) = phase.tickets_raw {
            // Use raw content verbatim for round-trip fidelity — preserves
            // ticket descriptions (e.g., "1. j-a1b2 - Add cache dependencies")
            output.push_str(raw);
            if !raw.ends_with('\n') {
                output.push('\n');
            }
        } else {
            // Fallback: generate numbered list from ticket IDs (programmatically constructed phases)
            for (i, ticket) in phase.tickets.iter().enumerate() {
                output.push_str(&format!("{}. {}\n", i + 1, ticket));
            }
        }
    }
}

/// Serialize any extra (unknown) subsections for a phase.
fn serialize_phase_extra_subsections(output: &mut String, phase: &Phase) {
    for extra in &phase.extra_subsections {
        output.push_str(&format!("\n### {}\n", extra.heading));
        if !extra.content.is_empty() {
            output.push('\n');
            output.push_str(&extra.content);
            if !extra.content.ends_with('\n') {
                output.push('\n');
            }
        }
    }
}

/// Serialize a tickets section (for simple plans) to markdown format,
/// including any H3 subsections that were present under `## Tickets`.
///
/// Output format:
/// ```text
/// ## Tickets
///
/// 1. ticket-id-1
/// 2. ticket-id-2
///
/// ### Ordering Notes
///
/// Extra subsection content...
/// ```
fn serialize_tickets_section(ts: &TicketsSection) -> String {
    let mut output = String::new();

    output.push_str("## Tickets\n\n");
    if let Some(ref raw) = ts.tickets_raw {
        // Use raw content verbatim for round-trip fidelity — preserves
        // ticket descriptions (e.g., "1. j-a1b2 - Add cache dependencies")
        output.push_str(raw);
        if !raw.ends_with('\n') {
            output.push('\n');
        }
    } else {
        // Fallback: generate numbered list from ticket IDs (programmatically constructed plans)
        for (i, ticket) in ts.tickets.iter().enumerate() {
            output.push_str(&format!("{}. {}\n", i + 1, ticket));
        }
    }

    for extra in &ts.extra_subsections {
        output.push_str(&format!("\n### {}\n", extra.heading));
        if !extra.content.is_empty() {
            output.push('\n');
            output.push_str(&extra.content);
            if !extra.content.ends_with('\n') {
                output.push('\n');
            }
        }
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
    use crate::plan::parser::parse_plan_content;
    use crate::types::{CreatedAt, PlanId};

    // ==================== Serialization Tests ====================

    #[test]
    fn test_serialize_simple_plan() {
        let metadata = PlanMetadata {
            id: Some(PlanId::new_unchecked("plan-a1b2")),
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".to_string()),
            created: Some(CreatedAt::new_unchecked("2024-01-01T00:00:00Z")),
            title: Some("Simple Plan Title".to_string()),
            description: Some("This is the plan description.".to_string()),
            acceptance_criteria: vec![
                "All tests pass".to_string(),
                "Documentation complete".to_string(),
            ],
            acceptance_criteria_raw: None,
            acceptance_criteria_extra: vec![],
            sections: vec![PlanSection::Tickets(TicketsSection::new(vec![
                "j-a1b2".to_string(),
                "j-c3d4".to_string(),
                "j-e5f6".to_string(),
            ]))],
            file_path: None,
            extra_frontmatter: None,
        };

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
        let mut phase1 = Phase::new("1", "Infrastructure");
        phase1.description = Some("Set up the foundational components.".to_string());
        phase1.success_criteria = vec![
            "Database tables created".to_string(),
            "Helper functions work".to_string(),
        ];
        phase1.tickets = vec!["j-a1b2".to_string(), "j-c3d4".to_string()];

        let mut phase2 = Phase::new("2", "Implementation");
        phase2.tickets = vec!["j-e5f6".to_string()];

        let metadata = PlanMetadata {
            id: Some(PlanId::new_unchecked("plan-b2c3")),
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".to_string()),
            created: Some(CreatedAt::new_unchecked("2024-01-01T00:00:00Z")),
            title: Some("Phased Plan".to_string()),
            description: Some("Overview of the plan.".to_string()),
            acceptance_criteria: vec!["Performance targets met".to_string()],
            acceptance_criteria_raw: None,
            acceptance_criteria_extra: vec![],
            sections: vec![PlanSection::Phase(phase1), PlanSection::Phase(phase2)],
            file_path: None,
            extra_frontmatter: None,
        };

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
        // Add free-form section
        let mut metadata = PlanMetadata {
            id: Some(PlanId::new_unchecked("plan-c3d4")),
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".to_string()),
            created: Some(CreatedAt::new_unchecked("2024-01-01T00:00:00Z")),
            title: Some("Plan with Free-form Content".to_string()),
            description: Some("Description.".to_string()),
            acceptance_criteria: vec![],
            acceptance_criteria_raw: None,
            acceptance_criteria_extra: vec![],
            sections: vec![PlanSection::FreeForm(FreeFormSection::new(
                "Overview",
                "### Motivation\n\nThis section explains why we're doing this.",
            ))],
            file_path: None,
            extra_frontmatter: None,
        };

        // Add phase
        let phase = Phase::new("1", "Setup");
        let mut phase = phase;
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
    fn test_serialize_tickets_section_helper() {
        let ts = TicketsSection::new(vec![
            "j-a1b2".to_string(),
            "j-c3d4".to_string(),
            "j-e5f6".to_string(),
        ]);
        let serialized = serialize_tickets_section(&ts);

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
uuid: 550e8400-e29b-41d4-a716-446655440200
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
uuid: 550e8400-e29b-41d4-a716-446655440300
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
uuid: 550e8400-e29b-41d4-a716-446655440400
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
                    assert_eq!(o.tickets, n.tickets);
                }
                _ => panic!("Section type mismatch"),
            }
        }
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

    // ==================== Unknown H3 Subsection Round-Trip Tests ====================

    #[test]
    fn test_roundtrip_phase_with_unknown_h3_subsections() {
        let original = r#"---
id: plan-extra
uuid: 550e8400-e29b-41d4-a716-446655440500
created: 2024-01-01T00:00:00Z
---
# Plan with Custom Phase Subsections

## Phase 1: Infrastructure

Set up the foundational components.

### Success Criteria

- Database tables created correctly
- Helper functions work

### Implementation Notes

These are custom notes that the user added.
They should survive a round-trip without data loss.

### Tickets

1. j-dep1
2. j-mod2

### Risk Assessment

- Risk 1: Might be slow under load
- Risk 2: Compatibility with older systems
"#;

        // Parse
        let metadata = parse_plan_content(original).unwrap();

        let phases = metadata.phases();
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].extra_subsections.len(), 2);
        assert_eq!(
            phases[0].extra_subsections[0].heading,
            "Implementation Notes"
        );
        assert_eq!(phases[0].extra_subsections[1].heading, "Risk Assessment");

        // Verify subsection order preserved
        assert_eq!(
            phases[0].subsection_order,
            vec![
                "success criteria",
                "implementation notes",
                "tickets",
                "risk assessment"
            ]
        );

        // Serialize
        let serialized = serialize_plan(&metadata);

        // Verify custom subsections appear in serialized output
        assert!(serialized.contains("### Implementation Notes"));
        assert!(serialized.contains("custom notes that the user added"));
        assert!(serialized.contains("### Risk Assessment"));
        assert!(serialized.contains("Risk 1: Might be slow"));

        // Verify ordering in serialized output:
        // Success Criteria < Implementation Notes < Tickets < Risk Assessment
        let sc_pos = serialized.find("### Success Criteria").unwrap();
        let notes_pos = serialized.find("### Implementation Notes").unwrap();
        let tickets_pos = serialized.find("### Tickets").unwrap();
        let risk_pos = serialized.find("### Risk Assessment").unwrap();
        assert!(
            sc_pos < notes_pos,
            "Success Criteria should come before Implementation Notes"
        );
        assert!(
            notes_pos < tickets_pos,
            "Implementation Notes should come before Tickets"
        );
        assert!(
            tickets_pos < risk_pos,
            "Tickets should come before Risk Assessment"
        );

        // Parse again to verify round-trip
        let reparsed = parse_plan_content(&serialized).unwrap();

        let new_phases = reparsed.phases();
        assert_eq!(new_phases.len(), 1);
        assert_eq!(new_phases[0].success_criteria, phases[0].success_criteria);
        assert_eq!(new_phases[0].tickets, phases[0].tickets);
        assert_eq!(new_phases[0].extra_subsections.len(), 2);
        assert_eq!(
            new_phases[0].extra_subsections[0].heading,
            "Implementation Notes"
        );
        assert_eq!(
            new_phases[0].extra_subsections[1].heading,
            "Risk Assessment"
        );
        assert_eq!(new_phases[0].subsection_order, phases[0].subsection_order);
    }

    #[test]
    fn test_roundtrip_phase_unknown_h3_only() {
        // Phase with only unknown subsections (no success criteria or tickets)
        let original = r#"---
id: plan-custom-only
uuid: 550e8400-e29b-41d4-a716-446655440501
created: 2024-01-01T00:00:00Z
---
# Plan with Only Custom Subsections

## Phase 1: Research

Background research phase.

### References

- Paper A: "Efficient caching strategies"
- Paper B: "Distributed systems primer"

### Open Questions

1. What cache eviction policy to use?
2. How to handle network partitions?
"#;

        let metadata = parse_plan_content(original).unwrap();
        let phases = metadata.phases();
        assert_eq!(phases[0].extra_subsections.len(), 2);
        assert!(phases[0].success_criteria.is_empty());
        assert!(phases[0].tickets.is_empty());

        let serialized = serialize_plan(&metadata);
        assert!(serialized.contains("### References"));
        assert!(serialized.contains("### Open Questions"));
        assert!(serialized.contains("cache eviction policy"));

        let reparsed = parse_plan_content(&serialized).unwrap();
        let new_phases = reparsed.phases();
        assert_eq!(new_phases[0].extra_subsections.len(), 2);
        assert_eq!(new_phases[0].extra_subsections[0].heading, "References");
        assert_eq!(new_phases[0].extra_subsections[1].heading, "Open Questions");
    }

    #[test]
    fn test_roundtrip_multiple_phases_with_unknown_h3s() {
        let original = r#"---
id: plan-multi-extra
uuid: 550e8400-e29b-41d4-a716-446655440502
created: 2024-01-01T00:00:00Z
---
# Multi-Phase Custom Subsections

## Phase 1: Setup

### Tickets

1. j-a1b2

### Dependencies

- Requires Node.js 18+
- Requires PostgreSQL 15

## Phase 2: Core

### Design Rationale

We chose this approach because of X, Y, Z.

### Tickets

1. j-c3d4
2. j-e5f6

### Caveats

This approach has limitations with large datasets.
"#;

        let metadata = parse_plan_content(original).unwrap();
        let serialized = serialize_plan(&metadata);
        let reparsed = parse_plan_content(&serialized).unwrap();

        let phases = reparsed.phases();
        assert_eq!(phases.len(), 2);

        // Phase 1: Tickets then Dependencies
        assert_eq!(phases[0].tickets, vec!["j-a1b2"]);
        assert_eq!(phases[0].extra_subsections.len(), 1);
        assert_eq!(phases[0].extra_subsections[0].heading, "Dependencies");
        assert!(
            phases[0].extra_subsections[0]
                .content
                .contains("Node.js 18+")
        );

        // Phase 2: Design Rationale, then Tickets, then Caveats
        assert_eq!(phases[1].tickets, vec!["j-c3d4", "j-e5f6"]);
        assert_eq!(phases[1].extra_subsections.len(), 2);
        assert_eq!(phases[1].extra_subsections[0].heading, "Design Rationale");
        assert_eq!(phases[1].extra_subsections[1].heading, "Caveats");
        assert!(
            phases[1].extra_subsections[1]
                .content
                .contains("limitations")
        );
    }

    #[test]
    fn test_serialize_phase_without_subsection_order_backwards_compatible() {
        // Phases constructed programmatically (without subsection_order) should
        // still serialize correctly with the default order
        let mut phase = Phase::new("1", "Legacy");
        phase.description = Some("A phase built without subsection_order.".to_string());
        phase.success_criteria = vec!["It works".to_string()];
        phase.tickets = vec!["j-a1b2".to_string()];
        phase.extra_subsections = vec![FreeFormSection::new("Custom", "Custom content here.")];
        // subsection_order is empty (default)

        let metadata = PlanMetadata {
            id: Some(PlanId::new_unchecked("plan-legacy")),
            uuid: Some("550e8400-e29b-41d4-a716-446655440503".to_string()),
            created: Some(CreatedAt::new_unchecked("2024-01-01T00:00:00Z")),
            title: Some("Legacy Plan".to_string()),
            description: None,
            acceptance_criteria: vec![],
            acceptance_criteria_raw: None,
            acceptance_criteria_extra: vec![],
            sections: vec![PlanSection::Phase(phase)],
            file_path: None,
            extra_frontmatter: None,
        };

        let serialized = serialize_plan(&metadata);

        // Default order: Success Criteria, Tickets, then extras
        let sc_pos = serialized.find("### Success Criteria").unwrap();
        let tickets_pos = serialized.find("### Tickets").unwrap();
        let custom_pos = serialized.find("### Custom").unwrap();
        assert!(sc_pos < tickets_pos);
        assert!(tickets_pos < custom_pos);
        assert!(serialized.contains("Custom content here."));
    }

    // ==================== Tickets Section H3 Subsection Round-Trip Tests ====================

    #[test]
    fn test_roundtrip_simple_plan_with_h3_under_tickets() {
        let original = r#"---
id: plan-tkt-h3
uuid: 550e8400-e29b-41d4-a716-446655440600
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

        // Parse
        let metadata = parse_plan_content(original).unwrap();

        assert!(metadata.is_simple());
        let tickets = metadata.all_tickets();
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4"]);

        if let PlanSection::Tickets(ts) = &metadata.sections[0] {
            assert_eq!(ts.extra_subsections.len(), 2);
            assert_eq!(ts.extra_subsections[0].heading, "Ordering Notes");
            assert_eq!(ts.extra_subsections[1].heading, "Risk Assessment");
        } else {
            panic!("Expected PlanSection::Tickets");
        }

        // Serialize
        let serialized = serialize_plan(&metadata);

        // Verify serialized output contains subsections
        assert!(serialized.contains("### Ordering Notes"));
        assert!(serialized.contains("API dependency"));
        assert!(serialized.contains("### Risk Assessment"));
        assert!(serialized.contains("Timeline pressure"));

        // Verify ordering: ticket list comes before subsections
        let tickets_header_pos = serialized.find("## Tickets").unwrap();
        let first_ticket_pos = serialized.find("1. j-a1b2").unwrap();
        let ordering_pos = serialized.find("### Ordering Notes").unwrap();
        let risk_pos = serialized.find("### Risk Assessment").unwrap();
        assert!(tickets_header_pos < first_ticket_pos);
        assert!(first_ticket_pos < ordering_pos);
        assert!(ordering_pos < risk_pos);

        // Re-parse and verify round-trip
        let reparsed = parse_plan_content(&serialized).unwrap();

        assert!(reparsed.is_simple());
        assert_eq!(reparsed.all_tickets(), vec!["j-a1b2", "j-c3d4"]);

        if let PlanSection::Tickets(ts) = &reparsed.sections[0] {
            assert_eq!(ts.extra_subsections.len(), 2);
            assert_eq!(ts.extra_subsections[0].heading, "Ordering Notes");
            assert!(ts.extra_subsections[0].content.contains("API dependency"));
            assert_eq!(ts.extra_subsections[1].heading, "Risk Assessment");
            assert!(
                ts.extra_subsections[1]
                    .content
                    .contains("Timeline pressure")
            );
        } else {
            panic!("Expected PlanSection::Tickets after round-trip");
        }
    }

    #[test]
    fn test_roundtrip_simple_plan_tickets_no_h3_unchanged() {
        let original = r#"---
id: plan-tkt-noh3
uuid: 550e8400-e29b-41d4-a716-446655440601
created: 2024-01-01T00:00:00Z
---
# Simple Plan No Subsections

## Tickets

1. j-a1b2
2. j-c3d4
"#;

        let metadata = parse_plan_content(original).unwrap();
        let serialized = serialize_plan(&metadata);
        let reparsed = parse_plan_content(&serialized).unwrap();

        assert!(reparsed.is_simple());
        assert_eq!(reparsed.all_tickets(), vec!["j-a1b2", "j-c3d4"]);

        if let PlanSection::Tickets(ts) = &reparsed.sections[0] {
            assert!(ts.extra_subsections.is_empty());
        } else {
            panic!("Expected PlanSection::Tickets");
        }
    }

    #[test]
    fn test_serialize_tickets_section_with_extra_subsections() {
        let ts = TicketsSection {
            tickets: vec!["j-a1b2".to_string(), "j-c3d4".to_string()],
            tickets_raw: None,
            extra_subsections: vec![
                FreeFormSection::new("Notes", "Some important notes."),
                FreeFormSection::new("Dependencies", "- Requires external API"),
            ],
        };
        let serialized = serialize_tickets_section(&ts);

        assert!(serialized.contains("## Tickets"));
        assert!(serialized.contains("1. j-a1b2"));
        assert!(serialized.contains("2. j-c3d4"));
        assert!(serialized.contains("### Notes"));
        assert!(serialized.contains("Some important notes."));
        assert!(serialized.contains("### Dependencies"));
        assert!(serialized.contains("Requires external API"));
    }

    // ==================== Acceptance Criteria H3 Subsection Round-Trip Tests ====================

    #[test]
    fn test_roundtrip_acceptance_criteria_with_h3_subsections() {
        let original = r#"---
id: plan-ac-rt
uuid: 550e8400-e29b-41d4-a716-446655440700
created: 2024-01-01T00:00:00Z
---
# Plan with AC Subsections

Description here.

## Acceptance Criteria

- All tests pass
- Documentation complete

### Testing Notes

Detailed testing instructions here.
Run the full integration suite before merging.

### Verification Steps

1. Deploy to staging
2. Run smoke tests
3. Check metrics

## Tickets

1. j-a1b2
2. j-c3d4
"#;

        // Parse
        let metadata = parse_plan_content(original).unwrap();

        assert_eq!(metadata.acceptance_criteria.len(), 2);
        assert_eq!(metadata.acceptance_criteria_extra.len(), 2);
        assert_eq!(
            metadata.acceptance_criteria_extra[0].heading,
            "Testing Notes"
        );
        assert_eq!(
            metadata.acceptance_criteria_extra[1].heading,
            "Verification Steps"
        );

        // Serialize
        let serialized = serialize_plan(&metadata);

        // Verify serialized output contains the H3 subsections
        assert!(serialized.contains("## Acceptance Criteria"));
        assert!(serialized.contains("- All tests pass"));
        assert!(serialized.contains("- Documentation complete"));
        assert!(serialized.contains("### Testing Notes"));
        assert!(serialized.contains("Detailed testing instructions"));
        assert!(serialized.contains("### Verification Steps"));
        assert!(serialized.contains("Deploy to staging"));

        // Verify ordering: criteria list before subsections, subsections before Tickets
        let criteria_pos = serialized.find("- All tests pass").unwrap();
        let notes_pos = serialized.find("### Testing Notes").unwrap();
        let steps_pos = serialized.find("### Verification Steps").unwrap();
        let tickets_pos = serialized.find("## Tickets").unwrap();
        assert!(criteria_pos < notes_pos);
        assert!(notes_pos < steps_pos);
        assert!(steps_pos < tickets_pos);

        // Re-parse and verify round-trip
        let reparsed = parse_plan_content(&serialized).unwrap();

        assert_eq!(reparsed.acceptance_criteria, metadata.acceptance_criteria);
        assert_eq!(
            reparsed.acceptance_criteria_extra.len(),
            metadata.acceptance_criteria_extra.len()
        );
        assert_eq!(
            reparsed.acceptance_criteria_extra[0].heading,
            "Testing Notes"
        );
        assert!(
            reparsed.acceptance_criteria_extra[0]
                .content
                .contains("Detailed testing instructions")
        );
        assert_eq!(
            reparsed.acceptance_criteria_extra[1].heading,
            "Verification Steps"
        );
        assert!(
            reparsed.acceptance_criteria_extra[1]
                .content
                .contains("Deploy to staging")
        );
    }

    #[test]
    fn test_roundtrip_inline_formatting_in_criteria() {
        let original = r#"---
id: plan-fmt-rt
uuid: 550e8400-e29b-41d4-a716-446655440800
created: 2024-01-01T00:00:00Z
---
# Inline Formatting Round-Trip

## Acceptance Criteria

- Performance must be **under 5ms** per lookup
- Use the `TicketStore` API
- See [design doc](https://example.com) for details

## Phase 1: Infrastructure

### Success Criteria

- Database responds in **under 10ms**
- The `cache_init()` function returns `Ok`

### Tickets

1. j-a1b2
"#;

        // Parse
        let metadata = parse_plan_content(original).unwrap();

        // Verify formatting preserved after first parse
        assert!(metadata.acceptance_criteria[0].contains("**under 5ms**"));
        assert!(metadata.acceptance_criteria[1].contains("`TicketStore`"));
        assert!(metadata.acceptance_criteria[2].contains("[design doc](https://example.com)"));

        let phases = metadata.phases();
        assert!(phases[0].success_criteria[0].contains("**under 10ms**"));
        assert!(phases[0].success_criteria[1].contains("`cache_init()`"));

        // Serialize
        let serialized = serialize_plan(&metadata);

        // Verify serialized output contains formatting
        assert!(
            serialized.contains("**under 5ms**"),
            "Serialized output missing bold: {}",
            serialized
        );
        assert!(
            serialized.contains("`TicketStore`"),
            "Serialized output missing code span: {}",
            serialized
        );
        assert!(
            serialized.contains("[design doc](https://example.com)"),
            "Serialized output missing link: {}",
            serialized
        );
        assert!(
            serialized.contains("**under 10ms**"),
            "Serialized output missing bold in success criteria: {}",
            serialized
        );

        // Re-parse and verify round-trip
        let reparsed = parse_plan_content(&serialized).unwrap();

        assert_eq!(
            reparsed.acceptance_criteria, metadata.acceptance_criteria,
            "Acceptance criteria not preserved through round-trip"
        );

        let new_phases = reparsed.phases();
        assert_eq!(
            new_phases[0].success_criteria, phases[0].success_criteria,
            "Success criteria not preserved through round-trip"
        );
    }

    #[test]
    fn test_roundtrip_acceptance_criteria_no_h3_unchanged() {
        let original = r#"---
id: plan-ac-noh3
uuid: 550e8400-e29b-41d4-a716-446655440701
created: 2024-01-01T00:00:00Z
---
# Plan without AC Subsections

## Acceptance Criteria

- All tests pass
- Documentation complete

## Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(original).unwrap();
        assert_eq!(metadata.acceptance_criteria.len(), 2);
        assert!(metadata.acceptance_criteria_extra.is_empty());

        let serialized = serialize_plan(&metadata);
        let reparsed = parse_plan_content(&serialized).unwrap();

        assert_eq!(reparsed.acceptance_criteria, metadata.acceptance_criteria);
        assert!(reparsed.acceptance_criteria_extra.is_empty());
    }

    #[test]
    fn test_roundtrip_acceptance_criteria_only_h3_no_list_items() {
        let original = r#"---
id: plan-ac-h3only
uuid: 550e8400-e29b-41d4-a716-446655440702
created: 2024-01-01T00:00:00Z
---
# Plan with Only AC Subsections

## Acceptance Criteria

### Testing Notes

Important testing information that must not be lost.

## Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(original).unwrap();
        assert!(metadata.acceptance_criteria.is_empty());
        assert_eq!(metadata.acceptance_criteria_extra.len(), 1);
        assert_eq!(
            metadata.acceptance_criteria_extra[0].heading,
            "Testing Notes"
        );

        let serialized = serialize_plan(&metadata);

        // The section should still be emitted because of the extra subsections
        assert!(serialized.contains("## Acceptance Criteria"));
        assert!(serialized.contains("### Testing Notes"));
        assert!(serialized.contains("Important testing information"));

        let reparsed = parse_plan_content(&serialized).unwrap();
        assert!(reparsed.acceptance_criteria.is_empty());
        assert_eq!(reparsed.acceptance_criteria_extra.len(), 1);
        assert_eq!(
            reparsed.acceptance_criteria_extra[0].heading,
            "Testing Notes"
        );
    }

    // ==================== Acceptance Criteria Raw Prose Round-Trip Tests ====================

    #[test]
    fn test_roundtrip_acceptance_criteria_with_prose_before_and_after_list() {
        let original = r#"---
id: plan-ac-prose
uuid: 550e8400-e29b-41d4-a716-446655440900
created: 2024-01-01T00:00:00Z
---
# Plan with AC Prose

## Acceptance Criteria

The following conditions must all be met:

- Criterion 1
- Criterion 2

Additional context about testing requirements...

## Tickets

1. j-a1b2
"#;

        // Parse
        let metadata = parse_plan_content(original).unwrap();

        // List items are still extracted for programmatic access
        // (Note: parse_list_items may include trailing prose in the last item —
        //  that's the pre-existing limitation. The raw field fixes serialization.)
        assert!(!metadata.acceptance_criteria.is_empty());
        assert!(metadata.acceptance_criteria[0].contains("Criterion 1"));

        // Raw content is stored — this is the key improvement
        assert!(metadata.acceptance_criteria_raw.is_some());
        let raw = metadata.acceptance_criteria_raw.as_ref().unwrap();
        assert!(
            raw.contains("following conditions must all be met"),
            "Raw should contain preamble prose, got: {}",
            raw
        );
        assert!(raw.contains("Criterion 1"));
        assert!(raw.contains("Criterion 2"));
        assert!(
            raw.contains("Additional context about testing requirements"),
            "Raw should contain postamble prose, got: {}",
            raw
        );

        // Serialize
        let serialized = serialize_plan(&metadata);

        // Prose is preserved in serialized output
        assert!(
            serialized.contains("following conditions must all be met"),
            "Preamble prose lost in serialization: {}",
            serialized
        );
        assert!(
            serialized.contains("Additional context about testing requirements"),
            "Postamble prose lost in serialization: {}",
            serialized
        );
        assert!(serialized.contains("Criterion 1"));
        assert!(serialized.contains("Criterion 2"));

        // Re-parse and verify full round-trip
        let reparsed = parse_plan_content(&serialized).unwrap();
        assert!(!reparsed.acceptance_criteria.is_empty());
        assert!(reparsed.acceptance_criteria_raw.is_some());
        let raw2 = reparsed.acceptance_criteria_raw.as_ref().unwrap();
        assert!(raw2.contains("following conditions must all be met"));
        assert!(raw2.contains("Additional context about testing requirements"));
    }

    #[test]
    fn test_roundtrip_acceptance_criteria_with_code_block() {
        let original = r#"---
id: plan-ac-code
uuid: 550e8400-e29b-41d4-a716-446655440901
created: 2024-01-01T00:00:00Z
---
# Plan with Code in AC

## Acceptance Criteria

- API returns correct status codes
- Response format matches spec

Example response:

```json
{"status": "ok", "count": 42}
```

## Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(original).unwrap();

        // List items extracted
        assert_eq!(metadata.acceptance_criteria.len(), 2);

        // Raw includes the code block (comrak may render ```json as ``` json with a space)
        assert!(metadata.acceptance_criteria_raw.is_some());
        let raw = metadata.acceptance_criteria_raw.as_ref().unwrap();
        assert!(
            raw.contains("json"),
            "Raw should contain json code fence, got: {}",
            raw
        );
        assert!(raw.contains(r#""status": "ok""#));

        // Round-trip preserves the code block
        let serialized = serialize_plan(&metadata);
        assert!(
            serialized.contains("json"),
            "Serialized should contain json code fence, got: {}",
            serialized
        );
        assert!(serialized.contains(r#""status": "ok""#));
        assert!(serialized.contains("Example response:"));

        let reparsed = parse_plan_content(&serialized).unwrap();
        assert_eq!(reparsed.acceptance_criteria.len(), 2);
        let reparsed_raw = reparsed.acceptance_criteria_raw.as_ref().unwrap();
        assert!(
            reparsed_raw.contains(r#""status": "ok""#),
            "Reparsed raw should preserve code block content, got: {}",
            reparsed_raw
        );
    }

    #[test]
    fn test_serialize_programmatic_plan_without_raw_falls_back_to_list() {
        // Programmatically constructed plan — no raw content set
        let metadata = PlanMetadata {
            id: Some(PlanId::new_unchecked("plan-prog")),
            uuid: Some("550e8400-e29b-41d4-a716-446655440902".to_string()),
            created: Some(CreatedAt::new_unchecked("2024-01-01T00:00:00Z")),
            title: Some("Programmatic Plan".to_string()),
            description: None,
            acceptance_criteria: vec![
                "First criterion".to_string(),
                "Second criterion".to_string(),
            ],
            acceptance_criteria_raw: None,
            acceptance_criteria_extra: vec![],
            sections: vec![PlanSection::Tickets(TicketsSection::new(vec![
                "j-a1b2".to_string(),
            ]))],
            file_path: None,
            extra_frontmatter: None,
        };

        let serialized = serialize_plan(&metadata);

        // Falls back to generating from list items
        assert!(serialized.contains("- First criterion"));
        assert!(serialized.contains("- Second criterion"));

        // Re-parse works correctly
        let reparsed = parse_plan_content(&serialized).unwrap();
        assert_eq!(reparsed.acceptance_criteria.len(), 2);
        assert_eq!(reparsed.acceptance_criteria[0], "First criterion");
        assert_eq!(reparsed.acceptance_criteria[1], "Second criterion");
    }

    #[test]
    fn test_roundtrip_acceptance_criteria_prose_between_lists() {
        let original = r#"---
id: plan-ac-between
uuid: 550e8400-e29b-41d4-a716-446655440903
created: 2024-01-01T00:00:00Z
---
# Plan with Prose Between Lists

## Acceptance Criteria

Performance requirements:

- Response time < 100ms
- Memory usage < 512MB

Reliability requirements:

- 99.9% uptime
- Graceful degradation

## Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(original).unwrap();

        // All 4 list items extracted
        assert_eq!(metadata.acceptance_criteria.len(), 4);

        // Raw content preserves prose between lists
        assert!(metadata.acceptance_criteria_raw.is_some());
        let raw = metadata.acceptance_criteria_raw.as_ref().unwrap();
        assert!(raw.contains("Performance requirements:"));
        assert!(raw.contains("Reliability requirements:"));

        // Serialize and verify prose survives
        let serialized = serialize_plan(&metadata);
        assert!(serialized.contains("Performance requirements:"));
        assert!(serialized.contains("Reliability requirements:"));

        // Second round-trip
        let reparsed = parse_plan_content(&serialized).unwrap();
        assert_eq!(reparsed.acceptance_criteria.len(), 4);
        let raw2 = reparsed.acceptance_criteria_raw.as_ref().unwrap();
        assert!(raw2.contains("Performance requirements:"));
        assert!(raw2.contains("Reliability requirements:"));
    }

    // ==================== Phase Success Criteria Raw Prose Round-Trip Tests ====================

    #[test]
    fn test_roundtrip_phase_success_criteria_with_prose() {
        let original = r#"---
id: plan-sc-prose
uuid: 550e8400-e29b-41d4-a716-446655441000
created: 2024-01-01T00:00:00Z
---
# Plan with Success Criteria Prose

## Phase 1: Infrastructure

Set up the foundational components.

### Success Criteria

All of the following must be verified:

- Database tables created
- Helper functions work

Run the validation script to confirm.

### Tickets

1. j-a1b2
2. j-c3d4
"#;

        // Parse
        let metadata = parse_plan_content(original).unwrap();

        let phases = metadata.phases();
        assert_eq!(phases.len(), 1);

        // List items are still extracted for programmatic access
        // (Note: parse_list_items may include trailing prose in the last item —
        //  that's a pre-existing limitation. The raw field fixes serialization.)
        assert!(!phases[0].success_criteria.is_empty());
        assert!(phases[0].success_criteria[0].contains("Database tables created"));

        // Raw content is stored — this is the key improvement
        assert!(phases[0].success_criteria_raw.is_some());
        let raw = phases[0].success_criteria_raw.as_ref().unwrap();
        assert!(
            raw.contains("All of the following must be verified:"),
            "Raw should contain preamble prose, got: {}",
            raw
        );
        assert!(raw.contains("Database tables created"));
        assert!(raw.contains("Helper functions work"));
        assert!(
            raw.contains("Run the validation script to confirm."),
            "Raw should contain postamble prose, got: {}",
            raw
        );

        // Serialize
        let serialized = serialize_plan(&metadata);

        // Prose is preserved in serialized output
        assert!(
            serialized.contains("All of the following must be verified:"),
            "Preamble prose lost in serialization: {}",
            serialized
        );
        assert!(
            serialized.contains("Run the validation script to confirm."),
            "Postamble prose lost in serialization: {}",
            serialized
        );
        assert!(serialized.contains("Database tables created"));
        assert!(serialized.contains("Helper functions work"));

        // Re-parse and verify full round-trip
        let reparsed = parse_plan_content(&serialized).unwrap();
        let new_phases = reparsed.phases();
        assert_eq!(new_phases[0].success_criteria.len(), 2);
        assert!(new_phases[0].success_criteria_raw.is_some());
        let raw2 = new_phases[0].success_criteria_raw.as_ref().unwrap();
        assert!(raw2.contains("All of the following must be verified:"));
        assert!(raw2.contains("Run the validation script to confirm."));
    }

    #[test]
    fn test_roundtrip_phase_success_criteria_with_code_block() {
        let original = r#"---
id: plan-sc-code
uuid: 550e8400-e29b-41d4-a716-446655441001
created: 2024-01-01T00:00:00Z
---
# Plan with Code in SC

## Phase 1: Setup

### Success Criteria

- API returns correct status codes
- Response format matches spec

Example validation:

```bash
curl -s http://localhost:8080/health | jq .status
```

### Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(original).unwrap();
        let phases = metadata.phases();

        // List items extracted
        assert_eq!(phases[0].success_criteria.len(), 2);

        // Raw includes the code block
        assert!(phases[0].success_criteria_raw.is_some());
        let raw = phases[0].success_criteria_raw.as_ref().unwrap();
        assert!(
            raw.contains("Example validation:"),
            "Raw should contain prose before code block, got: {}",
            raw
        );
        assert!(
            raw.contains("curl"),
            "Raw should contain code block content, got: {}",
            raw
        );

        // Round-trip preserves the code block
        let serialized = serialize_plan(&metadata);
        assert!(serialized.contains("Example validation:"));
        assert!(serialized.contains("curl"));

        let reparsed = parse_plan_content(&serialized).unwrap();
        let new_phases = reparsed.phases();
        assert_eq!(new_phases[0].success_criteria.len(), 2);
        let reparsed_raw = new_phases[0].success_criteria_raw.as_ref().unwrap();
        assert!(reparsed_raw.contains("curl"));
    }

    #[test]
    fn test_roundtrip_phase_success_criteria_prose_between_lists() {
        let original = r#"---
id: plan-sc-between
uuid: 550e8400-e29b-41d4-a716-446655441002
created: 2024-01-01T00:00:00Z
---
# Plan with Prose Between SC Lists

## Phase 1: Core

### Success Criteria

Performance requirements:

- Response time < 100ms
- Memory usage < 512MB

Reliability requirements:

- 99.9% uptime
- Graceful degradation

### Tickets

1. j-a1b2
"#;

        let metadata = parse_plan_content(original).unwrap();
        let phases = metadata.phases();

        // All 4 list items extracted
        assert_eq!(phases[0].success_criteria.len(), 4);

        // Raw content preserves prose between lists
        assert!(phases[0].success_criteria_raw.is_some());
        let raw = phases[0].success_criteria_raw.as_ref().unwrap();
        assert!(raw.contains("Performance requirements:"));
        assert!(raw.contains("Reliability requirements:"));

        // Serialize and verify prose survives
        let serialized = serialize_plan(&metadata);
        assert!(serialized.contains("Performance requirements:"));
        assert!(serialized.contains("Reliability requirements:"));

        // Second round-trip
        let reparsed = parse_plan_content(&serialized).unwrap();
        let new_phases = reparsed.phases();
        assert_eq!(new_phases[0].success_criteria.len(), 4);
        let raw2 = new_phases[0].success_criteria_raw.as_ref().unwrap();
        assert!(raw2.contains("Performance requirements:"));
        assert!(raw2.contains("Reliability requirements:"));
    }

    #[test]
    fn test_serialize_programmatic_phase_without_raw_falls_back_to_list() {
        // Programmatically constructed phase — no raw content set
        let mut phase = Phase::new("1", "Programmatic");
        phase.description = Some("A programmatic phase.".to_string());
        phase.success_criteria = vec![
            "First criterion".to_string(),
            "Second criterion".to_string(),
        ];
        phase.tickets = vec!["j-a1b2".to_string()];

        let metadata = PlanMetadata {
            id: Some(PlanId::new_unchecked("plan-sc-prog")),
            uuid: Some("550e8400-e29b-41d4-a716-446655441003".to_string()),
            created: Some(CreatedAt::new_unchecked("2024-01-01T00:00:00Z")),
            title: Some("Programmatic Plan".to_string()),
            description: None,
            acceptance_criteria: vec![],
            acceptance_criteria_raw: None,
            acceptance_criteria_extra: vec![],
            sections: vec![PlanSection::Phase(phase)],
            file_path: None,
            extra_frontmatter: None,
        };

        let serialized = serialize_plan(&metadata);

        // Falls back to generating from list items
        assert!(serialized.contains("### Success Criteria"));
        assert!(serialized.contains("- First criterion"));
        assert!(serialized.contains("- Second criterion"));

        // Re-parse works correctly
        let reparsed = parse_plan_content(&serialized).unwrap();
        let new_phases = reparsed.phases();
        assert_eq!(new_phases[0].success_criteria.len(), 2);
        assert_eq!(new_phases[0].success_criteria[0], "First criterion");
        assert_eq!(new_phases[0].success_criteria[1], "Second criterion");
    }

    // ==================== Ticket List Description Round-Trip Tests ====================

    #[test]
    fn test_roundtrip_phased_plan_ticket_descriptions_preserved() {
        let original = r#"---
id: plan-tkt-desc
uuid: 550e8400-e29b-41d4-a716-446655442000
created: 2024-01-01T00:00:00Z
---
# Plan with Ticket Descriptions

## Phase 1: Infrastructure

### Tickets

1. j-dep1 - Add cache dependencies
2. j-mod2 - Create src/cache.rs with basic structure
3. j-cfg3 (optional: low priority)
"#;

        // Parse
        let metadata = parse_plan_content(original).unwrap();
        assert!(metadata.is_phased());

        let phases = metadata.phases();
        assert_eq!(phases[0].tickets, vec!["j-dep1", "j-mod2", "j-cfg3"]);

        // Raw content stored
        assert!(phases[0].tickets_raw.is_some());
        let raw = phases[0].tickets_raw.as_ref().unwrap();
        assert!(
            raw.contains("Add cache dependencies"),
            "Raw should contain ticket description, got: {}",
            raw
        );
        assert!(raw.contains("Create src/cache.rs"));
        assert!(raw.contains("optional: low priority"));

        // Serialize
        let serialized = serialize_plan(&metadata);

        // Descriptions preserved in serialized output
        assert!(
            serialized.contains("Add cache dependencies"),
            "Ticket description lost in serialization: {}",
            serialized
        );
        assert!(serialized.contains("Create src/cache.rs"));
        assert!(serialized.contains("optional: low priority"));

        // Re-parse and verify full round-trip
        let reparsed = parse_plan_content(&serialized).unwrap();
        let new_phases = reparsed.phases();
        assert_eq!(new_phases[0].tickets, vec!["j-dep1", "j-mod2", "j-cfg3"]);
        assert!(new_phases[0].tickets_raw.is_some());
        let raw2 = new_phases[0].tickets_raw.as_ref().unwrap();
        assert!(raw2.contains("Add cache dependencies"));
        assert!(raw2.contains("Create src/cache.rs"));
    }

    #[test]
    fn test_roundtrip_simple_plan_ticket_descriptions_preserved() {
        let original = r#"---
id: plan-tkt-desc-simple
uuid: 550e8400-e29b-41d4-a716-446655442001
created: 2024-01-01T00:00:00Z
---
# Simple Plan with Ticket Descriptions

## Tickets

1. j-a1b2 - Add cache dependencies
2. j-c3d4 - Implement sync algorithm
3. j-e5f6 (nice to have)
"#;

        // Parse
        let metadata = parse_plan_content(original).unwrap();
        assert!(metadata.is_simple());

        let tickets = metadata.all_tickets();
        assert_eq!(tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);

        // Raw content stored
        if let PlanSection::Tickets(ts) = &metadata.sections[0] {
            assert!(ts.tickets_raw.is_some());
            let raw = ts.tickets_raw.as_ref().unwrap();
            assert!(
                raw.contains("Add cache dependencies"),
                "Raw should contain ticket description, got: {}",
                raw
            );
            assert!(raw.contains("Implement sync algorithm"));
            assert!(raw.contains("nice to have"));
        } else {
            panic!("Expected PlanSection::Tickets");
        }

        // Serialize
        let serialized = serialize_plan(&metadata);

        // Descriptions preserved in serialized output
        assert!(
            serialized.contains("Add cache dependencies"),
            "Ticket description lost in serialization: {}",
            serialized
        );
        assert!(serialized.contains("Implement sync algorithm"));
        assert!(serialized.contains("nice to have"));

        // Re-parse and verify full round-trip
        let reparsed = parse_plan_content(&serialized).unwrap();
        assert!(reparsed.is_simple());
        assert_eq!(reparsed.all_tickets(), vec!["j-a1b2", "j-c3d4", "j-e5f6"]);

        if let PlanSection::Tickets(ts) = &reparsed.sections[0] {
            assert!(ts.tickets_raw.is_some());
            let raw = ts.tickets_raw.as_ref().unwrap();
            assert!(raw.contains("Add cache dependencies"));
            assert!(raw.contains("Implement sync algorithm"));
        } else {
            panic!("Expected PlanSection::Tickets after round-trip");
        }
    }

    #[test]
    fn test_roundtrip_design_section_from_import() {
        // Simulates what cmd_plan_import does: constructs PlanMetadata with a Design
        // freeform section from the parsed ImportablePlan.design field, then verifies
        // the design content survives serialize -> parse round-trip.
        let design_content = "### Architecture\n\nThe system uses a modular design.\n\n### Key Decisions\n\n1. Decision one\n2. Decision two";

        let mut phase = Phase::new("1", "Setup");
        phase.tickets = vec!["j-a1b2".to_string()];

        let metadata = PlanMetadata {
            id: Some(PlanId::new_unchecked("plan-design-rt")),
            uuid: Some("550e8400-e29b-41d4-a716-446655440999".to_string()),
            created: Some(CreatedAt::new_unchecked("2024-01-01T00:00:00Z")),
            title: Some("Plan with Design Section".to_string()),
            description: Some("Overview of the plan.".to_string()),
            acceptance_criteria: vec!["All tests pass".to_string()],
            acceptance_criteria_raw: None,
            acceptance_criteria_extra: vec![],
            sections: vec![
                PlanSection::FreeForm(FreeFormSection::new("Design", design_content)),
                PlanSection::Phase(phase),
            ],
            file_path: None,
            extra_frontmatter: None,
        };

        // Serialize
        let serialized = serialize_plan(&metadata);

        // Verify Design section present in serialized output
        assert!(
            serialized.contains("## Design"),
            "Serialized output should contain Design section heading, got:\n{}",
            serialized
        );
        assert!(
            serialized.contains("Architecture"),
            "Serialized output should contain Architecture content, got:\n{}",
            serialized
        );
        assert!(
            serialized.contains("Key Decisions"),
            "Serialized output should contain Key Decisions content, got:\n{}",
            serialized
        );
        assert!(
            serialized.contains("Decision one"),
            "Serialized output should contain decision content, got:\n{}",
            serialized
        );

        // Verify ordering: Design before Phase
        let design_pos = serialized.find("## Design").unwrap();
        let phase_pos = serialized.find("## Phase 1").unwrap();
        assert!(
            design_pos < phase_pos,
            "Design section should come before Phase section"
        );

        // Re-parse and verify round-trip
        let reparsed = parse_plan_content(&serialized).unwrap();

        assert_eq!(reparsed.id.as_deref(), Some("plan-design-rt"));
        assert_eq!(reparsed.title, Some("Plan with Design Section".to_string()));
        assert_eq!(
            reparsed.description,
            Some("Overview of the plan.".to_string())
        );

        // Design should be a freeform section
        let freeform = reparsed.free_form_sections();
        assert_eq!(
            freeform.len(),
            1,
            "Should have exactly one freeform section"
        );
        assert_eq!(freeform[0].heading, "Design");
        assert!(
            freeform[0].content.contains("Architecture"),
            "Design content should contain Architecture, got: {}",
            freeform[0].content
        );
        assert!(
            freeform[0].content.contains("Key Decisions"),
            "Design content should contain Key Decisions, got: {}",
            freeform[0].content
        );
        assert!(
            freeform[0].content.contains("Decision one"),
            "Design content should contain decision details, got: {}",
            freeform[0].content
        );

        // Phases still intact
        let phases = reparsed.phases();
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].tickets, vec!["j-a1b2"]);

        // Acceptance criteria still intact
        assert_eq!(reparsed.acceptance_criteria.len(), 1);
        assert_eq!(reparsed.acceptance_criteria[0], "All tests pass");
    }

    #[test]
    fn test_serialize_programmatic_plan_tickets_without_raw_falls_back_to_numbered_list() {
        // Programmatically constructed plan — no tickets_raw set
        let mut phase = Phase::new("1", "Programmatic");
        phase.tickets = vec!["j-a1b2".to_string(), "j-c3d4".to_string()];
        // tickets_raw is None by default

        let metadata = PlanMetadata {
            id: Some(PlanId::new_unchecked("plan-tkt-prog")),
            uuid: Some("550e8400-e29b-41d4-a716-446655442002".to_string()),
            created: Some(CreatedAt::new_unchecked("2024-01-01T00:00:00Z")),
            title: Some("Programmatic Ticket Plan".to_string()),
            description: None,
            acceptance_criteria: vec![],
            acceptance_criteria_raw: None,
            acceptance_criteria_extra: vec![],
            sections: vec![
                PlanSection::Phase(phase),
                PlanSection::Tickets(TicketsSection::new(vec![
                    "j-e5f6".to_string(),
                    "j-g7h8".to_string(),
                ])),
            ],
            file_path: None,
            extra_frontmatter: None,
        };

        let serialized = serialize_plan(&metadata);

        // Falls back to generating numbered list from ticket IDs
        assert!(
            serialized.contains("1. j-a1b2"),
            "Phase tickets should be numbered, got: {}",
            serialized
        );
        assert!(serialized.contains("2. j-c3d4"));
        assert!(serialized.contains("1. j-e5f6"));
        assert!(serialized.contains("2. j-g7h8"));

        // Re-parse works correctly
        let reparsed = parse_plan_content(&serialized).unwrap();
        let all = reparsed.all_tickets();
        assert_eq!(all, vec!["j-a1b2", "j-c3d4", "j-e5f6", "j-g7h8"]);
    }
}
