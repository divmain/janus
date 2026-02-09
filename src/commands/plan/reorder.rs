//! Plan reorder command

use std::collections::HashSet;

use serde_json::json;

use super::edit_in_editor;
use crate::commands::CommandOutput;
use crate::error::{JanusError, Result};
use crate::plan::Plan;
use crate::plan::types::PlanSection;

fn parse_and_validate_ticket_order(
    new_order: &str,
    original_tickets: &[String],
) -> Result<Vec<String>> {
    let new_ticket_order: Vec<String> = new_order
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            l.split('.')
                .nth(1)
                .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
        })
        .filter(|s| !s.is_empty())
        .collect();

    let original_set: HashSet<_> = original_tickets.iter().collect();
    let new_set: HashSet<_> = new_ticket_order.iter().collect();
    if original_set != new_set {
        return Err(JanusError::ReorderTicketMismatch);
    }

    Ok(new_ticket_order)
}

fn parse_and_validate_phase_order(
    new_order: &str,
    original_phases: &[(String, String)],
) -> Result<Vec<String>> {
    let new_phase_order: Vec<String> = new_order
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            // Extract phase number (before colon or the whole line)
            l.split(':').next().unwrap_or(l).trim().to_string()
        })
        .filter(|s| !s.is_empty())
        .collect();

    // Build set of original phase numbers
    let original_set: HashSet<_> = original_phases.iter().map(|(num, _)| num).collect();
    let new_set: HashSet<_> = new_phase_order.iter().collect();

    // Validate set equality - no phases dropped or added
    if original_set != new_set {
        return Err(JanusError::ReorderTicketMismatch);
    }

    Ok(new_phase_order)
}

/// Reorder phase sections within a plan's section list.
///
/// Non-phase sections (FreeForm, Tickets) remain in their original positions.
/// Phase slots are filled with the reordered phases in the specified order.
///
/// # Invariants
/// - The output has the same total length as the input
/// - Every non-phase section appears at its original index
/// - Phases appear in the order specified by `new_phase_order`
fn reorder_phase_sections(
    sections: Vec<PlanSection>,
    new_phase_order: &[String],
) -> Vec<PlanSection> {
    // Partition into phases and non-phases, recording original indices
    let mut phase_sections: Vec<PlanSection> = Vec::new();
    let mut other_sections: Vec<(usize, PlanSection)> = Vec::new();

    for (i, section) in sections.into_iter().enumerate() {
        match &section {
            PlanSection::Phase(_) => phase_sections.push(section),
            _ => other_sections.push((i, section)),
        }
    }

    // Sort phase_sections according to new_phase_order
    let mut ordered_phases: Vec<PlanSection> = Vec::new();
    for phase_num in new_phase_order {
        if let Some(idx) = phase_sections.iter().position(|s| {
            if let PlanSection::Phase(p) = s {
                p.number.eq_ignore_ascii_case(phase_num)
            } else {
                false
            }
        }) {
            ordered_phases.push(phase_sections.remove(idx));
        }
    }
    // Add any remaining phases that weren't in the new order
    ordered_phases.extend(phase_sections);

    // Rebuild deterministically:
    // Walk indices 0..total_len. `other_sections` is sorted by original
    // index (collected in order via `enumerate`). Peek the next non-phase
    // original index to decide what goes at each output position: if the
    // current index matches, restore the non-phase section; otherwise,
    // pull the next reordered phase.
    let total_len = ordered_phases.len() + other_sections.len();
    let mut new_sections: Vec<PlanSection> = Vec::with_capacity(total_len);
    let mut phase_iter = ordered_phases.into_iter();
    let mut other_peek = other_sections.into_iter().peekable();

    for i in 0..total_len {
        if let Some(&(orig_idx, _)) = other_peek.peek() {
            if orig_idx == i {
                let (_, section) = other_peek.next().unwrap();
                new_sections.push(section);
                continue;
            }
        }
        if let Some(phase) = phase_iter.next() {
            new_sections.push(phase);
        }
    }

    new_sections
}

/// Reorder tickets or phases interactively
///
/// # Arguments
/// * `plan_id` - The plan ID (can be partial)
/// * `phase` - Optional phase to reorder tickets within
/// * `reorder_phases` - If true, reorder phases instead of tickets
/// * `output_json` - If true, output result as JSON
pub async fn cmd_plan_reorder(
    plan_id: &str,
    phase: Option<&str>,
    reorder_phases: bool,
    output_json: bool,
) -> Result<()> {
    let plan = Plan::find(plan_id).await?;
    let mut metadata = plan.read()?;

    if reorder_phases {
        // Reorder phases
        let phases: Vec<(String, String)> = metadata
            .phases()
            .iter()
            .map(|p| (p.number.clone(), p.name.clone()))
            .collect();

        if phases.is_empty() {
            println!("No phases to reorder");
            return Ok(());
        }

        // Create a temp file with the current order
        let mut temp_content = String::new();
        for (num, name) in &phases {
            if name.is_empty() {
                temp_content.push_str(&format!("{num}\n"));
            } else {
                temp_content.push_str(&format!("{num}: {name}\n"));
            }
        }

        // Open in editor
        let new_order = edit_in_editor(&temp_content)?;
        if new_order.trim() == temp_content.trim() {
            println!("No changes made");
            return Ok(());
        }

        // Parse and validate new phase order
        let new_phase_order = parse_and_validate_phase_order(&new_order, &phases)?;

        metadata.sections = reorder_phase_sections(metadata.sections, &new_phase_order);
    } else if let Some(phase_identifier) = phase {
        // Reorder tickets within a specific phase
        let phase_obj = metadata
            .find_phase_mut(phase_identifier)
            .ok_or_else(|| JanusError::PhaseNotFound(phase_identifier.to_string()))?;

        if phase_obj.tickets.is_empty() {
            println!("No tickets to reorder in phase '{phase_identifier}'");
            return Ok(());
        }

        // Create temp content with current order
        let temp_content: String = phase_obj
            .tickets
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{}. {}\n", i + 1, t))
            .collect();

        // Open in editor
        let new_order = edit_in_editor(&temp_content)?;
        if new_order.trim() == temp_content.trim() {
            println!("No changes made");
            return Ok(());
        }

        phase_obj.tickets = parse_and_validate_ticket_order(&new_order, &phase_obj.tickets)?;
        phase_obj.tickets_raw = None; // Invalidate stale raw content
    } else if metadata.is_simple() {
        // Reorder tickets in simple plan
        let ts = metadata
            .tickets_section_mut()
            .ok_or_else(|| JanusError::PlanNoTicketsSection)?;

        if ts.tickets.is_empty() {
            println!("No tickets to reorder");
            return Ok(());
        }

        // Create temp content with current order
        let temp_content: String = ts
            .tickets
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{}. {}\n", i + 1, t))
            .collect();

        // Open in editor
        let new_order = edit_in_editor(&temp_content)?;
        if new_order.trim() == temp_content.trim() {
            println!("No changes made");
            return Ok(());
        }

        ts.tickets = parse_and_validate_ticket_order(&new_order, &ts.tickets)?;
        ts.tickets_raw = None; // Invalidate stale raw content
    } else {
        println!(
            "Use --phase to specify which phase to reorder, or --reorder-phases to reorder phases"
        );
        return Ok(());
    }

    // Write updated plan
    plan.write_metadata(&metadata)?;

    CommandOutput::new(json!({
        "plan_id": plan.id,
        "action": "reordered",
        "type": if reorder_phases { "phases" } else { "tickets" },
        "phase": phase,
    }))
    .with_text(format!("Reorder complete for plan {}", plan.id))
    .print(output_json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::types::{FreeFormSection, Phase};

    /// Helper: extract section labels for easy assertion.
    /// Phases become "Phase N", freeform becomes "FF:heading", tickets becomes "Tickets".
    fn section_labels(sections: &[PlanSection]) -> Vec<String> {
        sections
            .iter()
            .map(|s| match s {
                PlanSection::Phase(p) => format!("Phase {}", p.number),
                PlanSection::FreeForm(f) => format!("FF:{}", f.heading),
                PlanSection::Tickets(_) => "Tickets".to_string(),
            })
            .collect()
    }

    fn make_phase(num: &str, name: &str) -> PlanSection {
        PlanSection::Phase(Phase::new(num, name))
    }

    fn make_freeform(heading: &str) -> PlanSection {
        PlanSection::FreeForm(FreeFormSection::new(heading, "content"))
    }

    // ==================== reorder_phase_sections tests ====================

    #[test]
    fn test_reorder_phases_simple_swap() {
        // [Phase 1, Phase 2] -> [Phase 2, Phase 1]
        let sections = vec![make_phase("1", "First"), make_phase("2", "Second")];
        let new_order = vec!["2".to_string(), "1".to_string()];

        let result = reorder_phase_sections(sections, &new_order);
        assert_eq!(section_labels(&result), vec!["Phase 2", "Phase 1"]);
    }

    #[test]
    fn test_reorder_phases_no_change() {
        let sections = vec![make_phase("1", "First"), make_phase("2", "Second")];
        let new_order = vec!["1".to_string(), "2".to_string()];

        let result = reorder_phase_sections(sections, &new_order);
        assert_eq!(section_labels(&result), vec!["Phase 1", "Phase 2"]);
    }

    #[test]
    fn test_reorder_phases_preserves_leading_freeform() {
        // [FF:Overview, Phase 1, Phase 2] -> reorder phases to [2, 1]
        // Expected: [FF:Overview, Phase 2, Phase 1]
        let sections = vec![
            make_freeform("Overview"),
            make_phase("1", "First"),
            make_phase("2", "Second"),
        ];
        let new_order = vec!["2".to_string(), "1".to_string()];

        let result = reorder_phase_sections(sections, &new_order);
        assert_eq!(
            section_labels(&result),
            vec!["FF:Overview", "Phase 2", "Phase 1"]
        );
    }

    #[test]
    fn test_reorder_phases_preserves_trailing_freeform() {
        // [Phase 1, Phase 2, FF:Conclusion] -> reorder phases to [2, 1]
        // Expected: [Phase 2, Phase 1, FF:Conclusion]
        let sections = vec![
            make_phase("1", "First"),
            make_phase("2", "Second"),
            make_freeform("Conclusion"),
        ];
        let new_order = vec!["2".to_string(), "1".to_string()];

        let result = reorder_phase_sections(sections, &new_order);
        assert_eq!(
            section_labels(&result),
            vec!["Phase 2", "Phase 1", "FF:Conclusion"]
        );
    }

    #[test]
    fn test_reorder_phases_preserves_interleaved_freeform() {
        // [FF:Overview, Phase 1, FF:TechDetails, Phase 2, FF:Conclusion]
        // Reorder phases to [2, 1]
        // Expected: [FF:Overview, Phase 2, FF:TechDetails, Phase 1, FF:Conclusion]
        let sections = vec![
            make_freeform("Overview"),
            make_phase("1", "First"),
            make_freeform("TechDetails"),
            make_phase("2", "Second"),
            make_freeform("Conclusion"),
        ];
        let new_order = vec!["2".to_string(), "1".to_string()];

        let result = reorder_phase_sections(sections, &new_order);
        assert_eq!(
            section_labels(&result),
            vec![
                "FF:Overview",
                "Phase 2",
                "FF:TechDetails",
                "Phase 1",
                "FF:Conclusion"
            ]
        );
    }

    #[test]
    fn test_reorder_phases_multiple_interleaved_freeform() {
        // [FF:A, FF:B, Phase 1, FF:C, Phase 2, Phase 3, FF:D]
        // Reorder phases to [3, 1, 2]
        // Expected: [FF:A, FF:B, Phase 3, FF:C, Phase 1, Phase 2, FF:D]
        let sections = vec![
            make_freeform("A"),
            make_freeform("B"),
            make_phase("1", "First"),
            make_freeform("C"),
            make_phase("2", "Second"),
            make_phase("3", "Third"),
            make_freeform("D"),
        ];
        let new_order = vec!["3".to_string(), "1".to_string(), "2".to_string()];

        let result = reorder_phase_sections(sections, &new_order);
        assert_eq!(
            section_labels(&result),
            vec![
                "FF:A", "FF:B", "Phase 3", "FF:C", "Phase 1", "Phase 2", "FF:D"
            ]
        );
    }

    #[test]
    fn test_reorder_phases_only_phases() {
        // [Phase 1, Phase 2, Phase 3] -> [3, 2, 1]
        let sections = vec![
            make_phase("1", "First"),
            make_phase("2", "Second"),
            make_phase("3", "Third"),
        ];
        let new_order = vec!["3".to_string(), "2".to_string(), "1".to_string()];

        let result = reorder_phase_sections(sections, &new_order);
        assert_eq!(
            section_labels(&result),
            vec!["Phase 3", "Phase 2", "Phase 1"]
        );
    }

    #[test]
    fn test_reorder_phases_only_freeform_no_phases() {
        // Edge case: no phases at all
        let sections = vec![make_freeform("A"), make_freeform("B")];
        let new_order: Vec<String> = vec![];

        let result = reorder_phase_sections(sections, &new_order);
        assert_eq!(section_labels(&result), vec!["FF:A", "FF:B"]);
    }

    #[test]
    fn test_reorder_phases_single_phase() {
        let sections = vec![
            make_freeform("Overview"),
            make_phase("1", "Only"),
            make_freeform("Conclusion"),
        ];
        let new_order = vec!["1".to_string()];

        let result = reorder_phase_sections(sections, &new_order);
        assert_eq!(
            section_labels(&result),
            vec!["FF:Overview", "Phase 1", "FF:Conclusion"]
        );
    }

    #[test]
    fn test_reorder_phases_consecutive_freeform_between_phases() {
        // [Phase 1, FF:A, FF:B, FF:C, Phase 2] -> swap phases
        // Expected: [Phase 2, FF:A, FF:B, FF:C, Phase 1]
        let sections = vec![
            make_phase("1", "First"),
            make_freeform("A"),
            make_freeform("B"),
            make_freeform("C"),
            make_phase("2", "Second"),
        ];
        let new_order = vec!["2".to_string(), "1".to_string()];

        let result = reorder_phase_sections(sections, &new_order);
        assert_eq!(
            section_labels(&result),
            vec!["Phase 2", "FF:A", "FF:B", "FF:C", "Phase 1"]
        );
    }

    #[test]
    fn test_reorder_preserves_total_section_count() {
        let sections = vec![
            make_freeform("Overview"),
            make_phase("1", "First"),
            make_freeform("Schema"),
            make_phase("2", "Second"),
            make_freeform("Benchmarks"),
            make_phase("3", "Third"),
            make_freeform("Questions"),
        ];
        let original_len = sections.len();
        let new_order = vec!["3".to_string(), "1".to_string(), "2".to_string()];

        let result = reorder_phase_sections(sections, &new_order);
        assert_eq!(result.len(), original_len);
    }

    #[test]
    fn test_reorder_is_deterministic() {
        // Running the same reorder twice should produce identical results
        let make_sections = || {
            vec![
                make_freeform("Overview"),
                make_phase("1", "First"),
                make_freeform("TechDetails"),
                make_phase("2", "Second"),
                make_freeform("Conclusion"),
            ]
        };
        let new_order = vec!["2".to_string(), "1".to_string()];

        let result1 = reorder_phase_sections(make_sections(), &new_order);
        let result2 = reorder_phase_sections(make_sections(), &new_order);

        assert_eq!(section_labels(&result1), section_labels(&result2));
    }

    #[test]
    fn test_reorder_identity_preserves_exact_order() {
        // Reordering phases to the same order should produce the original layout
        let sections = vec![
            make_freeform("Overview"),
            make_phase("1", "First"),
            make_freeform("TechDetails"),
            make_phase("2", "Second"),
            make_freeform("Conclusion"),
        ];
        let new_order = vec!["1".to_string(), "2".to_string()];

        let result = reorder_phase_sections(sections, &new_order);
        assert_eq!(
            section_labels(&result),
            vec![
                "FF:Overview",
                "Phase 1",
                "FF:TechDetails",
                "Phase 2",
                "FF:Conclusion"
            ]
        );
    }

    #[test]
    fn test_reorder_case_insensitive_phase_matching() {
        let sections = vec![make_phase("1a", "Sub A"), make_phase("2B", "Sub B")];
        // Matching should be case-insensitive
        let new_order = vec!["2b".to_string(), "1A".to_string()];

        let result = reorder_phase_sections(sections, &new_order);
        assert_eq!(section_labels(&result), vec!["Phase 2B", "Phase 1a"]);
    }

    // ==================== parse_and_validate tests ====================

    #[test]
    fn test_parse_and_validate_ticket_order_valid() {
        let order = "1. j-a1b2\n2. j-c3d4\n3. j-e5f6\n";
        let original = vec![
            "j-e5f6".to_string(),
            "j-a1b2".to_string(),
            "j-c3d4".to_string(),
        ];
        let result = parse_and_validate_ticket_order(order, &original).unwrap();
        assert_eq!(result, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);
    }

    #[test]
    fn test_parse_and_validate_ticket_order_mismatch() {
        let order = "1. j-a1b2\n2. j-new\n";
        let original = vec!["j-a1b2".to_string(), "j-c3d4".to_string()];
        let result = parse_and_validate_ticket_order(order, &original);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_and_validate_phase_order_valid() {
        let order = "2: Implementation\n1: Infrastructure\n";
        let original = vec![
            ("1".to_string(), "Infrastructure".to_string()),
            ("2".to_string(), "Implementation".to_string()),
        ];
        let result = parse_and_validate_phase_order(order, &original).unwrap();
        assert_eq!(result, vec!["2", "1"]);
    }

    #[test]
    fn test_parse_and_validate_phase_order_mismatch() {
        let order = "3: New Phase\n";
        let original = vec![("1".to_string(), "Infrastructure".to_string())];
        let result = parse_and_validate_phase_order(order, &original);
        assert!(result.is_err());
    }
}
