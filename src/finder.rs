//! Simple find-by-partial-ID functions for tickets and plans.
//!
//! This module provides two async functions for finding entities (tickets, plans)
//! by partial ID. The store is treated as authoritative when available:
//! 1. If the store initializes successfully:
//!    a. Check for exact match on disk (file exists)
//!    b. Check store for partial matches
//!    c. Return NotFound/Ambiguous errors (no filesystem fallback)
//! 2. If the store fails to initialize, fall back to filesystem-based search

use std::path::{Path, PathBuf};

use crate::error::{JanusError, Result};
use crate::store::get_or_init_store;
use crate::types::{plans_dir, tickets_items_dir};
use crate::utils::DirScanner;

/// Validate that an ID is safe for filesystem use (no path traversal)
fn validate_id(id: &str) -> Result<()> {
    // Check for path separators and parent directory references
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(JanusError::InvalidPlanId(id.to_string()));
    }

    // Ensure ID contains only alphanumeric characters, hyphens, and underscores
    if !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(JanusError::InvalidPlanId(id.to_string()));
    }

    Ok(())
}

/// Find a ticket by partial ID.
///
/// Searches for a ticket file matching the given partial ID in the tickets directory.
/// Returns the full path to the ticket file if found, or an error if not found
/// or if multiple tickets match (ambiguous).
pub async fn find_ticket_by_id(partial_id: &str) -> Result<PathBuf> {
    let dir = tickets_items_dir();

    // Validate ID before any path construction
    validate_id(partial_id)?;

    // Use store as authoritative source when available; filesystem fallback only when store fails
    match get_or_init_store().await {
        Ok(store) => {
            // Exact match check - does file exist on disk?
            let exact_match_path = dir.join(format!("{partial_id}.md"));
            if exact_match_path.exists() {
                return Ok(exact_match_path);
            }

            // Partial match via store (store is authoritative, no filesystem fallback)
            let matches = store.find_by_partial_id(partial_id);
            match matches.len() {
                0 => Err(JanusError::TicketNotFound(partial_id.to_string())),
                1 => Ok(dir.join(format!("{}.md", &matches[0]))),
                _ => Err(JanusError::AmbiguousId(partial_id.to_string(), matches)),
            }
        }
        Err(_) => {
            // FALLBACK: File-based implementation only when store is unavailable
            find_ticket_by_id_filesystem(partial_id, &dir)
        }
    }
}

/// Find a plan by partial ID.
///
/// Searches for a plan file matching the given partial ID in the plans directory.
/// Returns the full path to the plan file if found, or an error if not found
/// or if multiple plans match (ambiguous).
pub async fn find_plan_by_id(partial_id: &str) -> Result<PathBuf> {
    let dir = plans_dir();

    // Validate ID before any path construction
    validate_id(partial_id)?;

    // Use store as authoritative source when available; filesystem fallback only when store fails
    match get_or_init_store().await {
        Ok(store) => {
            // Exact match check - does file exist on disk?
            let exact_match_path = dir.join(format!("{partial_id}.md"));
            if exact_match_path.exists() {
                return Ok(exact_match_path);
            }

            // Partial match via store (store is authoritative, no filesystem fallback)
            let matches = store.find_plan_by_partial_id(partial_id);
            match matches.len() {
                0 => Err(JanusError::PlanNotFound(partial_id.to_string())),
                1 => Ok(dir.join(format!("{}.md", &matches[0]))),
                _ => Err(JanusError::AmbiguousPlanId(partial_id.to_string(), matches)),
            }
        }
        Err(_) => {
            // FALLBACK: File-based implementation only when store is unavailable
            find_plan_by_id_filesystem(partial_id, &dir)
        }
    }
}

/// Filesystem-based find implementation for tickets (fallback when store unavailable).
fn find_ticket_by_id_filesystem(partial_id: &str, dir: &Path) -> Result<PathBuf> {
    let files = DirScanner::find_markdown_files_from_path(dir).unwrap_or_else(|e| {
        eprintln!("Warning: failed to read {} directory: {}", dir.display(), e);
        Vec::new()
    });

    // Check for exact match first
    let exact_name = format!("{partial_id}.md");
    if files.iter().any(|f| f == &exact_name) {
        return Ok(dir.join(&exact_name));
    }

    // Then check for partial matches
    let matches: Vec<_> = files.iter().filter(|f| f.contains(partial_id)).collect();

    match matches.len() {
        0 => Err(JanusError::TicketNotFound(partial_id.to_string())),
        1 => Ok(dir.join(matches[0])),
        _ => Err(JanusError::AmbiguousId(
            partial_id.to_string(),
            matches.iter().map(|m| m.replace(".md", "")).collect(),
        )),
    }
}

/// Filesystem-based find implementation for plans (fallback when store unavailable).
fn find_plan_by_id_filesystem(partial_id: &str, dir: &Path) -> Result<PathBuf> {
    let files = DirScanner::find_markdown_files_from_path(dir).unwrap_or_else(|e| {
        eprintln!("Warning: failed to read {} directory: {}", dir.display(), e);
        Vec::new()
    });

    // Check for exact match first
    let exact_name = format!("{partial_id}.md");
    if files.iter().any(|f| f == &exact_name) {
        return Ok(dir.join(&exact_name));
    }

    // Then check for partial matches
    let matches: Vec<_> = files.iter().filter(|f| f.contains(partial_id)).collect();

    match matches.len() {
        0 => Err(JanusError::PlanNotFound(partial_id.to_string())),
        1 => Ok(dir.join(matches[0])),
        _ => Err(JanusError::AmbiguousPlanId(
            partial_id.to_string(),
            matches.iter().map(|m| m.replace(".md", "")).collect(),
        )),
    }
}
