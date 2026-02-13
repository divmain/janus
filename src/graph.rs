//! Graph algorithms for dependency resolution and circular dependency detection.
//!
//! This module contains pure graph algorithms that operate on HashMaps of
//! ticket metadata, separated from ticket file I/O concerns.

use std::collections::{HashMap, HashSet};

use crate::error::{JanusError, Result};
use crate::types::{TicketId, TicketMetadata};

/// Resolve a partial ID to a full ID using an in-memory HashMap
///
/// This function works on an in-memory HashMap (useful for pre-loaded data),
/// while `find_by_partial_id()` in locator.rs works on the filesystem/store.
///
/// # Arguments
///
/// * `partial_id` - The partial ID to resolve (e.g., "j-a1")
/// * `map` - A HashMap of ticket IDs to tickets
///
/// # Returns
///
/// Returns the full ID if found uniquely, otherwise an error:
/// - `EmptyTicketMap` with "No tickets loaded" if map is empty
/// - `TicketNotFound` if no matches
/// - `AmbiguousTicketId` if multiple matches
pub fn resolve_id_from_map<T>(
    partial_id: &str,
    map: &std::collections::HashMap<String, T>,
) -> Result<String> {
    if map.is_empty() {
        return Err(JanusError::EmptyTicketMap);
    }

    if map.contains_key(partial_id) {
        return Ok(partial_id.to_string());
    }

    let matches: Vec<_> = map
        .keys()
        .filter(|k| k.contains(partial_id))
        .cloned()
        .collect();

    match matches.len() {
        0 => Err(JanusError::TicketNotFound(TicketId::new_unchecked(
            partial_id,
        ))),
        1 => Ok(matches[0].clone()),
        _ => Err(JanusError::AmbiguousTicketId(
            partial_id.to_string(),
            matches,
        )),
    }
}

/// Check if adding a dependency would create a circular dependency.
///
/// This function performs both direct and transitive circular dependency detection:
/// - Direct: A->B when B already depends on A
/// - Transitive: A->B->C->A (multi-level cycles)
///
/// Returns an error describing the cycle if one is detected.
pub fn check_circular_dependency(
    from_id: &str,
    to_id: &str,
    ticket_map: &HashMap<String, TicketMetadata>,
) -> Result<()> {
    // Direct circular dependency: A->B when B already depends on A
    let from_ticket_id = crate::types::TicketId::new_unchecked(from_id);
    if let Some(dep_ticket) = ticket_map.get(to_id)
        && dep_ticket.deps.contains(&from_ticket_id)
    {
        return Err(JanusError::CircularDependency(format!(
            "{from_id} -> {to_id} (direct: {to_id} already depends on {from_id})"
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
                if let Some(found_path) =
                    has_path_to(dep.as_ref(), target, ticket_map, visited, path)
                {
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
            "{from_id} -> {to_id} would create cycle: {cycle_str}"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_exact_match() {
        let mut map: std::collections::HashMap<String, ()> = std::collections::HashMap::new();
        map.insert("j-a1b2".to_string(), ());

        let result = resolve_id_from_map("j-a1b2", &map).unwrap();
        assert_eq!(result, "j-a1b2");
    }

    #[test]
    fn test_resolve_partial_match_single() {
        let mut map: std::collections::HashMap<String, ()> = std::collections::HashMap::new();
        map.insert("j-a1b2".to_string(), ());
        map.insert("k-c3d4".to_string(), ());

        let result = resolve_id_from_map("j-a1", &map).unwrap();
        assert_eq!(result, "j-a1b2");
    }

    #[test]
    fn test_resolve_partial_match_multiple() {
        let mut map: std::collections::HashMap<String, ()> = std::collections::HashMap::new();
        map.insert("j-a1b2".to_string(), ());
        map.insert("j-a1c3".to_string(), ());

        let result = resolve_id_from_map("j-a1", &map);
        assert!(matches!(result, Err(JanusError::AmbiguousTicketId(_, _))));
    }

    #[test]
    fn test_resolve_no_match() {
        let map: std::collections::HashMap<String, ()> = std::collections::HashMap::new();

        let result = resolve_id_from_map("x-y-z", &map);
        assert!(matches!(result, Err(JanusError::EmptyTicketMap)));
    }

    #[test]
    fn test_resolve_empty_map() {
        let map: std::collections::HashMap<String, ()> = std::collections::HashMap::new();

        let result = resolve_id_from_map("j-a1b2", &map);
        assert!(matches!(result, Err(JanusError::EmptyTicketMap)));
    }
}
