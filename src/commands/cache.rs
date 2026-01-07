use crate::cache::TicketCache;
use crate::error::Result;
use std::fs;

pub async fn cmd_cache_status() -> Result<()> {
    match TicketCache::open().await {
        Ok(cache) => {
            let db_path = cache.cache_db_path();
            let tickets = cache.get_all_tickets().await.unwrap_or_default();

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
        Err(e) => {
            let error_str = e.to_string();

            if error_str.contains("corrupted") || error_str.contains("CORRUPT") {
                println!("Cache database is corrupted and cannot be opened.");
                println!("\nTo fix this issue:");
                println!("  1. Run 'janus cache clear' to delete the corrupted cache");
                println!("  2. Run any janus command to rebuild the cache automatically");
                println!("  3. Or run 'janus cache rebuild' to rebuild it manually");
            } else if error_str.contains("AccessDenied") || error_str.contains("Permission") {
                println!("Cache database cannot be accessed due to permission issues.");
                println!("\nTo fix this issue:");
                println!("  1. Check file permissions for:");
                let cache_dir = crate::cache::cache_dir();
                println!("     {}", cache_dir.display());
                println!("  2. Ensure the cache directory is writable");
                println!("  3. Try 'janus cache rebuild' after fixing permissions");
            } else {
                println!("Cache not available: {}", e);
                println!("Run 'janus cache rebuild' to create a cache.");
            }
        }
    }
    Ok(())
}

pub async fn cmd_cache_clear() -> Result<()> {
    let db_path = match TicketCache::open().await {
        Ok(cache) => cache.cache_db_path(),
        Err(e) => {
            let error_str = e.to_string();
            if error_str.contains("AccessDenied") || error_str.contains("Permission") {
                return Err(e.into());
            }

            if db_path_from_current_dir().exists() {
                db_path_from_current_dir()
            } else {
                println!("Cache does not exist or has already been cleared.");
                println!("\nThe cache will be created automatically on the next janus command.");
                return Ok(());
            }
        }
    };

    println!("Deleting cache database: {}", db_path.display());

    if let Err(e) = fs::remove_file(&db_path) {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            println!("Error: Permission denied when trying to delete cache.");
            println!("Please check file permissions for: {}", db_path.display());
            return Err(e.into());
        }
        return Err(e.into());
    }

    println!("Cache cleared successfully.");
    println!("\nNote: The cache will be rebuilt automatically on the next janus command.");
    Ok(())
}

pub async fn cmd_cache_rebuild() -> Result<()> {
    println!("Rebuilding cache...");

    let db_path = db_path_from_current_dir();

    let start_total = std::time::Instant::now();

    if db_path.exists() {
        println!("Found existing cache at: {}", db_path.display());
        if let Err(e) = fs::remove_file(&db_path) {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                println!("Error: Permission denied when trying to delete existing cache.");
                println!("Please check file permissions for: {}", db_path.display());
                return Err(e.into());
            }
            println!("Warning: failed to delete existing cache: {}", e);
            println!("Continuing with rebuild...");
        } else {
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

                    println!("Cache rebuilt successfully:");
                    println!("  Tickets cached: {}", ticket_count);
                    println!("  Sync time: {:?}", sync_duration);
                    println!("  Total time: {:?}", total_duration);
                }
                Err(e) => {
                    println!("Error: cache sync failed during rebuild: {}", e);

                    let error_str = e.to_string();
                    if error_str.contains("AccessDenied") || error_str.contains("Permission") {
                        println!(
                            "\nPermission denied when accessing ticket files or cache directory."
                        );
                        println!("Please check file permissions and try again.");
                    } else if error_str.contains("corrupted") || error_str.contains("CORRUPT") {
                        println!("\nCache corruption detected during rebuild.");
                        println!("Please run 'janus cache clear' and try again.");
                    }

                    return Err(e.into());
                }
            }
        }
        Err(e) => {
            println!("Error: failed to initialize cache during rebuild: {}", e);

            let error_str = e.to_string();
            if error_str.contains("AccessDenied") || error_str.contains("Permission") {
                println!("\nPermission denied when accessing cache directory.");
                println!("Cache directory: {}", crate::cache::cache_dir().display());
                println!("Please check file permissions and try again.");
            }

            return Err(e.into());
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

pub async fn cmd_cache_path() -> Result<()> {
    let cache = TicketCache::open().await?;
    println!("{}", cache.cache_db_path().display());
    Ok(())
}
