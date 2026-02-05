//! Path utilities for the cache module.
//!
//! This module provides functions for determining cache file locations.
//! The cache is stored locally within the `.janus` directory of each repository.

use std::path::{Path, PathBuf};

use super::database::CACHE_VERSION;
use crate::error::Result;

/// Get the path to the cache database file.
///
/// The cache is stored at `.janus/cache-v{VERSION}.db` where VERSION includes
/// the semantic search suffix when that feature is enabled. This ensures:
/// - Each repo has its own isolated cache
/// - Different feature builds don't conflict (e.g., semantic vs non-semantic)
/// - Version changes are handled gracefully without constant rebuilds
pub fn cache_db_path() -> PathBuf {
    let janus_root = PathBuf::from(".janus");

    // Attempt to migrate old cache files from the feature-flag era
    // This is best-effort; if it fails, we'll use the standard path
    let _ = migrate_old_cache_files(&janus_root);

    janus_root.join(format!("cache-v{CACHE_VERSION}.db"))
}

/// Migrate old cache files from the feature-flag era to new unified naming.
///
/// Before semantic search was always enabled, there were two cache file variants:
/// - `cache-v13.db` (non-semantic builds)
/// - `cache-v13-semantic.db` (semantic search builds)
///
/// Now that semantic search is always enabled, we use a single unified name:
/// - `cache-v13.db` (all builds)
///
/// This function renames the old semantic cache file if it exists and the new
/// file doesn't, preserving the user's cached data.
fn migrate_old_cache_files(janus_root: &Path) -> Result<()> {
    let old_path = janus_root.join(format!("cache-v{CACHE_VERSION}-semantic.db"));
    let new_path = janus_root.join(format!("cache-v{CACHE_VERSION}.db"));

    if old_path.exists() && !new_path.exists() {
        // Rename old semantic cache to new unified name
        std::fs::rename(&old_path, &new_path)?;
    }

    // Also handle WAL and SHM files
    let old_wal = janus_root.join(format!("cache-v{CACHE_VERSION}-semantic.db-wal"));
    let new_wal = janus_root.join(format!("cache-v{CACHE_VERSION}.db-wal"));
    if old_wal.exists() && !new_wal.exists() {
        std::fs::rename(&old_wal, &new_wal)?;
    }

    let old_shm = janus_root.join(format!("cache-v{CACHE_VERSION}-semantic.db-shm"));
    let new_shm = janus_root.join(format!("cache-v{CACHE_VERSION}.db-shm"));
    if old_shm.exists() && !new_shm.exists() {
        std::fs::rename(&old_shm, &new_shm)?;
    }

    Ok(())
}

/// Delete the cache database and all associated WAL/SHM files.
///
/// SQLite in WAL mode creates additional files alongside the main database:
/// - `.db-wal` - Write-ahead log file
/// - `.db-shm` - Shared memory file
///
/// This function removes all of them to ensure a clean state.
pub fn delete_cache_files(db_path: &PathBuf) -> std::io::Result<()> {
    // Delete main database file
    if db_path.exists() {
        std::fs::remove_file(db_path)?;
    }

    // Delete WAL file
    let wal_path = db_path.with_extension("db-wal");
    if wal_path.exists() {
        std::fs::remove_file(&wal_path)?;
    }

    // Delete SHM file
    let shm_path = db_path.with_extension("db-shm");
    if shm_path.exists() {
        std::fs::remove_file(&shm_path)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_cache_db_path_format() {
        let path = cache_db_path();

        // Should be in .janus directory
        assert!(path.starts_with(".janus"));

        // Should have .db extension
        assert_eq!(path.extension().unwrap().to_str().unwrap(), "db");

        // Should contain version number
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert!(filename.starts_with("cache-v"));
    }

    #[test]
    fn test_delete_cache_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");
        let wal_path = temp.path().join("test.db-wal");
        let shm_path = temp.path().join("test.db-shm");

        // Create the files
        std::fs::write(&db_path, "db").unwrap();
        std::fs::write(&wal_path, "wal").unwrap();
        std::fs::write(&shm_path, "shm").unwrap();

        assert!(db_path.exists());
        assert!(wal_path.exists());
        assert!(shm_path.exists());

        // Delete them
        delete_cache_files(&db_path).unwrap();

        assert!(!db_path.exists());
        assert!(!wal_path.exists());
        assert!(!shm_path.exists());
    }

    #[test]
    fn test_delete_cache_files_partial() {
        let temp = tempfile::TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");

        // Only create the main file, not WAL/SHM
        std::fs::write(&db_path, "db").unwrap();

        // Should succeed even if WAL/SHM don't exist
        delete_cache_files(&db_path).unwrap();

        assert!(!db_path.exists());
    }

    #[test]
    fn test_delete_cache_files_none_exist() {
        let temp = tempfile::TempDir::new().unwrap();
        let db_path = temp.path().join("nonexistent.db");

        // Should succeed even if nothing exists
        delete_cache_files(&db_path).unwrap();
    }

    #[test]
    #[serial]
    fn test_cache_migration_semantic_to_unified() {
        use std::fs;

        let temp = tempfile::TempDir::new().unwrap();
        let janus_root = temp.path();

        // Create old-style semantic cache file
        let old_db = janus_root.join(format!("cache-v{CACHE_VERSION}-semantic.db"));
        let old_wal = janus_root.join(format!("cache-v{CACHE_VERSION}-semantic.db-wal"));
        let old_shm = janus_root.join(format!("cache-v{CACHE_VERSION}-semantic.db-shm"));

        fs::write(&old_db, "db content").unwrap();
        fs::write(&old_wal, "wal content").unwrap();
        fs::write(&old_shm, "shm content").unwrap();

        // Create a new unified path for comparison
        let new_db = janus_root.join(format!("cache-v{CACHE_VERSION}.db"));
        let new_wal = janus_root.join(format!("cache-v{CACHE_VERSION}.db-wal"));
        let new_shm = janus_root.join(format!("cache-v{CACHE_VERSION}.db-shm"));

        // Ensure new paths don't exist yet
        assert!(!new_db.exists());
        assert!(!new_wal.exists());
        assert!(!new_shm.exists());

        // Run migration
        migrate_old_cache_files(janus_root).unwrap();

        // Verify old files are gone and new files exist
        assert!(!old_db.exists());
        assert!(!old_wal.exists());
        assert!(!old_shm.exists());
        assert!(new_db.exists());
        assert!(new_wal.exists());
        assert!(new_shm.exists());

        // Verify content was preserved
        assert_eq!(fs::read_to_string(&new_db).unwrap(), "db content");
        assert_eq!(fs::read_to_string(&new_wal).unwrap(), "wal content");
        assert_eq!(fs::read_to_string(&new_shm).unwrap(), "shm content");
    }

    #[test]
    #[serial]
    fn test_cache_migration_skips_if_new_exists() {
        use std::fs;

        let temp = tempfile::TempDir::new().unwrap();
        let janus_root = temp.path();

        // Create old-style semantic cache file
        let old_db = janus_root.join(format!("cache-v{CACHE_VERSION}-semantic.db"));
        fs::write(&old_db, "old content").unwrap();

        // Create new unified cache file
        let new_db = janus_root.join(format!("cache-v{CACHE_VERSION}.db"));
        fs::write(&new_db, "new content").unwrap();

        // Run migration
        migrate_old_cache_files(janus_root).unwrap();

        // Verify both files still exist (migration skipped)
        assert!(old_db.exists());
        assert!(new_db.exists());

        // Verify content unchanged
        assert_eq!(fs::read_to_string(&old_db).unwrap(), "old content");
        assert_eq!(fs::read_to_string(&new_db).unwrap(), "new content");
    }

    #[test]
    #[serial]
    fn test_cache_migration_no_old_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let janus_root = temp.path();

        // Don't create any old files
        let new_db = janus_root.join(format!("cache-v{CACHE_VERSION}.db"));

        // Run migration - should succeed without doing anything
        migrate_old_cache_files(janus_root).unwrap();

        // Verify new file wasn't created
        assert!(!new_db.exists());
    }
}
