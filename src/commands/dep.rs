use std::collections::{HashMap, HashSet};

use owo_colors::OwoColorize;

use crate::error::{JanusError, Result};
use crate::ticket::{Ticket, build_ticket_map};
use crate::types::TicketMetadata;

/// Add a dependency to a ticket
pub fn cmd_dep_add(id: &str, dep_id: &str) -> Result<()> {
    let ticket = Ticket::find(id)?;

    // Validate that the dependency exists
    Ticket::find(dep_id)?;

    let added = ticket.add_to_array_field("deps", dep_id)?;
    if added {
        println!("Added dependency: {} -> {}", ticket.id, dep_id);
    } else {
        println!("Dependency already exists");
    }

    Ok(())
}

/// Remove a dependency from a ticket
pub fn cmd_dep_remove(id: &str, dep_id: &str) -> Result<()> {
    let ticket = Ticket::find(id)?;

    let removed = ticket.remove_from_array_field("deps", dep_id)?;
    if removed {
        println!("Removed dependency: {} -/-> {}", ticket.id, dep_id);
    } else {
        return Err(JanusError::Other("Dependency not found".to_string()));
    }

    Ok(())
}

/// Display the dependency tree for a ticket
pub fn cmd_dep_tree(id: &str, full_mode: bool) -> Result<()> {
    let ticket_map = build_ticket_map();

    // Find the matching ticket ID
    let matching_ids: Vec<_> = ticket_map.keys().filter(|k| k.contains(id)).collect();

    if matching_ids.is_empty() {
        return Err(JanusError::TicketNotFound(id.to_string()));
    }
    if matching_ids.len() > 1 {
        return Err(JanusError::AmbiguousId(id.to_string()));
    }

    let root = matching_ids[0].clone();

    // Calculate the maximum depth at which each node appears
    let mut max_depth: HashMap<String, usize> = HashMap::new();
    let mut subtree_depth: HashMap<String, usize> = HashMap::new();

    fn find_max_depth(
        id: &str,
        current_depth: usize,
        path: &mut HashSet<String>,
        max_depth: &mut HashMap<String, usize>,
        ticket_map: &HashMap<String, TicketMetadata>,
    ) {
        if path.contains(id) {
            return;
        }

        let current_max = max_depth.get(id).copied().unwrap_or(0);
        max_depth.insert(id.to_string(), current_max.max(current_depth));

        if let Some(ticket) = ticket_map.get(id) {
            path.insert(id.to_string());
            for dep in &ticket.deps {
                find_max_depth(dep, current_depth + 1, path, max_depth, ticket_map);
            }
            path.remove(id);
        }
    }

    fn compute_subtree_depth(
        id: &str,
        max_depth: &HashMap<String, usize>,
        subtree_depth: &mut HashMap<String, usize>,
        ticket_map: &HashMap<String, TicketMetadata>,
    ) -> usize {
        let mut max = max_depth.get(id).copied().unwrap_or(0);
        if let Some(ticket) = ticket_map.get(id) {
            for dep in &ticket.deps {
                max = max.max(compute_subtree_depth(
                    dep,
                    max_depth,
                    subtree_depth,
                    ticket_map,
                ));
            }
        }
        subtree_depth.insert(id.to_string(), max);
        max
    }

    let mut path = HashSet::new();
    find_max_depth(&root, 0, &mut path, &mut max_depth, &ticket_map);
    compute_subtree_depth(&root, &max_depth, &mut subtree_depth, &ticket_map);

    // Print the tree
    fn get_printable_children(
        id: &str,
        depth: usize,
        full_mode: bool,
        max_depth: &HashMap<String, usize>,
        subtree_depth: &HashMap<String, usize>,
        ticket_map: &HashMap<String, TicketMetadata>,
    ) -> Vec<String> {
        let deps = ticket_map
            .get(id)
            .map(|t| &t.deps)
            .cloned()
            .unwrap_or_default();

        let mut children: Vec<String> = deps
            .into_iter()
            .filter(|dep| {
                if !max_depth.contains_key(dep) {
                    return false;
                }
                full_mode || depth + 1 == max_depth.get(dep).copied().unwrap_or(0)
            })
            .collect();

        children.sort_by(|a, b| {
            let depth_diff = subtree_depth
                .get(b)
                .copied()
                .unwrap_or(0)
                .cmp(&subtree_depth.get(a).copied().unwrap_or(0));
            if depth_diff != std::cmp::Ordering::Equal {
                depth_diff
            } else {
                a.cmp(b)
            }
        });

        children
    }

    fn print_tree(
        id: &str,
        depth: usize,
        prefix: &str,
        full_mode: bool,
        max_depth: &HashMap<String, usize>,
        subtree_depth: &HashMap<String, usize>,
        ticket_map: &HashMap<String, TicketMetadata>,
    ) {
        let children =
            get_printable_children(id, depth, full_mode, max_depth, subtree_depth, ticket_map);

        for (i, child) in children.iter().enumerate() {
            let is_last = i == children.len() - 1;
            let connector = if is_last { "└── " } else { "├── " };
            let child_prefix = if is_last { "    " } else { "│   " };

            let ticket = ticket_map.get(child);
            let status = ticket
                .and_then(|t| t.status)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "?".to_string());
            let title = ticket
                .and_then(|t| t.title.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("");

            println!(
                "{}{}{} [{}] {}",
                prefix.dimmed(),
                connector.dimmed(),
                child.cyan(),
                status,
                title
            );

            print_tree(
                child,
                depth + 1,
                &format!("{}{}", prefix, child_prefix),
                full_mode,
                max_depth,
                subtree_depth,
                ticket_map,
            );
        }
    }

    // Print root
    let root_ticket = ticket_map.get(&root);
    let root_status = root_ticket
        .and_then(|t| t.status)
        .map(|s| s.to_string())
        .unwrap_or_else(|| "?".to_string());
    let root_title = root_ticket
        .and_then(|t| t.title.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("");

    println!("{} [{}] {}", root.cyan(), root_status, root_title);
    print_tree(
        &root,
        0,
        "",
        full_mode,
        &max_depth,
        &subtree_depth,
        &ticket_map,
    );

    Ok(())
}
