//! Cache module for fast ticket and plan queries.
//!
//! This module provides a SQLite-based caching layer that acts as a read replica
//! of the `.janus/items/` and `.janus/plans/` directories. The cache enables
//! ~100x faster queries compared to parsing files from disk.
//!
//! ## Module Structure
//!
//! - `paths`: Helper functions for cache directory and database paths
//! - `types`: Cache-specific types (CachedPlanMetadata, CachedPhase)
//! - `database`: Cache lifecycle management (open, initialize, version validation)
//! - `sync`: Synchronization logic (sync_tickets, sync_plans)
//! - `queries`: Query operations (get_all_tickets, find_by_partial_id, etc.)
//! - `traits`: CacheableItem trait for generic sync implementation
//!
//! ## Usage
//!
//! The cache is automatically initialized and synced on every command invocation
//! via `get_or_init_cache()`. If the cache is unavailable, operations fall back
//! to direct file reads.

mod database;
mod paths;
mod queries;
mod sync;
mod traits;
mod types;

// Re-export public items
pub use database::TicketCache;
pub use paths::{cache_db_path, cache_dir, repo_hash};
pub use traits::CacheableItem;
pub use types::{CachedPhase, CachedPlanMetadata};

use tokio::sync::OnceCell;

use crate::error::{is_corruption_error, is_permission_error};

static GLOBAL_CACHE: OnceCell<Option<TicketCache>> = OnceCell::const_new();

/// Get or initialize the global cache instance.
///
/// This function lazily initializes the cache on first call and returns
/// a reference to it. If the cache cannot be initialized (e.g., due to
/// permission errors or corruption), it returns None and prints a warning.
pub async fn get_or_init_cache() -> Option<&'static TicketCache> {
    GLOBAL_CACHE
        .get_or_init(|| async {
            match TicketCache::open_with_corruption_handling().await {
                Ok(cache) => {
                    if let Err(e) = cache.sync().await {
                        eprintln!(
                            "Warning: cache sync failed: {}. Falling back to file reads.",
                            e
                        );

                        if is_corruption_error(&e) {
                            let db_path = cache.cache_db_path();
                            eprintln!("Cache appears corrupted at: {}", db_path.display());
                            eprintln!(
                                "Run 'janus cache clear' or 'janus cache rebuild' to fix this issue."
                            );
                        }

                        None
                    } else {
                        Some(cache)
                    }
                }
                Err(e) => {
                    if is_permission_error(&e) {
                        eprintln!(
                            "Warning: cannot access cache directory (permission denied). \
                             Falling back to file reads.",
                        );
                        eprintln!("Tip: Check file permissions or try 'janus cache rebuild'.");
                    } else if is_corruption_error(&e) {
                        eprintln!(
                            "Warning: cache database is corrupted. Falling back to file reads."
                        );
                        eprintln!("Tip: Run 'janus cache clear' or 'janus cache rebuild' to fix this.");
                    } else {
                        eprintln!(
                            "Warning: failed to open cache: {}. Falling back to file reads.",
                            e
                        );
                    }

                    None
                }
            }
        })
        .await
        .as_ref()
}

/// Sync the global cache with disk.
///
/// This should be called after modifying ticket or plan files to ensure
/// the cache reflects the latest state. If the cache is not initialized,
/// this is a no-op.
///
/// Returns Ok(true) if changes were synced, Ok(false) if no changes or cache unavailable.
pub async fn sync_cache() -> crate::error::Result<bool> {
    if let Some(cache) = get_or_init_cache().await {
        cache.sync().await
    } else {
        Ok(false)
    }
}

/// Get the ticket cache, returning an error if unavailable.
///
/// This is a convenience wrapper around `get_or_init_cache()` that returns
/// a Result instead of Option, making it easier to use in contexts where
/// you want to propagate errors.
pub async fn get_ticket_cache() -> crate::error::Result<&'static TicketCache> {
    match get_or_init_cache().await {
        Some(cache) => Ok(cache),
        None => Err(crate::error::JanusError::CacheNotAvailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;

    /// Helper to get the first row from a query result, avoiding .unwrap().unwrap() pattern
    async fn get_first_row(rows: &mut turso::Rows) -> turso::Row {
        let row_opt = rows.next().await.expect("query failed");
        row_opt.expect("expected at least one row")
    }

    fn create_test_ticket(
        dir: &std::path::Path,
        ticket_id: &str,
        title: &str,
    ) -> std::path::PathBuf {
        create_test_ticket_with_body(dir, ticket_id, title, 2, "")
    }

    fn create_test_ticket_with_body(
        dir: &std::path::Path,
        ticket_id: &str,
        title: &str,
        priority: u8,
        body: &str,
    ) -> std::path::PathBuf {
        let tickets_dir = dir.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        let ticket_path = tickets_dir.join(format!("{}.md", ticket_id));
        let content = if body.is_empty() {
            format!(
                r#"---
id: {}
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: {}
---
# {}
"#,
                ticket_id, priority, title
            )
        } else {
            format!(
                r#"---
id: {}
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: {}
---
# {}

{}
"#,
                ticket_id, priority, title, body
            )
        };
        fs::write(&ticket_path, content).unwrap();
        ticket_path
    }

    fn create_test_plan(
        dir: &std::path::Path,
        plan_id: &str,
        title: &str,
        is_phased: bool,
    ) -> std::path::PathBuf {
        let plans_dir = dir.join(".janus/plans");
        fs::create_dir_all(&plans_dir).unwrap();

        let plan_path = plans_dir.join(format!("{}.md", plan_id));
        let content = if is_phased {
            format!(
                r#"---
id: {}
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# {}

Description of the plan.

## Phase 1: Infrastructure

### Tickets

1. j-a1b2
2. j-c3d4

## Phase 2: Implementation

### Tickets

1. j-e5f6
"#,
                plan_id, title
            )
        } else {
            format!(
                r#"---
id: {}
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# {}

Description of the plan.

## Tickets

1. j-a1b2
2. j-c3d4
3. j-e5f6
"#,
                plan_id, title
            )
        };
        fs::write(&plan_path, content).unwrap();
        plan_path
    }

    #[tokio::test]
    #[serial]
    async fn test_cache_initialization() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_cache_initialization");
        fs::create_dir_all(&repo_path).unwrap();

        std::env::set_current_dir(&repo_path).unwrap();

        let cache = TicketCache::open().await.unwrap();
        let db_path = cache.cache_db_path();

        assert!(db_path.exists());
        assert!(db_path.is_absolute());
    }

    #[tokio::test]
    #[serial]
    async fn test_wal_mode_enabled() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_wal_mode");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let cache = TicketCache::open().await.unwrap();

        // Verify WAL mode is enabled (this is the key fix for concurrent access)
        let conn = cache.create_connection().await.unwrap();
        let mut rows = conn.query("PRAGMA journal_mode", ()).await.unwrap();
        let row = get_first_row(&mut rows).await;
        let mode: String = row.get(0).unwrap();
        assert_eq!(mode.to_lowercase(), "wal", "WAL mode should be enabled");

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_repo_path_stored_in_meta() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_repo_path_stored_in_meta");
        fs::create_dir_all(&repo_path).unwrap();
        let repo_path_str = repo_path
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();

        std::env::set_current_dir(&repo_path).unwrap();

        let cache = TicketCache::open().await.unwrap();

        let conn = cache.create_connection().await.unwrap();
        let mut rows = conn
            .query("SELECT value FROM meta WHERE key = 'repo_path'", ())
            .await
            .unwrap();

        let stored_path: Option<String> = rows.next().await.unwrap().map(|row| row.get(0).unwrap());

        assert_eq!(stored_path, Some(repo_path_str));
    }

    #[tokio::test]
    #[serial]
    async fn test_cache_version_stored_in_meta() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_cache_version_stored_in_meta");
        fs::create_dir_all(&repo_path).unwrap();

        std::env::set_current_dir(&repo_path).unwrap();

        let cache = TicketCache::open().await.unwrap();

        let conn = cache.create_connection().await.unwrap();
        let mut rows = conn
            .query("SELECT value FROM meta WHERE key = 'cache_version'", ())
            .await
            .unwrap();

        let stored_version: Option<String> =
            rows.next().await.unwrap().map(|row| row.get(0).unwrap());

        #[cfg(feature = "semantic-search")]
        assert_eq!(stored_version, Some("13-semantic".to_string()));
        #[cfg(not(feature = "semantic-search"))]
        assert_eq!(stored_version, Some("13".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_creates_entries() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_creates_entries");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");
        create_test_ticket(&repo_path, "j-c3d4", "Ticket 2");
        create_test_ticket(&repo_path, "j-e5f6", "Ticket 3");

        let cache = TicketCache::open().await.unwrap();
        let changed = cache.sync().await.unwrap();

        assert!(changed);

        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query("SELECT COUNT(*) FROM tickets", ())
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 3);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_detects_additions() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_detects_additions");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");

        let cache = TicketCache::open().await.unwrap();
        let changed1 = cache.sync().await.unwrap();
        assert!(changed1);

        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query("SELECT COUNT(*) FROM tickets", ())
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count1: i64 = row.get(0).unwrap();
        assert_eq!(count1, 1);

        create_test_ticket(&repo_path, "j-c3d4", "Ticket 2");

        std::thread::sleep(std::time::Duration::from_millis(10));

        let changed2 = cache.sync().await.unwrap();
        assert!(changed2);

        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query("SELECT COUNT(*) FROM tickets", ())
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count2: i64 = row.get(0).unwrap();
        assert_eq!(count2, 2);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_detects_deletions() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_detects_deletions");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let ticket_path = create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query("SELECT COUNT(*) FROM tickets", ())
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count1: i64 = row.get(0).unwrap();
        assert_eq!(count1, 1);

        fs::remove_file(&ticket_path).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        let changed = cache.sync().await.unwrap();
        assert!(changed);

        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query("SELECT COUNT(*) FROM tickets", ())
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count2: i64 = row.get(0).unwrap();
        assert_eq!(count2, 0);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_detects_modifications() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_detects_modifications");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let ticket_path = create_test_ticket(&repo_path, "j-a1b2", "Original Title");

        std::thread::sleep(std::time::Duration::from_millis(10));

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query("SELECT title FROM tickets WHERE ticket_id = ?1", ["j-a1b2"])
            .await
            .unwrap();
        let original_title: Option<String> = if let Some(row) = rows.next().await.unwrap() {
            row.get(0).ok()
        } else {
            None
        };
        assert_eq!(original_title, Some("Original Title".to_string()));

        std::thread::sleep(std::time::Duration::from_millis(1100));

        let content = fs::read_to_string(&ticket_path).unwrap();
        let modified_content = content.replace("Original Title", "Modified Title");
        fs::write(&ticket_path, modified_content).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        let changed = cache.sync().await.unwrap();
        assert!(changed);

        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query("SELECT title FROM tickets WHERE ticket_id = ?1", ["j-a1b2"])
            .await
            .unwrap();
        let modified_title: Option<String> = if let Some(row) = rows.next().await.unwrap() {
            row.get(0).ok()
        } else {
            None
        };
        assert_eq!(modified_title, Some("Modified Title".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_serialize_deserialize_arrays() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_serialize_deserialize");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let _cache = TicketCache::open().await.unwrap();

        let arr = vec!["j-a1b2".to_string(), "j-c3d4".to_string()];
        let json = TicketCache::serialize_array(&arr).unwrap();

        assert!(json.starts_with('['));
        assert!(json.ends_with(']'));

        let decoded: Vec<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, arr);

        let empty_arr: Vec<String> = vec![];
        let empty_json = TicketCache::serialize_array(&empty_arr).unwrap();
        assert_eq!(empty_json, "[]");

        let db_path = _cache.cache_db_path();
        drop(_cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_scan_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_scan_directory");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");
        create_test_ticket(&repo_path, "j-c3d4", "Ticket 2");

        let non_md_file = tickets_dir.join("other.txt");
        fs::write(&non_md_file, "not a ticket").unwrap();

        let cache = TicketCache::open().await.unwrap();
        let files = TicketCache::scan_directory_static(&tickets_dir).unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.contains_key("j-a1b2"));
        assert!(files.contains_key("j-c3d4"));

        for mtime_ns in files.values() {
            assert!(*mtime_ns > 0);
        }

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[test]
    fn test_deserialize_array_handles_empty_and_invalid() {
        // Empty string should return empty array
        let result: Vec<String> = TicketCache::deserialize_array(None).unwrap();
        assert_eq!(result, Vec::<String>::new());

        // Some empty string should return empty array
        let result: Vec<String> = TicketCache::deserialize_array(Some("")).unwrap();
        assert_eq!(result, Vec::<String>::new());

        // Valid JSON array should parse correctly
        let result: Vec<String> =
            TicketCache::deserialize_array(Some(r#"["a", "b", "c"]"#)).unwrap();
        assert_eq!(
            result,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_with_deps_and_links() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_deps_links");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        // Create a ticket with deps and links (using JSON array format on single line)
        let ticket_path = tickets_dir.join("j-a1b2.md");
        let content = r#"---
id: j-a1b2
uuid: 550e8400-e29b-41d4-a716-446655440600
status: new
deps: ["j-dep1", "j-dep2"]
links: ["j-link1"]
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Ticket with deps
"#;
        fs::write(&ticket_path, content).unwrap();

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Verify deps were stored correctly
        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query(
                "SELECT deps, links FROM tickets WHERE ticket_id = ?1",
                ["j-a1b2"],
            )
            .await
            .unwrap();

        let row = get_first_row(&mut rows).await;
        let deps_json: Option<String> = row.get(0).ok();
        let links_json: Option<String> = row.get(1).ok();

        assert!(deps_json.is_some());
        assert!(links_json.is_some());

        let deps: Vec<String> = serde_json::from_str(&deps_json.unwrap()).unwrap();
        let links: Vec<String> = serde_json::from_str(&links_json.unwrap()).unwrap();

        assert_eq!(deps, vec!["j-dep1", "j-dep2"]);
        assert_eq!(links, vec!["j-link1"]);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_with_all_fields() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_all_fields");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        // Create a ticket with all fields populated
        // Note: parser uses "external-ref" (with hyphen), not "external_ref"
        let ticket_path = tickets_dir.join("j-full.md");
        let content = r#"---
id: j-full
uuid: 550e8400-e29b-41d4-a716-446655440500
status: in_progress
deps: []
links: []
created: 2024-06-15T10:30:00Z
type: bug
priority: 0
parent: j-parent
external-ref: GH-123
remote: github
---
# Full Ticket
"#;
        fs::write(&ticket_path, content).unwrap();

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Verify all fields were stored correctly
        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query(
                "SELECT status, title, priority, ticket_type, parent, external_ref, remote 
                 FROM tickets WHERE ticket_id = ?1",
                ["j-full"],
            )
            .await
            .unwrap();

        let row = get_first_row(&mut rows).await;
        let status: Option<String> = row.get(0).ok();
        let title: Option<String> = row.get(1).ok();
        let priority: Option<i64> = row.get(2).ok();
        let ticket_type: Option<String> = row.get(3).ok();
        let parent: Option<String> = row.get(4).ok();
        let external_ref: Option<String> = row.get(5).ok();
        let remote: Option<String> = row.get(6).ok();

        assert_eq!(status, Some("in_progress".to_string()));
        assert_eq!(title, Some("Full Ticket".to_string()));
        assert_eq!(priority, Some(0));
        assert_eq!(ticket_type, Some("bug".to_string()));
        assert_eq!(parent, Some("j-parent".to_string()));
        assert_eq!(external_ref, Some("GH-123".to_string()));
        assert_eq!(remote, Some("github".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_reopen_existing_cache() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_reopen_cache");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");
        create_test_ticket(&repo_path, "j-c3d4", "Ticket 2");

        // Open cache and sync
        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let db_path = cache.cache_db_path();

        // Drop and reopen
        drop(cache);

        // Reopen the cache - should preserve existing data
        let cache2 = TicketCache::open().await.unwrap();

        let mut rows = cache2
            .create_connection()
            .await
            .unwrap()
            .query("SELECT COUNT(*) FROM tickets", ())
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count: i64 = row.get(0).unwrap();

        // Data should still be there from before
        assert_eq!(count, 2);

        drop(cache2);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_database_indexes_created() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_indexes");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let cache = TicketCache::open().await.unwrap();

        // Query for indexes
        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query(
                "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='tickets'",
                (),
            )
            .await
            .unwrap();

        let mut indexes = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            let name: String = row.get(0).unwrap();
            indexes.push(name);
        }

        assert!(indexes.contains(&"idx_tickets_status".to_string()));
        assert!(indexes.contains(&"idx_tickets_priority".to_string()));
        assert!(indexes.contains(&"idx_tickets_type".to_string()));
        assert!(indexes.contains(&"idx_tickets_status_priority".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_empty_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_empty");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create empty tickets directory
        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        let cache = TicketCache::open().await.unwrap();

        // Sync with empty directory should return false (no changes)
        let changed = cache.sync().await.unwrap();
        assert!(!changed);

        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query("SELECT COUNT(*) FROM tickets", ())
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 0);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_creates_tickets_dir_if_missing() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_missing_dir");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Don't create the tickets directory

        let cache = TicketCache::open().await.unwrap();

        // Sync should create the directory and return false
        let changed = cache.sync().await.unwrap();
        assert!(!changed);

        // Verify directory was created
        let tickets_dir = repo_path.join(".janus/items");
        assert!(tickets_dir.exists());

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_get_all_tickets() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_get_all_tickets");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");
        create_test_ticket(&repo_path, "j-c3d4", "Ticket 2");
        create_test_ticket(&repo_path, "j-e5f6", "Ticket 3");

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let tickets = cache.get_all_tickets().await.unwrap();
        assert_eq!(tickets.len(), 3);

        let titles: Vec<&str> = tickets.iter().filter_map(|t| t.title.as_deref()).collect();
        assert!(titles.contains(&"Ticket 1"));
        assert!(titles.contains(&"Ticket 2"));
        assert!(titles.contains(&"Ticket 3"));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_get_ticket() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_get_ticket");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Test Ticket");

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let ticket = cache.get_ticket("j-a1b2").await.unwrap();
        assert!(ticket.is_some());

        let metadata = ticket.unwrap();
        assert_eq!(metadata.id, Some("j-a1b2".to_string()));
        assert_eq!(metadata.title, Some("Test Ticket".to_string()));

        let nonexistent = cache.get_ticket("j-xxxx").await.unwrap();
        assert!(nonexistent.is_none());

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_find_by_partial_id() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_find_by_partial_id");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");
        create_test_ticket(&repo_path, "j-c3d4", "Ticket 2");
        create_test_ticket(&repo_path, "j-e5f6", "Ticket 3");

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let matches = cache.find_by_partial_id("j-a").await.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "j-a1b2");

        let matches = cache.find_by_partial_id("j-").await.unwrap();
        assert_eq!(matches.len(), 3);

        let matches = cache.find_by_partial_id("j-xxx").await.unwrap();
        assert_eq!(matches.len(), 0);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_find_by_partial_id_ambiguous() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_find_by_partial_id_ambiguous");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket A1");
        create_test_ticket(&repo_path, "j-a2c3", "Ticket A2");

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let matches = cache.find_by_partial_id("j-a").await.unwrap();
        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&"j-a1b2".to_string()));
        assert!(matches.contains(&"j-a2c3".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_build_ticket_map() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_build_ticket_map");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");
        create_test_ticket(&repo_path, "j-c3d4", "Ticket 2");
        create_test_ticket(&repo_path, "j-e5f6", "Ticket 3");

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let map = cache.build_ticket_map().await.unwrap();
        assert_eq!(map.len(), 3);

        assert!(map.contains_key("j-a1b2"));
        assert!(map.contains_key("j-c3d4"));
        assert!(map.contains_key("j-e5f6"));

        let ticket1 = map.get("j-a1b2").unwrap();
        assert_eq!(ticket1.title, Some("Ticket 1".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_get_all_tickets_with_all_fields() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_get_all_fields");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        let ticket_path = tickets_dir.join("j-full.md");
        let content = r#"---
id: j-full
uuid: 550e8400-e29b-41d4-a716-446655440000
status: in_progress
deps: ["j-dep1", "j-dep2"]
links: ["j-link1"]
created: 2024-06-15T10:30:00Z
type: bug
priority: 0
parent: j-parent
external-ref: GH-123
remote: github
---
# Full Ticket
"#;
        fs::write(&ticket_path, content).unwrap();

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let tickets = cache.get_all_tickets().await.unwrap();
        assert_eq!(tickets.len(), 1);

        let ticket = &tickets[0];
        assert_eq!(ticket.id, Some("j-full".to_string()));
        assert_eq!(ticket.title, Some("Full Ticket".to_string()));
        assert_eq!(ticket.status, Some(crate::types::TicketStatus::InProgress));
        assert_eq!(ticket.ticket_type, Some(crate::types::TicketType::Bug));
        assert_eq!(ticket.priority, Some(crate::types::TicketPriority::P0));
        assert_eq!(ticket.parent, Some("j-parent".to_string()));
        assert_eq!(ticket.external_ref, Some("GH-123".to_string()));
        assert_eq!(ticket.remote, Some("github".to_string()));
        assert_eq!(ticket.deps, vec!["j-dep1", "j-dep2"]);
        assert_eq!(ticket.links, vec!["j-link1"]);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    // =========================================================================
    // Plan caching tests
    // =========================================================================

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_simple_plan() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_simple_plan");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_plan(&repo_path, "plan-a1b2", "Simple Test Plan", false);

        let cache = TicketCache::open().await.unwrap();
        let changed = cache.sync().await.unwrap();

        assert!(changed);

        // Verify plan was cached
        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query("SELECT COUNT(*) FROM plans", ())
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 1);

        // Verify plan data
        let plan = cache.get_plan("plan-a1b2").await.unwrap();
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.id, Some("plan-a1b2".to_string()));
        assert_eq!(plan.title, Some("Simple Test Plan".to_string()));
        assert_eq!(plan.structure_type, "simple");
        assert!(plan.is_simple());
        assert!(!plan.is_phased());
        assert_eq!(plan.tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_phased_plan() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_phased_plan");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_plan(&repo_path, "plan-b2c3", "Phased Test Plan", true);

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let plan = cache.get_plan("plan-b2c3").await.unwrap();
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.id, Some("plan-b2c3".to_string()));
        assert_eq!(plan.title, Some("Phased Test Plan".to_string()));
        assert_eq!(plan.structure_type, "phased");
        assert!(plan.is_phased());
        assert!(!plan.is_simple());

        // Verify phases
        assert_eq!(plan.phases.len(), 2);
        assert_eq!(plan.phases[0].number, "1");
        assert_eq!(plan.phases[0].name, "Infrastructure");
        assert_eq!(plan.phases[0].tickets, vec!["j-a1b2", "j-c3d4"]);
        assert_eq!(plan.phases[1].number, "2");
        assert_eq!(plan.phases[1].name, "Implementation");
        assert_eq!(plan.phases[1].tickets, vec!["j-e5f6"]);

        // Verify all_tickets helper
        let all_tickets = plan.all_tickets();
        assert_eq!(all_tickets, vec!["j-a1b2", "j-c3d4", "j-e5f6"]);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_multiple_plans() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_multiple_plans");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_plan(&repo_path, "plan-a1b2", "Plan One", false);
        create_test_plan(&repo_path, "plan-c3d4", "Plan Two", true);
        create_test_plan(&repo_path, "plan-e5f6", "Plan Three", false);

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let plans = cache.get_all_plans().await.unwrap();
        assert_eq!(plans.len(), 3);

        // Verify titles
        let titles: Vec<&str> = plans.iter().filter_map(|p| p.title.as_deref()).collect();
        assert!(titles.contains(&"Plan One"));
        assert!(titles.contains(&"Plan Two"));
        assert!(titles.contains(&"Plan Three"));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_detects_additions() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_plans_additions");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_plan(&repo_path, "plan-a1b2", "Plan One", false);

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let plans = cache.get_all_plans().await.unwrap();
        assert_eq!(plans.len(), 1);

        // Add another plan
        create_test_plan(&repo_path, "plan-c3d4", "Plan Two", true);

        std::thread::sleep(std::time::Duration::from_millis(10));

        let changed = cache.sync().await.unwrap();
        assert!(changed);

        let plans = cache.get_all_plans().await.unwrap();
        assert_eq!(plans.len(), 2);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_detects_deletions() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_plans_deletions");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let plan_path = create_test_plan(&repo_path, "plan-a1b2", "Plan One", false);

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let plans = cache.get_all_plans().await.unwrap();
        assert_eq!(plans.len(), 1);

        // Delete the plan
        fs::remove_file(&plan_path).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        let changed = cache.sync().await.unwrap();
        assert!(changed);

        let plans = cache.get_all_plans().await.unwrap();
        assert_eq!(plans.len(), 0);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_detects_modifications() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_plans_modifications");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let plan_path = create_test_plan(&repo_path, "plan-a1b2", "Original Title", false);

        std::thread::sleep(std::time::Duration::from_millis(10));

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let plan = cache.get_plan("plan-a1b2").await.unwrap().unwrap();
        assert_eq!(plan.title, Some("Original Title".to_string()));

        // Modify the plan
        std::thread::sleep(std::time::Duration::from_millis(1100));

        let content = fs::read_to_string(&plan_path).unwrap();
        let modified_content = content.replace("Original Title", "Modified Title");
        fs::write(&plan_path, modified_content).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        let changed = cache.sync().await.unwrap();
        assert!(changed);

        let plan = cache.get_plan("plan-a1b2").await.unwrap().unwrap();
        assert_eq!(plan.title, Some("Modified Title".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_find_plan_by_partial_id() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_find_plan_partial");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_plan(&repo_path, "plan-a1b2", "Plan One", false);
        create_test_plan(&repo_path, "plan-a2c3", "Plan Two", true);
        create_test_plan(&repo_path, "plan-b3d4", "Plan Three", false);

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Find by prefix
        let matches = cache.find_plan_by_partial_id("plan-a").await.unwrap();
        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&"plan-a1b2".to_string()));
        assert!(matches.contains(&"plan-a2c3".to_string()));

        // Find exact match
        let matches = cache.find_plan_by_partial_id("plan-b3d4").await.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "plan-b3d4");

        // Find all plans
        let matches = cache.find_plan_by_partial_id("plan-").await.unwrap();
        assert_eq!(matches.len(), 3);

        // No match
        let matches = cache.find_plan_by_partial_id("plan-xxx").await.unwrap();
        assert_eq!(matches.len(), 0);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_no_changes() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_plans_no_changes");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_plan(&repo_path, "plan-a1b2", "Test Plan", false);

        let cache = TicketCache::open().await.unwrap();

        // First sync should return true
        let changed1 = cache.sync().await.unwrap();
        assert!(changed1);

        // Second sync with no changes should return false
        let changed2 = cache.sync().await.unwrap();
        assert!(!changed2);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_empty_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_plans_empty");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create empty plans directory
        let plans_dir = repo_path.join(".janus/plans");
        fs::create_dir_all(&plans_dir).unwrap();

        let cache = TicketCache::open().await.unwrap();
        let changed = cache.sync().await.unwrap();
        assert!(!changed);

        let plans = cache.get_all_plans().await.unwrap();
        assert_eq!(plans.len(), 0);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_plans_creates_directory_if_missing() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_plans_missing_dir");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Don't create the plans directory

        let cache = TicketCache::open().await.unwrap();
        let changed = cache.sync().await.unwrap();
        assert!(!changed);

        // Verify plans directory was created
        let plans_dir = repo_path.join(".janus/plans");
        assert!(plans_dir.exists());

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_plans_index_created() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_plans_index");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let cache = TicketCache::open().await.unwrap();

        // Query for indexes on plans table
        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query(
                "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='plans'",
                (),
            )
            .await
            .unwrap();

        let mut indexes = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            let name: String = row.get(0).unwrap();
            indexes.push(name);
        }

        assert!(indexes.contains(&"idx_plans_structure_type".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_logs_warnings_for_parse_errors() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_parse_errors");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        // Create a valid ticket
        create_test_ticket(&repo_path, "j-valid", "Valid Ticket");

        // Create an invalid ticket (missing YAML frontmatter - just plain text)
        let invalid_path = tickets_dir.join("j-invalid.md");
        let invalid_content =
            "This is not a valid ticket file - no frontmatter\n\n# Invalid Ticket\n";
        fs::write(&invalid_path, invalid_content).unwrap();

        // Capture stderr to verify warning is logged
        let cache = TicketCache::open().await.unwrap();

        // Sync should succeed and log a warning about the invalid ticket
        let changed = cache.sync().await.unwrap();
        assert!(changed);

        // Verify the valid ticket was synced
        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query(
                "SELECT COUNT(*) FROM tickets WHERE ticket_id = ?1",
                ["j-valid"],
            )
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 1);

        // Verify the invalid ticket was not synced
        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query(
                "SELECT COUNT(*) FROM tickets WHERE ticket_id = ?1",
                ["j-invalid"],
            )
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 0);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_removes_stale_cache_on_parse_failure() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("stale_cache_test");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        // Create a valid ticket
        let ticket_path = tickets_dir.join("j-test.md");
        let valid_content = r#"---
id: j-test
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
---
# Test Ticket
This is a valid ticket.
"#;
        fs::write(&ticket_path, valid_content).unwrap();

        // Sync to populate cache
        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Verify ticket is in cache
        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query(
                "SELECT title, status FROM tickets WHERE ticket_id = ?1",
                ["j-test"],
            )
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let title: String = row.get(0).unwrap();
        let status: String = row.get(1).unwrap();
        assert_eq!(title, "Test Ticket");
        assert_eq!(status, "new");

        // Sleep to ensure mtime changes
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Corrupt the ticket file (remove frontmatter)
        let invalid_content = "# Corrupted Ticket\nThis file has no frontmatter\n";
        fs::write(&ticket_path, invalid_content).unwrap();

        // Sync again - should detect modification but keep stale cache entry
        let changed = cache.sync().await.unwrap();
        assert!(changed);

        // Verify the stale cache entry is still present (not removed)
        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query(
                "SELECT title, status FROM tickets WHERE ticket_id = ?1",
                ["j-test"],
            )
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let title: String = row.get(0).unwrap();
        let status: String = row.get(1).unwrap();
        assert_eq!(
            title, "Test Ticket",
            "Stale cache entry should be preserved"
        );
        assert_eq!(status, "new", "Stale cache entry should have old status");

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    fn create_test_ticket_with_spawned_from(
        dir: &std::path::Path,
        ticket_id: &str,
        title: &str,
        spawned_from: &str,
    ) -> std::path::PathBuf {
        let tickets_dir = dir.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        let ticket_path = tickets_dir.join(format!("{}.md", ticket_id));
        let content = format!(
            r#"---
id: {}
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
spawned-from: {}
---
# {}
"#,
            ticket_id, spawned_from, title
        );
        fs::write(&ticket_path, content).unwrap();
        ticket_path
    }

    #[tokio::test]
    #[serial]
    async fn test_get_children_count() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_get_children_count");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create a parent ticket
        create_test_ticket(&repo_path, "j-parent", "Parent Ticket");

        // Create 3 child tickets spawned from the parent
        create_test_ticket_with_spawned_from(&repo_path, "j-child1", "Child 1", "j-parent");
        create_test_ticket_with_spawned_from(&repo_path, "j-child2", "Child 2", "j-parent");
        create_test_ticket_with_spawned_from(&repo_path, "j-child3", "Child 3", "j-parent");

        // Create another ticket with no children
        create_test_ticket(&repo_path, "j-solo", "Solo Ticket");

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Test: parent should have 3 children
        let parent_count = cache.get_children_count("j-parent").await.unwrap();
        assert_eq!(parent_count, 3, "Parent should have 3 children");

        // Test: solo ticket should have 0 children
        let solo_count = cache.get_children_count("j-solo").await.unwrap();
        assert_eq!(solo_count, 0, "Solo ticket should have 0 children");

        // Test: nonexistent ticket should have 0 children
        let nonexistent_count = cache.get_children_count("j-nonexistent").await.unwrap();
        assert_eq!(
            nonexistent_count, 0,
            "Nonexistent ticket should have 0 children"
        );

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_sync_with_triaged_field() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_sync_triaged");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        let tickets_dir = repo_path.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        let ticket_path = tickets_dir.join("j-a1b2.md");
        let content = r#"---
id: j-a1b2
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
triaged: true
---
# A Triaged Ticket
"#;
        fs::write(&ticket_path, content).unwrap();

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let ticket = cache.get_ticket("j-a1b2").await.unwrap();
        assert!(ticket.is_some());
        let ticket = ticket.unwrap();
        assert_eq!(ticket.triaged, Some(true));

        let tickets = cache.get_all_tickets().await.unwrap();
        assert_eq!(tickets.len(), 1);
        assert_eq!(tickets[0].triaged, Some(true));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    // =========================================================================
    // Search tickets tests
    // =========================================================================

    #[tokio::test]
    #[serial]
    async fn test_search_tickets_empty_query() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_search_empty");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket(&repo_path, "j-a1b2", "Ticket 1");
        create_test_ticket(&repo_path, "j-c3d4", "Ticket 2");
        create_test_ticket(&repo_path, "j-e5f6", "Ticket 3");

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let results = cache.search_tickets("").await.unwrap();
        assert_eq!(results.len(), 3, "Empty query should return all tickets");

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_search_tickets_priority_only() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_search_priority");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket_with_body(&repo_path, "j-p0", "P0 Ticket", 0, "");
        create_test_ticket_with_body(&repo_path, "j-p1", "P1 Ticket", 1, "");
        create_test_ticket_with_body(&repo_path, "j-p2", "P2 Ticket", 2, "");

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Search for P0 tickets only
        let results = cache.search_tickets("p0").await.unwrap();
        assert_eq!(results.len(), 1, "Should return only P0 ticket");
        assert_eq!(
            results[0].id,
            Some("j-p0".to_string()),
            "Should return the P0 ticket"
        );
        assert_eq!(
            results[0].priority,
            Some(crate::types::TicketPriority::P0),
            "Priority should be P0"
        );

        // Search for P1 tickets only
        let results = cache.search_tickets("p1").await.unwrap();
        assert_eq!(results.len(), 1, "Should return only P1 ticket");
        assert_eq!(
            results[0].id,
            Some("j-p1".to_string()),
            "Should return the P1 ticket"
        );

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_search_tickets_text_in_title() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_search_title");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket_with_body(&repo_path, "j-a1b2", "Fix the bug", 2, "Some other content");
        create_test_ticket_with_body(&repo_path, "j-c3d4", "Add new feature", 2, "No issues here");

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let results = cache.search_tickets("bug").await.unwrap();
        assert_eq!(results.len(), 1, "Should return one ticket matching 'bug'");
        assert_eq!(
            results[0].id,
            Some("j-a1b2".to_string()),
            "Should return the ticket with 'bug' in title"
        );
        assert_eq!(
            results[0].title,
            Some("Fix the bug".to_string()),
            "Title should contain 'bug'"
        );

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_search_tickets_text_in_body() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_search_body");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket_with_body(
            &repo_path,
            "j-a1b2",
            "Auth Issue",
            2,
            "Users are experiencing authentication error when logging in.",
        );
        create_test_ticket_with_body(
            &repo_path,
            "j-c3d4",
            "Other Task",
            2,
            "Just some random content without the keyword.",
        );

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        let results = cache.search_tickets("authentication").await.unwrap();
        assert_eq!(
            results.len(),
            1,
            "Should return one ticket matching 'authentication'"
        );
        assert_eq!(
            results[0].id,
            Some("j-a1b2".to_string()),
            "Should return the ticket with 'authentication' in body"
        );
        assert!(
            results[0]
                .body
                .as_ref()
                .map(|b| b.contains("authentication"))
                .unwrap_or(false),
            "Body should contain 'authentication'"
        );

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_search_tickets_combined_priority_and_text() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_search_combined");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket_with_body(
            &repo_path,
            "j-p0-ticket",
            "P0 Critical Issue",
            0,
            "We need to fix this critical issue.",
        );
        create_test_ticket_with_body(
            &repo_path,
            "j-p1-ticket",
            "P1 Normal Issue",
            1,
            "This also needs to be addressed but less urgent.",
        );
        create_test_ticket_with_body(
            &repo_path,
            "j-p0-other",
            "P0 Other",
            0,
            "Different task without the keyword.",
        );

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // First, verify that priority-only search works
        let p0_results = cache.search_tickets("p0").await.unwrap();
        assert_eq!(
            p0_results.len(),
            2,
            "Priority-only p0 search should return 2 P0 tickets"
        );

        // Verify text-only search works
        let fix_results = cache.search_tickets("fix").await.unwrap();
        assert_eq!(
            fix_results.len(),
            1,
            "Text-only 'fix' search should return 1 ticket"
        );
        assert_eq!(
            fix_results[0].id,
            Some("j-p0-ticket".to_string()),
            "Should be the P0 ticket with 'fix' in body"
        );

        // Now search for P0 tickets containing "fix"
        let results = cache.search_tickets("p0 fix").await.unwrap();
        assert_eq!(results.len(), 1, "Should return only P0 ticket with 'fix'");
        assert_eq!(
            results[0].id,
            Some("j-p0-ticket".to_string()),
            "Should return the P0 ticket with 'fix' in body"
        );
        assert_eq!(
            results[0].priority,
            Some(crate::types::TicketPriority::P0),
            "Priority should be P0"
        );
        assert!(
            results[0]
                .body
                .as_ref()
                .map(|b| b.contains("fix"))
                .unwrap_or(false),
            "Body should contain 'fix'"
        );

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_search_tickets_special_characters() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_search_special");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Test % character (LIKE wildcard)
        create_test_ticket_with_body(
            &repo_path,
            "j-percent",
            "Progress Ticket",
            2,
            "The task is 100% complete.",
        );

        // Test _ character (LIKE single char wildcard)
        create_test_ticket_with_body(
            &repo_path,
            "j-underscore",
            "Underscore Ticket",
            2,
            "Use variable_name for clarity.",
        );

        // Test \ character (like escape character)
        create_test_ticket_with_body(
            &repo_path,
            "j-backslash",
            "Path Ticket",
            2,
            "Use C:\\Users\\name\\path for Windows.",
        );

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Search for % character
        let results = cache.search_tickets("100%").await.unwrap();
        assert_eq!(
            results.len(),
            1,
            "% should be escaped and matched literally"
        );
        assert_eq!(
            results[0].id,
            Some("j-percent".to_string()),
            "Should find ticket with '%' in body"
        );

        // Search for _ character
        let results = cache.search_tickets("variable_name").await.unwrap();
        assert_eq!(
            results.len(),
            1,
            "_ should be escaped and matched literally"
        );
        assert_eq!(
            results[0].id,
            Some("j-underscore".to_string()),
            "Should find ticket with '_' in body"
        );

        // Search for \ character
        let results = cache.search_tickets("C:\\Users").await.unwrap();
        assert_eq!(
            results.len(),
            1,
            "\\ should be escaped and matched literally"
        );
        assert_eq!(
            results[0].id,
            Some("j-backslash".to_string()),
            "Should find ticket with '\\' in body"
        );

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_search_tickets_case_insensitive() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_search_case");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket_with_body(
            &repo_path,
            "j-case",
            "Fix Bug in Authentication",
            2,
            "The authentication module has issues.",
        );

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Search with lowercase
        let results = cache.search_tickets("fix bug").await.unwrap();
        assert_eq!(
            results.len(),
            1,
            "Lowercase search should find ticket with uppercase title"
        );
        assert_eq!(
            results[0].id,
            Some("j-case".to_string()),
            "Should find ticket with case-insensitive match"
        );

        // Search with mixed case
        let results = cache.search_tickets("AuThEnTiCaTiOn").await.unwrap();
        assert_eq!(results.len(), 1, "Mixed case search should find ticket");
        assert_eq!(
            results[0].id,
            Some("j-case".to_string()),
            "Should find ticket with case-insensitive match"
        );

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    // =========================================================================
    // Escape LIKE pattern tests
    // =========================================================================

    #[test]
    fn test_escape_like_pattern_special_characters() {
        use crate::cache::queries::escape_like_pattern;

        // Test % escaping
        assert_eq!(
            escape_like_pattern("100%"),
            "100\\%",
            "Percent sign should be escaped"
        );

        // Test _ escaping
        assert_eq!(
            escape_like_pattern("variable_name"),
            "variable\\_name",
            "Underscore should be escaped"
        );

        // Test \ escaping (backslash itself)
        assert_eq!(
            escape_like_pattern("C:\\Users"),
            "C:\\\\Users",
            "Backslash should be escaped"
        );

        // Test multiple special characters in one string
        assert_eq!(
            escape_like_pattern("100%_done\\"),
            "100\\%\\_done\\\\",
            "All special characters should be escaped"
        );
    }

    #[test]
    fn test_escape_like_pattern_regular_characters() {
        use crate::cache::queries::escape_like_pattern;

        // Test regular characters remain unchanged
        assert_eq!(
            escape_like_pattern("hello world"),
            "hello world",
            "Regular characters should not be escaped"
        );

        // Test numbers
        assert_eq!(
            escape_like_pattern("12345"),
            "12345",
            "Numbers should not be escaped"
        );

        // Test empty string
        assert_eq!(
            escape_like_pattern(""),
            "",
            "Empty string should remain empty"
        );
    }

    // =========================================================================
    // Size field tests
    // =========================================================================

    fn create_test_ticket_with_size(
        dir: &std::path::Path,
        ticket_id: &str,
        title: &str,
        size: &str,
    ) -> std::path::PathBuf {
        let tickets_dir = dir.join(".janus/items");
        fs::create_dir_all(&tickets_dir).unwrap();

        let ticket_path = tickets_dir.join(format!("{}.md", ticket_id));
        let content = format!(
            r#"---
id: {}
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
size: {}
---
# {}
"#,
            ticket_id, size, title
        );
        fs::write(&ticket_path, content).unwrap();
        ticket_path
    }

    #[tokio::test]
    #[serial]
    async fn test_cache_stores_size() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_cache_stores_size");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket_with_size(&repo_path, "j-a1b2", "Small Ticket", "small");

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Verify size was stored in database
        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query("SELECT size FROM tickets WHERE ticket_id = ?1", ["j-a1b2"])
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let size: Option<String> = row.get(0).ok();
        assert_eq!(size, Some("small".to_string()));

        // Verify size is returned when getting ticket
        let ticket = cache.get_ticket("j-a1b2").await.unwrap().unwrap();
        assert_eq!(ticket.size, Some(crate::types::TicketSize::Small));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_cache_size_null() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_cache_size_null");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create ticket without size field
        create_test_ticket(&repo_path, "j-no-size", "No Size Ticket");

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Verify size is NULL in database
        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query(
                "SELECT size FROM tickets WHERE ticket_id = ?1",
                ["j-no-size"],
            )
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let size: Option<String> = row.get(0).ok();
        assert_eq!(size, None);

        // Verify size is None when getting ticket
        let ticket = cache.get_ticket("j-no-size").await.unwrap().unwrap();
        assert_eq!(ticket.size, None);

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_cache_filter_by_size_single() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_cache_filter_by_size_single");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket_with_size(&repo_path, "j-small", "Small Ticket", "small");
        create_test_ticket_with_size(&repo_path, "j-medium", "Medium Ticket", "medium");
        create_test_ticket_with_size(&repo_path, "j-large", "Large Ticket", "large");
        create_test_ticket(&repo_path, "j-no-size", "No Size Ticket");

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Filter by single size
        let small_tickets = cache
            .get_tickets_by_size(&[crate::types::TicketSize::Small])
            .await
            .unwrap();
        assert_eq!(small_tickets.len(), 1);
        assert_eq!(small_tickets[0].id, Some("j-small".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_cache_filter_by_size_multiple() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_cache_filter_by_size_multiple");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        create_test_ticket_with_size(&repo_path, "j-xs", "XS Ticket", "xsmall");
        create_test_ticket_with_size(&repo_path, "j-small", "Small Ticket", "small");
        create_test_ticket_with_size(&repo_path, "j-medium", "Medium Ticket", "medium");
        create_test_ticket_with_size(&repo_path, "j-large", "Large Ticket", "large");
        create_test_ticket_with_size(&repo_path, "j-xl", "XL Ticket", "xlarge");

        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();

        // Filter by multiple sizes
        let tickets = cache
            .get_tickets_by_size(&[
                crate::types::TicketSize::Small,
                crate::types::TicketSize::Medium,
            ])
            .await
            .unwrap();
        assert_eq!(tickets.len(), 2);

        let ids: Vec<String> = tickets.iter().filter_map(|t| t.id.clone()).collect();
        assert!(ids.contains(&"j-small".to_string()));
        assert!(ids.contains(&"j-medium".to_string()));

        let db_path = cache.cache_db_path();
        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_cache_migration_adds_size_column() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_path = temp.path().join("test_cache_migration_adds_size_column");
        fs::create_dir_all(&repo_path).unwrap();
        std::env::set_current_dir(&repo_path).unwrap();

        // Create a ticket first
        create_test_ticket(&repo_path, "j-a1b2", "Test Ticket");

        // Open cache and sync with current version
        let cache = TicketCache::open().await.unwrap();
        cache.sync().await.unwrap();
        let db_path = cache.cache_db_path();
        drop(cache);

        // Manually set cache version to old version to simulate outdated cache
        let conn = turso::Builder::new_local(&db_path.to_string_lossy())
            .build()
            .await
            .unwrap()
            .connect()
            .unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('cache_version', '10')",
            (),
        )
        .await
        .unwrap();
        drop(conn);

        // Reopen cache - should trigger rebuild with size column
        let cache = TicketCache::open().await.unwrap();

        // After rebuild, data is lost - need to re-sync
        cache.sync().await.unwrap();

        // Verify the size column exists by querying it
        let mut rows = cache
            .create_connection()
            .await
            .unwrap()
            .query("SELECT size FROM tickets WHERE ticket_id = ?1", ["j-a1b2"])
            .await
            .unwrap();
        let row = get_first_row(&mut rows).await;
        let size: Option<String> = row.get(0).ok();
        assert_eq!(size, None); // Should be NULL (not error)

        // Verify cache version was updated
        let version = cache.get_meta("cache_version").await.unwrap();
        #[cfg(feature = "semantic-search")]
        assert_eq!(version, Some("13-semantic".to_string()));
        #[cfg(not(feature = "semantic-search"))]
        assert_eq!(version, Some("13".to_string()));

        drop(cache);
        fs::remove_file(&db_path).ok();
        fs::remove_dir_all(&repo_path).ok();
    }
}
