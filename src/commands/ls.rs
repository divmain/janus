use std::fs;
use std::path::PathBuf;

use crate::commands::{FormatOptions, format_deps, format_ticket_line, sort_by_priority};
use crate::error::Result;
use crate::parser::parse_ticket_content;
use crate::ticket::{build_ticket_map, get_all_tickets, get_file_mtime};
use crate::types::{TICKETS_DIR, TicketMetadata, TicketStatus};

/// List all tickets, optionally filtered by status
pub fn cmd_ls(status_filter: Option<&str>) -> Result<()> {
    let tickets = get_all_tickets();

    for t in &tickets {
        // Filter by status if provided
        if let Some(filter) = status_filter {
            let ticket_status = t.status.map(|s| s.to_string()).unwrap_or_default();
            if ticket_status != filter {
                continue;
            }
        }

        let opts = FormatOptions {
            suffix: Some(format_deps(&t.deps)),
            ..Default::default()
        };
        println!("{}", format_ticket_line(t, opts));
    }

    Ok(())
}

/// List tickets that are ready to work on (new or next status, all deps complete)
pub fn cmd_ready() -> Result<()> {
    let ticket_map = build_ticket_map();

    let mut ready: Vec<TicketMetadata> = ticket_map
        .values()
        .filter(|t| {
            // Must be "new" or "next" status
            if !matches!(t.status, Some(TicketStatus::New) | Some(TicketStatus::Next)) {
                return false;
            }

            // All deps must be complete
            t.deps.iter().all(|dep_id| {
                ticket_map
                    .get(dep_id)
                    .map(|dep| dep.status == Some(TicketStatus::Complete))
                    .unwrap_or(false)
            })
        })
        .cloned()
        .collect();

    sort_by_priority(&mut ready);

    for t in &ready {
        let opts = FormatOptions {
            show_priority: true,
            ..Default::default()
        };
        println!("{}", format_ticket_line(t, opts));
    }

    Ok(())
}

/// List tickets that are blocked (have incomplete deps)
pub fn cmd_blocked() -> Result<()> {
    let ticket_map = build_ticket_map();

    let mut blocked: Vec<(TicketMetadata, Vec<String>)> = Vec::new();

    for t in ticket_map.values() {
        // Must be "new" or "next" status
        if !matches!(t.status, Some(TicketStatus::New) | Some(TicketStatus::Next)) {
            continue;
        }

        // Must have deps
        if t.deps.is_empty() {
            continue;
        }

        // Find open blockers
        let open_blockers: Vec<String> = t
            .deps
            .iter()
            .filter(|dep_id| {
                ticket_map
                    .get(*dep_id)
                    .map(|dep| dep.status != Some(TicketStatus::Complete))
                    .unwrap_or(true) // Missing dep counts as open blocker
            })
            .cloned()
            .collect();

        if !open_blockers.is_empty() {
            blocked.push((t.clone(), open_blockers));
        }
    }

    // Sort by priority
    blocked.sort_by(|(a, _), (b, _)| {
        let pa = a.priority_num();
        let pb = b.priority_num();
        if pa != pb {
            pa.cmp(&pb)
        } else {
            a.id.cmp(&b.id)
        }
    });

    for (t, blockers) in &blocked {
        let opts = FormatOptions {
            show_priority: true,
            suffix: Some(format_deps(blockers)),
        };
        println!("{}", format_ticket_line(t, opts));
    }

    Ok(())
}

/// List recently closed tickets
pub fn cmd_closed(limit: usize) -> Result<()> {
    let files: Vec<String> = fs::read_dir(TICKETS_DIR)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if name.ends_with(".md") {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    // Get files with their modification times
    let mut file_stats: Vec<(String, PathBuf, std::time::SystemTime)> = files
        .into_iter()
        .filter_map(|file| {
            let path = PathBuf::from(TICKETS_DIR).join(&file);
            get_file_mtime(&path).map(|mtime| (file, path, mtime))
        })
        .collect();

    // Sort by modification time (newest first)
    file_stats.sort_by(|a, b| b.2.cmp(&a.2));

    let mut closed_tickets: Vec<TicketMetadata> = Vec::new();

    // Look through files (up to limit * 2 to account for non-closed tickets)
    for (file, path, _) in file_stats.iter().take(limit * 2) {
        if closed_tickets.len() >= limit {
            break;
        }

        if let Ok(content) = fs::read_to_string(path)
            && let Ok(mut metadata) = parse_ticket_content(&content)
            && metadata.status == Some(TicketStatus::Complete)
        {
            if metadata.id.is_none() {
                metadata.id = Some(file.strip_suffix(".md").unwrap_or(file).to_string());
            }
            metadata.file_path = Some(path.clone());
            closed_tickets.push(metadata);
        }
    }

    for t in &closed_tickets {
        println!("{}", format_ticket_line(t, FormatOptions::default()));
    }

    Ok(())
}
