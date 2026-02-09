use std::collections::HashSet;

use serde_json::json;

use super::CommandOutput;
use crate::commands::dep_tree::{DepthCalculator, TreeBuilder, TreeFormatter};
use crate::error::{JanusError, Result};
use crate::events::{log_dependency_added, log_dependency_removed};
use crate::ticket::{Ticket, build_ticket_map, check_circular_dependency, resolve_id_from_map};

/// Add a dependency to a ticket
pub async fn cmd_dep_add(id: &str, dep_id: &str, output_json: bool) -> Result<()> {
    let ticket = Ticket::find(id).await?;

    // Validate that the dependency exists
    let dep_ticket = Ticket::find(dep_id).await?;

    // Check for self-dependency
    if ticket.id == dep_ticket.id {
        return Err(JanusError::SelfDependency);
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

    // Resolve the dependency ID to get the full ID
    let dep_ticket = Ticket::find(dep_id).await?;

    let removed = ticket.remove_from_array_field("deps", &dep_ticket.id)?;
    if !removed {
        return Err(JanusError::DependencyNotFound(dep_ticket.id.clone()));
    }

    // Log the event
    log_dependency_removed(&ticket.id, &dep_ticket.id);

    let metadata = ticket.read()?;
    CommandOutput::new(json!({
        "id": ticket.id,
        "action": "dep_removed",
        "dep_id": dep_ticket.id,
        "current_deps": metadata.deps,
    }))
    .with_text(format!(
        "Removed dependency: {} -/-> {}",
        ticket.id, dep_ticket.id
    ))
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
