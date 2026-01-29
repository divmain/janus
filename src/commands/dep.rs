use std::collections::{HashMap, HashSet};

use serde_json::json;

use super::CommandOutput;
use crate::commands::dep_tree::{DepthCalculator, TreeBuilder, TreeFormatter};
use crate::error::{JanusError, Result};
use crate::events::{log_dependency_added, log_dependency_removed};
use crate::ticket::{Ticket, build_ticket_map, resolve_id_from_map};
use crate::types::TicketMetadata;

/// Check if adding a dependency would create a circular dependency.
///
/// This function performs both direct and transitive circular dependency detection:
/// - Direct: A->B when B already depends on A
/// - Transitive: A->B->C->A (multi-level cycles)
///
/// Returns an error describing the cycle if one is detected.
fn check_circular_dependency(
    from_id: &str,
    to_id: &str,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> Result<()> {
    // Direct circular dependency: A->B when B already depends on A
    if let Some(dep_ticket) = ticket_map.get(to_id)
        && dep_ticket.deps.contains(&from_id.to_string())
    {
        return Err(JanusError::CircularDependency(format!(
            "{} -> {} (direct: {} already depends on {})",
            from_id, to_id, to_id, from_id
        )));
    }

    // Transitive circular dependency: A->B->...->A
    // Use DFS to detect if we can reach from_id starting from to_id
    fn has_path_to(
        current: &str,
        target: &str,
        ticket_map: &HashMap<String, TicketMetadata>,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        if current == target {
            path.push(current.to_string());
            return Some(path.clone());
        }

        if visited.contains(current) {
            return None;
        }

        visited.insert(current.to_string());
        path.push(current.to_string());

        if let Some(ticket) = ticket_map.get(current) {
            for dep in &ticket.deps {
                if let Some(found_path) = has_path_to(dep, target, ticket_map, visited, path) {
                    return Some(found_path);
                }
            }
        }

        path.pop();
        None
    }

    let mut visited = HashSet::new();
    let mut path = Vec::new();

    if let Some(cycle_path) = has_path_to(to_id, from_id, ticket_map, &mut visited, &mut path) {
        // Format the cycle path for the error message
        let cycle_str = cycle_path.join(" -> ");
        return Err(JanusError::CircularDependency(format!(
            "{} -> {} would create cycle: {}",
            from_id, to_id, cycle_str
        )));
    }

    Ok(())
}

/// Add a dependency to a ticket
pub async fn cmd_dep_add(id: &str, dep_id: &str, output_json: bool) -> Result<()> {
    let ticket = Ticket::find(id).await?;

    // Validate that the dependency exists
    let dep_ticket = Ticket::find(dep_id).await?;

    // Check for self-dependency
    if ticket.id == dep_ticket.id {
        return Err(JanusError::Other(
            "A ticket cannot depend on itself.".to_string(),
        ));
    }

    // Check for circular dependencies before adding
    let ticket_map = build_ticket_map().await?;
    check_circular_dependency(&ticket.id, &dep_ticket.id, &ticket_map)?;

    let added = ticket.add_to_array_field("deps", &dep_ticket.id)?;
    let metadata = ticket.read()?;

    // Log the event if dependency was actually added
    if added {
        log_dependency_added(&ticket.id, &dep_ticket.id);
    }

    let text = if added {
        format!("Added dependency: {} -> {}", ticket.id, dep_ticket.id)
    } else {
        "Dependency already exists".to_string()
    };

    CommandOutput::new(json!({
        "id": ticket.id,
        "action": if added { "dep_added" } else { "dep_already_exists" },
        "dep_id": dep_ticket.id,
        "current_deps": metadata.deps,
    }))
    .with_text(text)
    .print(output_json)
}

/// Remove a dependency from a ticket
pub async fn cmd_dep_remove(id: &str, dep_id: &str, output_json: bool) -> Result<()> {
    let ticket = Ticket::find(id).await?;

    let removed = ticket.remove_from_array_field("deps", dep_id)?;
    if !removed {
        return Err(JanusError::DependencyNotFound(dep_id.to_string()));
    }

    // Log the event
    log_dependency_removed(&ticket.id, dep_id);

    let metadata = ticket.read()?;
    CommandOutput::new(json!({
        "id": ticket.id,
        "action": "dep_removed",
        "dep_id": dep_id,
        "current_deps": metadata.deps,
    }))
    .with_text(format!("Removed dependency: {} -/-> {}", ticket.id, dep_id))
    .print(output_json)
}

/// Display the dependency tree for a ticket
pub async fn cmd_dep_tree(id: &str, full_mode: bool, output_json: bool) -> Result<()> {
    let ticket_map = build_ticket_map().await?;

    let root = resolve_id_from_map(id, &ticket_map)?;

    let mut json_path = HashSet::new();
    let tree = TreeBuilder::build_json_tree(
        &root,
        &mut json_path,
        &ticket_map,
        &super::ticket_minimal_json_with_exists,
    );
    let json_output = json!({ "root": tree });

    if output_json {
        return CommandOutput::new(json_output).print(output_json);
    }

    let (max_depth, subtree_depth) = DepthCalculator::calculate_depths(&root, &ticket_map);

    let formatter = TreeFormatter::new(&ticket_map, &max_depth, &subtree_depth);
    formatter.print_root(&root);
    formatter.print_tree(&root, 0, "", full_mode);

    Ok(())
}
