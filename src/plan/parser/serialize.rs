//! Plan serialization functions
//!
//! Serializes `PlanMetadata` structures back to markdown format for writing to disk.

use crate::plan::types::{FreeFormSection, Phase, PlanMetadata, PlanSection};

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
    use crate::plan::parser::parse_plan_content;

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
