use serde_json::json;
use std::fs;

use super::{CommandOutput, print_json};
use crate::cache::TicketCache;
use crate::error::{Result, is_corruption_error, is_permission_error};

pub async fn cmd_cache_status(output_json: bool) -> Result<()> {
    match TicketCache::open().await {
        Ok(cache) => {
            let db_path = cache.cache_db_path();
            let tickets = cache.get_all_tickets().await.unwrap_or_default();

            if output_json {
                let mut output = json!({
                    "database_path": db_path.to_string_lossy(),
                    "ticket_count": tickets.len(),
                    "status": "healthy",
                });

                if let Ok(meta) = fs::metadata(&db_path) {
                    output["database_size_bytes"] = json!(meta.len());
                    if let Ok(modified) = meta.modified() {
                        output["last_modified"] = json!(format!("{:?}", modified));
                    }
                }

                print_json(&output)?;
            } else {
                println!("Cache status:");
                println!("  Database path: {}", db_path.display());
                println!("  Cached tickets: {}", tickets.len());

                if let Ok(meta) = fs::metadata(&db_path) {
                    let size = meta.len();
                    println!("  Database size: {} bytes", size);
                    if let Ok(modified) = meta.modified() {
                        println!("  Last modified: {:?}", modified);
                    }
                }
            }
        }
        Err(e) => {
            let error_str = e.to_string();

            if output_json {
                let status = if is_corruption_error(&error_str) {
                    "corrupted"
                } else if is_permission_error(&error_str) {
                    "permission_denied"
                } else {
                    "not_available"
                };

                print_json(&json!({
                    "status": status,
                    "error": error_str,
                }))?;
            } else if is_corruption_error(&error_str) {
                eprintln!("Cache database is corrupted and cannot be opened.");
                eprintln!("\nTo fix this issue:");
                eprintln!("  1. Run 'janus cache clear' to delete the corrupted cache");
                eprintln!("  2. Run any janus command to rebuild the cache automatically");
                eprintln!("  3. Or run 'janus cache rebuild' to rebuild it manually");
            } else if is_permission_error(&error_str) {
                eprintln!("Cache database cannot be accessed due to permission issues.");
                eprintln!("\nTo fix this issue:");
                eprintln!("  1. Check file permissions for:");
                let cache_dir = crate::cache::cache_dir();
                eprintln!("     {}", cache_dir.display());
                eprintln!("  2. Ensure the cache directory is writable");
                eprintln!("  3. Try 'janus cache rebuild' after fixing permissions");
            } else {
                eprintln!("Cache not available: {}", e);
                eprintln!("Run 'janus cache rebuild' to create a cache.");
            }

            return Err(e);
        }
    }
    Ok(())
}

pub async fn cmd_cache_clear(output_json: bool) -> Result<()> {
    let db_path = match TicketCache::open().await {
        Ok(cache) => cache.cache_db_path(),
        Err(e) => {
            let error_str = e.to_string();
            if is_permission_error(&error_str) {
                return Err(e);
            }

            if db_path_from_current_dir().exists() {
                db_path_from_current_dir()
            } else {
                if output_json {
                    print_json(&json!({
                        "action": "cache_clear",
                        "success": true,
                        "message": "Cache does not exist or has already been cleared",
                    }))?;
                } else {
                    println!("Cache does not exist or has already been cleared.");
                    println!(
                        "\nThe cache will be created automatically on the next janus command."
                    );
                }
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

    if output_json {
        print_json(&json!({
            "action": "cache_cleared",
            "database_path": db_path.to_string_lossy(),
            "success": true,
        }))?;
    } else {
        println!("Cache cleared successfully.");
        println!("\nNote: The cache will be rebuilt automatically on the next janus command.");
    }
    Ok(())
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
        Ok(mut cache) => {
            let start_sync = std::time::Instant::now();
            match cache.sync().await {
                Ok(_changed) => {
                    let sync_duration = start_sync.elapsed();

                    let ticket_count = cache.get_all_tickets().await.unwrap_or_default().len();

                    let total_duration = start_total.elapsed();

                    if output_json {
                        print_json(&json!({
                            "action": "cache_rebuilt",
                            "ticket_count": ticket_count,
                            "sync_time_ms": sync_duration.as_millis(),
                            "total_time_ms": total_duration.as_millis(),
                            "success": true,
                        }))?;
                    } else {
                        println!("Cache rebuilt successfully:");
                        println!("  Tickets cached: {}", ticket_count);
                        println!("  Sync time: {:?}", sync_duration);
                        println!("  Total time: {:?}", total_duration);
                    }
                }
                Err(e) => {
                    if !output_json {
                        println!("Error: cache sync failed during rebuild: {}", e);

                        let error_str = e.to_string();
                        if is_permission_error(&error_str) {
                            println!(
                                "\nPermission denied when accessing ticket files or cache directory."
                            );
                            println!("Please check file permissions and try again.");
                        } else if is_corruption_error(&error_str) {
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

                let error_str = e.to_string();
                if is_permission_error(&error_str) {
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
