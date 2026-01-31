use serde_json::json;
use std::fs;

use super::CommandOutput;
use crate::cache::TicketCache;
use crate::error::{Result, is_corruption_error, is_permission_error};
use crate::events::log_cache_rebuilt;

#[cfg(feature = "semantic-search")]
use crate::embedding::model::EMBEDDING_MODEL_NAME;

pub async fn cmd_cache_status(output_json: bool) -> Result<()> {
    match TicketCache::open().await {
        Ok(cache) => {
            let db_path = cache.cache_db_path();
            let tickets = match cache.get_all_tickets().await {
                Ok(tickets) => tickets,
                Err(e) => {
                    eprintln!("Warning: failed to read tickets from cache: {}", e);
                    eprintln!("Falling back to empty ticket list for status display.");
                    Vec::new()
                }
            };

            let mut output = json!({
                "database_path": db_path.to_string_lossy(),
                "ticket_count": tickets.len(),
                "status": "healthy",
            });

            // Add embedding status for semantic-search feature
            #[cfg(feature = "semantic-search")]
            let embedding_info = match cache.embedding_coverage().await {
                Ok((with_embedding, total)) => {
                    let percentage = if total > 0 {
                        (with_embedding as f64 / total as f64 * 100.0) as u32
                    } else {
                        0
                    };

                    // Check model version
                    let model_version_ok = match cache.get_meta("embedding_model").await {
                        Ok(Some(stored_model)) => stored_model == EMBEDDING_MODEL_NAME,
                        _ => false,
                    };

                    Some((with_embedding, total, percentage, model_version_ok))
                }
                Err(e) => {
                    eprintln!("Warning: failed to get embedding coverage: {}", e);
                    None
                }
            };

            #[cfg(not(feature = "semantic-search"))]
            let _embedding_info: Option<(usize, usize, u32, bool)> = None;

            let text = if let Ok(meta) = fs::metadata(&db_path) {
                let size = meta.len();
                let modified_text = if let Ok(modified) = meta.modified() {
                    format!("  Last modified: {:?}", modified)
                } else {
                    String::new()
                };

                #[allow(unused_mut)]
                let mut text = format!(
                    "Cache status:\n  Database path: {}\n  Cached tickets: {}\n  Database size: {} bytes\n{}",
                    db_path.display(),
                    tickets.len(),
                    size,
                    modified_text
                );

                // Add embedding status to text output
                #[cfg(feature = "semantic-search")]
                if let Some((with_embedding, total, percentage, _model_version_ok)) = embedding_info
                {
                    text.push_str(&format!(
                        "\n\nEmbedding Coverage: {}/{} ({}%)\n",
                        with_embedding, total, percentage
                    ));
                    text.push_str(&format!("Embedding Model: {}\n", EMBEDDING_MODEL_NAME));

                    // Get stored model version from cache
                    let stored_model = cache.get_meta("embedding_model").await.ok().flatten();

                    match stored_model {
                        Some(model) if model == EMBEDDING_MODEL_NAME => {
                            text.push_str(&format!("Model Version: {} ✓", model));
                        }
                        Some(model) => {
                            text.push_str(&format!(
                                "Model Version: {} ⚠ (current: {})\n",
                                model, EMBEDDING_MODEL_NAME
                            ));
                            text.push_str("  Run 'janus cache rebuild' to update embeddings");
                        }
                        None => {
                            text.push_str("Model Version: Not tracked");
                        }
                    }
                }

                text
            } else {
                #[allow(unused_mut)]
                let mut text = format!(
                    "Cache status:\n  Database path: {}\n  Cached tickets: {}",
                    db_path.display(),
                    tickets.len()
                );

                // Add embedding status to text output
                #[cfg(feature = "semantic-search")]
                if let Some((with_embedding, total, percentage, model_version_ok)) = embedding_info
                {
                    text.push_str(&format!(
                        "\n\nEmbedding Coverage: {}/{} ({}%)\n",
                        with_embedding, total, percentage
                    ));
                    text.push_str(&format!("Embedding Model: {}\n", EMBEDDING_MODEL_NAME));

                    if model_version_ok {
                        text.push_str("Model Version: ✓");
                    } else {
                        text.push_str(&format!(
                            "Model Version: ⚠ (expected: {})\n",
                            EMBEDDING_MODEL_NAME
                        ));
                        text.push_str("  Run 'janus cache rebuild' to update embeddings");
                    }
                }

                text
            };

            if let Ok(meta) = fs::metadata(&db_path) {
                output["database_size_bytes"] = json!(meta.len());
                if let Ok(modified) = meta.modified() {
                    output["last_modified"] = json!(format!("{:?}", modified));
                }
            }

            // Add embedding info to JSON output
            #[cfg(feature = "semantic-search")]
            if let Some((with_embedding, total, percentage, model_version_ok)) = embedding_info {
                output["embedding_coverage"] = json!({
                    "with_embedding": with_embedding,
                    "total": total,
                    "percentage": percentage
                });
                output["embedding_model"] = json!(EMBEDDING_MODEL_NAME);
                output["model_version_ok"] = json!(model_version_ok);
            }

            CommandOutput::new(output)
                .with_text(text)
                .print(output_json)?;
        }
        Err(e) => {
            let status = if is_corruption_error(&e) {
                "corrupted"
            } else if is_permission_error(&e) {
                "permission_denied"
            } else {
                "not_available"
            };

            let text = if is_corruption_error(&e) {
                "Cache database is corrupted and cannot be opened.\n\nTo fix this issue:\n  1. Run 'janus cache clear' to delete the corrupted cache\n  2. Run any janus command to rebuild the cache automatically\n  3. Or run 'janus cache rebuild' to rebuild it manually".to_string()
            } else if is_permission_error(&e) {
                format!(
                    "Cache database cannot be accessed due to permission issues.\n\nTo fix this issue:\n  1. Check file permissions for:\n     {}\n  2. Ensure the cache directory is writable\n  3. Try 'janus cache rebuild' after fixing permissions",
                    crate::cache::cache_dir().display()
                )
            } else {
                format!(
                    "Cache not available: {}\nRun 'janus cache rebuild' to create a cache.",
                    e
                )
            };

            CommandOutput::new(json!({
                "status": status,
                "error": e.to_string(),
            }))
            .with_text(text)
            .print(output_json)?;

            return Err(e);
        }
    }
    Ok(())
}

pub async fn cmd_cache_clear(output_json: bool) -> Result<()> {
    let db_path = match TicketCache::open().await {
        Ok(cache) => cache.cache_db_path(),
        Err(e) => {
            if is_permission_error(&e) {
                return Err(e);
            }

            if db_path_from_current_dir().exists() {
                db_path_from_current_dir()
            } else {
                CommandOutput::new(json!({
                    "action": "cache_clear",
                    "success": true,
                    "message": "Cache does not exist or has already been cleared",
                }))
                .with_text("Cache does not exist or has already been cleared.\n\nThe cache will be created automatically on the next janus command.")
                .print(output_json)?;
                return Ok(());
            }
        }
    };

    if !output_json {
        println!("Deleting cache database: {}", db_path.display());
    }

    if let Err(e) = fs::remove_file(&db_path) {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            if !output_json {
                println!("Error: Permission denied when trying to delete cache.");
                println!("Please check file permissions for: {}", db_path.display());
            }
            return Err(e.into());
        }
        return Err(e.into());
    }

    CommandOutput::new(json!({
        "action": "cache_cleared",
        "database_path": db_path.to_string_lossy(),
        "success": true,
        "message": format!("Cache database deleted: {}", db_path.display()),
    }))
    .with_text("Cache cleared successfully.\n\nNote: The cache will be rebuilt automatically on the next janus command.")
    .print(output_json)
}

pub async fn cmd_cache_rebuild(output_json: bool) -> Result<()> {
    if !output_json {
        println!("Rebuilding cache...");
    }

    let db_path = db_path_from_current_dir();

    let start_total = std::time::Instant::now();

    if db_path.exists() {
        if !output_json {
            println!("Found existing cache at: {}", db_path.display());
        }
        if let Err(e) = fs::remove_file(&db_path) {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                if !output_json {
                    println!("Error: Permission denied when trying to delete existing cache.");
                    println!("Please check file permissions for: {}", db_path.display());
                }
                return Err(e.into());
            }
            if !output_json {
                println!("Warning: failed to delete existing cache: {}", e);
                println!("Continuing with rebuild...");
            }
        } else if !output_json {
            println!("Deleted existing cache.");
        }
    }

    match TicketCache::open().await {
        Ok(cache) => {
            let start_sync = std::time::Instant::now();
            match cache.sync().await {
                Ok(_changed) => {
                    let sync_duration = start_sync.elapsed();

                    let ticket_count = cache.get_all_tickets().await?.len();

                    // Check if we need to regenerate embeddings due to model version mismatch
                    #[cfg(feature = "semantic-search")]
                    let needs_reemb = cache.needs_reembedding().await.unwrap_or(true);

                    #[cfg(not(feature = "semantic-search"))]
                    let _needs_reemb = false;

                    // Regenerate embeddings if needed (model version mismatch)
                    #[cfg(feature = "semantic-search")]
                    if needs_reemb && !output_json {
                        println!("\nEmbedding model version mismatch detected.");
                        println!("Regenerating embeddings for all tickets...");
                    }

                    #[cfg(feature = "semantic-search")]
                    if needs_reemb {
                        cache.regenerate_all_embeddings(output_json).await?;
                    }

                    let total_duration = start_total.elapsed();

                    // Update model version in meta table after successful rebuild
                    #[cfg(feature = "semantic-search")]
                    {
                        let conn = cache.create_connection().await?;
                        conn.execute(
                            "INSERT OR REPLACE INTO meta (key, value) VALUES ('embedding_model', ?1)",
                            (EMBEDDING_MODEL_NAME,)
                        ).await?;
                    }

                    #[allow(unused_mut)]
                    let mut output = json!({
                        "action": "cache_rebuilt",
                        "ticket_count": ticket_count,
                        "sync_time_ms": sync_duration.as_millis(),
                        "total_time_ms": total_duration.as_millis(),
                        "success": true,
                    });

                    #[cfg(feature = "semantic-search")]
                    {
                        output["embeddings_regenerated"] = json!(needs_reemb);
                        output["embedding_model"] = json!(EMBEDDING_MODEL_NAME);
                    }

                    CommandOutput::new(output)
                        .with_text(format!(
                            "Cache rebuilt successfully:\n  Tickets cached: {}\n  Sync time: {:?}\n  Total time: {:?}",
                            ticket_count, sync_duration, total_duration
                        ))
                        .print(output_json)?;

                    // Log the cache rebuild event with detailed information
                    #[cfg(feature = "semantic-search")]
                    let embeddings_regenerated = needs_reemb;
                    #[cfg(not(feature = "semantic-search"))]
                    let embeddings_regenerated = false;
                    
                    let details = json!({
                        "sync_time_ms": sync_duration.as_millis(),
                        "embeddings_regenerated": embeddings_regenerated,
                    });
                    log_cache_rebuilt(
                        "explicit_rebuild",
                        "janus cache rebuild command",
                        Some(total_duration.as_millis() as u64),
                        Some(ticket_count),
                        Some(details),
                    );
                }
                Err(e) => {
                    if !output_json {
                        println!("Error: cache sync failed during rebuild: {}", e);

                        if is_permission_error(&e) {
                            println!(
                                "\nPermission denied when accessing ticket files or cache directory."
                            );
                            println!("Please check file permissions and try again.");
                        } else if is_corruption_error(&e) {
                            println!("\nCache corruption detected during rebuild.");
                            println!("Please run 'janus cache clear' and try again.");
                        }
                    }

                    return Err(e);
                }
            }
        }
        Err(e) => {
            if !output_json {
                println!("Error: failed to initialize cache during rebuild: {}", e);

                if is_permission_error(&e) {
                    println!("\nPermission denied when accessing cache directory.");
                    println!("Cache directory: {}", crate::cache::cache_dir().display());
                    println!("Please check file permissions and try again.");
                }
            }

            return Err(e);
        }
    }

    Ok(())
}

fn db_path_from_current_dir() -> std::path::PathBuf {
    use crate::cache::{cache_db_path, repo_hash};
    if let Ok(repo_path) = std::env::current_dir() {
        let hash = repo_hash(&repo_path);
        cache_db_path(&hash)
    } else {
        std::path::PathBuf::from("unknown.cache.db")
    }
}

pub async fn cmd_cache_path(output_json: bool) -> Result<()> {
    let cache = TicketCache::open().await?;
    let path = cache.cache_db_path();

    CommandOutput::new(json!({
        "path": path.to_string_lossy(),
    }))
    .with_text(path.display().to_string())
    .print(output_json)
}
