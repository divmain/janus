use serde_json::json;
use std::fs;

use super::CommandOutput;
use crate::error::Result;
use crate::events::log_cache_rebuilt;
use crate::store::get_or_init_store;

use crate::embedding::model::EMBEDDING_MODEL_NAME;

pub async fn cmd_cache_status(output_json: bool) -> Result<()> {
    let store = get_or_init_store().await?;

    let (with_embedding, total) = store.embedding_coverage();
    let percentage = if total > 0 {
        (with_embedding as f64 / total as f64 * 100.0) as u32
    } else {
        0
    };

    let emb_dir = crate::types::janus_root().join("embeddings");
    let emb_dir_size = if emb_dir.exists() {
        dir_size(&emb_dir)
    } else {
        0
    };

    let text = format!(
        "Cache status:\n  Tickets loaded: {total}\n  Embedding Coverage: {with_embedding}/{total} ({percentage}%)\n  Embedding Model: {EMBEDDING_MODEL_NAME}\n  Embeddings Directory: {}\n  Embeddings Directory Size: {} bytes",
        crate::utils::format_relative_path(&emb_dir),
        emb_dir_size,
    );

    let output = json!({
        "ticket_count": total,
        "status": "healthy",
        "embedding_coverage": {
            "with_embedding": with_embedding,
            "total": total,
            "percentage": percentage,
        },
        "embedding_model": EMBEDDING_MODEL_NAME,
        "embeddings_directory": emb_dir.to_string_lossy(),
        "embeddings_directory_size_bytes": emb_dir_size,
    });

    CommandOutput::new(output)
        .with_text(text)
        .print(output_json)?;

    Ok(())
}

/// Prune orphaned embedding files that no longer correspond to current tickets.
///
/// # Concurrency Warning
///
/// This command is subject to a TOCTOU race: valid embedding keys are computed from
/// current ticket file mtimes, and then orphaned files are deleted. If a ticket is
/// modified between these two steps (e.g., by another process or a concurrent
/// `janus cache rebuild`), a freshly-generated embedding could be incorrectly pruned.
/// Do not run this command concurrently with `janus cache rebuild` or other operations
/// that modify ticket files.
pub async fn cmd_cache_prune(output_json: bool) -> Result<()> {
    // 1. Get the store and compute valid embedding keys for all current ticket files
    let store = get_or_init_store().await?;
    let tickets = store.get_all_tickets();

    let mut valid_keys = std::collections::HashSet::new();
    for ticket in &tickets {
        let file_path = match &ticket.file_path {
            Some(fp) => fp,
            None => continue,
        };
        // Get mtime
        let mtime_ns = match fs::metadata(file_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as i64)
        {
            Some(ns) => ns,
            None => continue,
        };
        let key = crate::store::TicketStore::embedding_key(file_path, mtime_ns);
        valid_keys.insert(key);
    }

    // 2. Calculate bytes that will be freed (before pruning)
    let emb_dir = crate::types::janus_root().join("embeddings");
    let bytes_before = if emb_dir.exists() {
        dir_size(&emb_dir)
    } else {
        0
    };

    // 3. Prune orphaned embedding files
    let pruned_count = match crate::store::TicketStore::prune_orphaned(&valid_keys) {
        Ok(count) => count,
        Err(e) => {
            return Err(crate::error::JanusError::Io(e));
        }
    };

    let bytes_after = if emb_dir.exists() {
        dir_size(&emb_dir)
    } else {
        0
    };
    let bytes_freed = bytes_before.saturating_sub(bytes_after);

    // 4. Output results
    let text = if pruned_count == 0 {
        "No orphaned embedding files found.".to_string()
    } else {
        format!(
            "Pruned {pruned_count} orphaned embedding file(s), freeing {bytes_freed} bytes.\n\
             Note: If tickets were modified during pruning, some valid embeddings may have been removed. \
             Run 'janus cache rebuild' to regenerate if needed."
        )
    };

    CommandOutput::new(json!({
        "action": "cache_prune",
        "pruned_count": pruned_count,
        "bytes_freed": bytes_freed,
        "valid_keys_count": valid_keys.len(),
        "success": true,
    }))
    .with_text(text)
    .print(output_json)
}

pub async fn cmd_cache_rebuild(output_json: bool) -> Result<()> {
    if !output_json {
        println!("Regenerating embeddings for all tickets...");
    }

    let start_total = std::time::Instant::now();

    // Delete existing embeddings directory
    let emb_dir = crate::types::janus_root().join("embeddings");
    if emb_dir.exists() {
        if !output_json {
            println!(
                "Deleting existing embeddings at: {}",
                crate::utils::format_relative_path(&emb_dir)
            );
        }
        if let Err(e) = fs::remove_dir_all(&emb_dir) {
            if !output_json {
                println!("Warning: failed to delete embeddings directory: {e}");
            }
        }
    }

    // Re-embed all tickets
    let store = get_or_init_store().await?;
    let tickets = store.get_all_tickets();
    let ticket_count = tickets.len();

    if !output_json {
        println!("Generating embeddings for {ticket_count} tickets...");
    }

    let mut embedded_count = 0_usize;
    let mut valid_keys = std::collections::HashSet::new();

    for ticket in &tickets {
        let file_path = match &ticket.file_path {
            Some(fp) => fp,
            None => continue,
        };

        let mtime_ns = match fs::metadata(file_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as i64)
        {
            Some(ns) => ns,
            None => continue,
        };

        // Build text for embedding
        let title = ticket.title.as_deref().unwrap_or("");
        let body = ticket.body.as_deref().unwrap_or("");
        let text = if body.is_empty() {
            title.to_string()
        } else {
            format!("{title}\n\n{body}")
        };

        // Generate embedding
        match crate::embedding::model::generate_embedding(&text).await {
            Ok(embedding) => {
                let key = crate::store::TicketStore::embedding_key(file_path, mtime_ns);
                if let Err(e) = crate::store::TicketStore::save_embedding(&key, &embedding) {
                    if !output_json {
                        eprintln!(
                            "Warning: failed to save embedding for {}: {e}",
                            ticket.id.as_deref().unwrap_or("unknown")
                        );
                    }
                } else {
                    valid_keys.insert(key);
                    embedded_count += 1;
                    if !output_json && embedded_count % 10 == 0 {
                        println!("  Progress: {embedded_count}/{ticket_count}");
                    }
                }
            }
            Err(e) => {
                if !output_json {
                    eprintln!(
                        "Warning: failed to generate embedding for {}: {e}",
                        ticket.id.as_deref().unwrap_or("unknown")
                    );
                }
            }
        }
    }

    // Prune orphaned embedding files
    if let Err(e) = crate::store::TicketStore::prune_orphaned(&valid_keys) {
        if !output_json {
            eprintln!("Warning: failed to prune orphaned embeddings: {e}");
        }
    }

    // Reload embeddings into the store
    store.load_embeddings();

    let total_duration = start_total.elapsed();

    let output = json!({
        "action": "cache_rebuilt",
        "ticket_count": ticket_count,
        "embedded_count": embedded_count,
        "total_time_ms": total_duration.as_millis(),
        "success": true,
        "embedding_model": EMBEDDING_MODEL_NAME,
    });

    CommandOutput::new(output)
        .with_text(format!(
            "Embeddings rebuilt successfully:\n  Tickets: {ticket_count}\n  Embeddings generated: {embedded_count}\n  Total time: {total_duration:?}"
        ))
        .print(output_json)?;

    // Log the cache rebuild event
    let details = json!({
        "embedded_count": embedded_count,
        "embedding_model": EMBEDDING_MODEL_NAME,
    });
    log_cache_rebuilt(
        "explicit_rebuild",
        "janus cache rebuild command",
        Some(total_duration.as_millis() as u64),
        Some(ticket_count),
        Some(details),
    );

    Ok(())
}

/// Calculate the total size of a directory in bytes.
///
/// Recursively traverses subdirectories for robustness, even though the
/// `.janus/embeddings/` directory is expected to be flat (only `.bin` files).
fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total += meta.len();
                } else if meta.is_dir() {
                    total += dir_size(&entry.path());
                }
            }
        }
    }
    total
}
