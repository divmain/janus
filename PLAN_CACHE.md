# SQLite Cache Implementation Plan

## Overview

This plan describes adding a SQLite-based caching layer to Janus that acts as a "read replica" of the `.janus/items/` directory. The Markdown files remain the source of truth. The cache provides dramatically faster lookups and list operations by syncing with filesystem changes using file modification times.

### Motivation

**Current performance characteristics (10,000 tickets):**
- Single ticket lookup: ~500ms (full directory scan on every call)
- `janus ls` / TUI startup: ~1-5s (read and parse all files)
- TUI reload: ~1-5s (even if nothing changed)

**Target performance (with cache):**
- Single ticket lookup: <5ms
- `janus ls` / TUI startup: ~25-50ms after cache warm
- TUI reload: ~25-50ms (incremental sync of changed files only)

### Key Design Decisions

1. **Cache is optional and transparent** — If cache operations fail, fall back to current file-based operations and rebuild cache on next run
2. **No directory mtime tracking** — Always perform mtime scan (~22ms for 10k files); complexity not worth minimal savings
3. **Per-repo isolation** — Each repo has its own cache database stored outside the repository
4. **Metadata-only cache** — Store YAML frontmatter fields; read full Markdown body from file on demand
5. **Sync on every command** — Cache is validated and updated on every `janus` invocation
6. **Pure Rust database** — Use Turso (SQLite rewrite in Rust) to avoid C compilation dependencies

### Database Choice: Turso

We use [Turso](https://github.com/tursodatabase/turso) (`turso` crate v0.4.1), a complete rewrite of SQLite in Rust, instead of `rusqlite` which wraps the C SQLite library. Key advantages:

- **Pure Rust** — No C compiler required, faster compilation
- **Async-native** — Built with async/await support using tokio
- **SQLite compatible** — Same SQL dialect and file format
- **Actively maintained** — By the Turso team

**Turso Limitations** (acceptable for our cache use case):
- No multi-process access (Janus CLI runs one at a time)
- No triggers/views/savepoints (not needed for cache)
- Beta status (cache can be rebuilt from source files if issues arise)

**Reference Documentation:**
- API docs: https://docs.rs/turso/0.4.1/turso/
- Crate: https://crates.io/crates/turso

---

## Cache Location & Identification

### Repository Path Hash

Repositories are identified by a base64-encoded SHA256 hash of the canonical repository path:

```rust
// Location: src/cache.rs (new file)
use sha2::{Sha256, Digest};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

pub fn repo_hash(repo_path: &Path) -> String {
    let canonical_path = repo_path
        .canonicalize()
        .unwrap_or_else(|_| repo_path.to_path_buf());
    
    let hash = Sha256::digest(canonical_path.to_string_lossy().as_bytes());
    URL_SAFE_NO_PAD.encode(&hash[..16]) // 16 bytes → 22 character string
}

// Example paths:
// /Users/dale/dev/myproject → "aB3xY9..."
// /home/sarah/projects/api   → "kL7mN2..."
```

### Cache Directory Structure

```
~/.local/share/janus/
└── cache/
    ├── aB3xY9zK1mP2qR4sT.db      # Cache for repo #1
    ├── kL7mN2oP8qR1sT3uV.db      # Cache for repo #2
    └── ...                       # One file per repo
```

**Implementation detail:** Use `directories` crate for cross-platform path resolution:

```rust
use directories::ProjectDirs;

pub fn cache_dir() -> PathBuf {
    let proj_dirs = ProjectDirs::from("com", "divmain", "janus")
        .expect("cannot determine cache directory");
    proj_dirs.cache_dir().to_path_buf()
}

pub fn cache_db_path(repo_hash: &str) -> PathBuf {
    cache_dir().join(format!("{}.db", repo_hash))
}
```

---

## SQLite Schema

### Tables

```sql
-- Metadata table for cache info
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Store repo path for debugging (see: janus cache path)
INSERT OR REPLACE INTO meta (key, value) VALUES ('repo_path', ?);
INSERT OR REPLACE INTO meta (key, value) VALUES ('cache_version', '1');

-- Main ticket cache table
CREATE TABLE IF NOT EXISTS tickets (
    ticket_id TEXT PRIMARY KEY,
    mtime_ns INTEGER NOT NULL,           -- nanoseconds since Unix epoch
    
    -- Cached YAML frontmatter fields
    status TEXT,                          -- "new", "next", "in_progress", etc.
    title TEXT,                           -- ticket title (from Markdown body)
    priority INTEGER,                     -- 0-4 (P0-P4)
    ticket_type TEXT,                     -- "bug", "feature", "task", "epic", "chore"
    assignee TEXT,                        
    deps TEXT,                            -- JSON array of ticket IDs
    links TEXT,                           -- JSON array of ticket IDs
    parent TEXT,                          
    created TEXT,                         -- ISO 8601 datetime string
    external_ref TEXT,
    remote TEXT
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_tickets_status ON tickets(status);
CREATE INDEX IF NOT EXISTS idx_tickets_priority ON tickets(priority);
CREATE INDEX IF NOT EXISTS idx_tickets_type ON tickets(ticket_type);
```

### Data Types

| SQLite Type | Rust Storage | Notes |
|-------------|--------------|-------|
| TEXT | String | ticket_id, status, title, ticket_type, assignee, parent, created, external_ref, remote |
| INTEGER | i64 | mtime_ns (nanoseconds), priority |
| TEXT (JSON) | Vec\<String\> | deps, links (serialize with serde_json) |

---

## Sync Algorithm

### Overview

On every command invocation, the cache validates its state against the filesystem and updates only what changed.

### Detailed Steps

```
1. Scan .janus/items/ directory
   - Use fs::read_dir() to get all entries
   - Filter for .md files only
   - For each file: fs::metadata() to get mtime
   - Build HashMap<String, SystemTime>: key=filename, value=mtime
   - Cost: ~22ms for 10,000 files on SSD

2. Query cached data
   - SELECT ticket_id, mtime_ns FROM tickets
   - Build HashMap<String, i64>: key=ticket_id, value=cached_mtime_ns
   - Cost: ~1ms for 10,000 rows

3. Compute diff
   Let disk_files = Map from step 1
   Let cache_files = Map from step 2
   
   added_tickets = disk_files.keys() - cache_files.keys()
   removed_tickets = cache_files.keys() - disk_files.keys()
   common_tickets = disk_files.keys() ∩ cache_files.keys()
   
   For ticket in common_tickets:
       if disk_mtime(ticket) != cache_mtime(ticket):
           modified_tickets.push(ticket)

4. Process changes
   - For each ticket in added_tickets ∪ modified_tickets:
     a. Read ticket file from disk
     b. Parse YAML frontmatter + extract title
     c. INSERT OR REPLACE into tickets table
   
   - For each ticket in removed_tickets:
     DELETE FROM tickets WHERE ticket_id = ?

5. All SQL operations in a single transaction for atomicity

6. Return cached ticket data
```

### Transaction Structure (Turso Async)

```rust
// Using Turso async API (requires &mut conn for transaction())
let tx = conn.transaction().await?;

for id in &added_tickets {
    let (metadata, mtime_ns) = read_and_parse_ticket(id)?;
    tx.execute(
        "INSERT OR REPLACE INTO tickets (ticket_id, mtime_ns, status, title, ...)
         VALUES (?1, ?2, ?3, ?4, ...)",
        turso::params![id, mtime_ns, metadata.status, metadata.title, ...]
    ).await?;
}

for id in &removed_tickets {
    tx.execute("DELETE FROM tickets WHERE ticket_id = ?", [id]).await?;
}

tx.commit().await?;
```

**Note:** `Transaction` derefs to `Connection`, so you can call `execute()`, `query()`, etc. directly on it.

---

## New Module: `src/cache.rs`

### Public API (Async)

```rust
use turso::{Builder, Connection, Database};

/// Ticket cache backed by SQLite (Turso)
pub struct TicketCache {
    db: Database,
    conn: Connection,
    repo_path: PathBuf,      // Absolute path to repository root
    repo_hash: String,       // For cache file naming
}

impl TicketCache {
    /// Open or create cache for repository at current working directory
    ///
    /// Returns error if cache cannot be opened/created (but continues gracefully)
    pub async fn open() -> Result<Self>;

    /// Sync cache with filesystem, returns true if any changes were detected
    ///
    /// Always called on every command to ensure cache consistency
    pub async fn sync(&mut self) -> Result<bool>;

    /// Get all tickets from cache (fast, no disk I/O)
    pub async fn get_all_tickets(&self) -> Result<Vec<TicketMetadata>>;

    /// Get a single ticket by exact ID
    pub async fn get_ticket(&self, id: &str) -> Result<Option<TicketMetadata>>;

    /// Find all ticket IDs matching a partial ID (for command-line tab completion)
    pub async fn find_by_partial_id(&self, partial: &str) -> Result<Vec<String>>;

    /// Build ticket map from cache (for dependency resolution, blocker calculation, etc.)
    pub async fn build_ticket_map(&self) -> Result<HashMap<String, TicketMetadata>>;

    /// Get cached directory path (for `janus cache path` command)
    pub fn cache_db_path(&self) -> PathBuf;
}

// Private helper functions
impl TicketCache {
    async fn initialize_database(&self) -> Result<()>;
    fn scan_directory(&self) -> Result<HashMap<String, SystemTime>>;
    fn read_and_parse_ticket(&self, id: &str) -> Result<(TicketMetadata, i64)>;
    fn serialize_array(&self, arr: &[String]) -> Result<String>;        // Vec -> JSON
    fn deserialize_array(&self, s: &str) -> Result<Vec<String>>;        // JSON -> Vec
}

/// Error type wrapper for cache-specific errors
#[derive(Error, Debug)]
pub enum CacheError {
    #[error("cache database corrupted: {0}")]
    Corrupted(String),

    #[error("cache database version mismatch: expected {expected}, found {found}")]
    VersionMismatch { expected: String, found: String },

    #[error("cannot access cache directory: {0}")]
    AccessDenied(PathBuf),

    #[error("database error: {0}")]
    Database(#[from] turso::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    SerdeJson(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CacheError>;
```

### Error Handling & Graceful Degradation

```rust
use tokio::sync::OnceCell;

// Global cache instance pattern (async initialization)
static GLOBAL_CACHE: OnceCell<Option<TicketCache>> = OnceCell::const_new();

async fn get_or_init_cache() -> Option<&'static TicketCache> {
    GLOBAL_CACHE.get_or_init(|| async {
        match TicketCache::open().await {
            Ok(mut cache) => {
                // Warn but don't fail if sync fails
                if let Err(e) = cache.sync().await {
                    eprintln!("Warning: cache sync failed: {}. Falling back to file reads.", e);
                    None
                } else {
                    Some(cache)
                }
            }
            Err(e) => {
                eprintln!("Warning: failed to open cache: {}. Falling back to file reads.", e);
                None
            }
        }
    }).await.as_ref()
}
```

**Fallback behavior:**
- If cache cannot be opened/initialized → use existing file-based operations
- If cache sync fails → log warning, use existing file-based operations
- If database is corrupted → delete file, reinitialize on next command

---

## Integration Points

### Modify `src/ticket.rs`

| Current Function | Change Required | Implementation |
|------------------|-----------------|----------------|
| `find_ticket_by_id()` | Use cache for partial ID matching | Query cache for `LIKE` pattern or scan in memory |
| `get_all_tickets()` | Use cache instead of file reads | `cache.get_all_tickets()` or fallback to file reads |
| `build_ticket_map()` | Use cache instead of file reads | `cache.build_ticket_map()` or fallback to file reads |
| `Ticket::find()` | Use cache for path resolution | Query cache for exact/partial match, return PathBuf |

**Example refactoring (async):**

```rust
// src/ticket.rs

// NEW: Try cache first, fall back to file scan
async fn find_ticket_by_id(partial_id: &str) -> Result<PathBuf> {
    // Try cache first
    if let Some(cache) = get_or_init_cache().await {
        // Exact match
        let exact_name = format!("{}.md", partial_id);
        let exact_match = PathBuf::from(TICKETS_ITEMS_DIR).join(&exact_name);
        if exact_match.exists() {
            return Ok(exact_match);
        }

        // Partial match via cache
        if let Ok(matches) = cache.find_by_partial_id(partial_id).await {
            match matches.len() {
                1 => {
                    let filename = format!("{}.md", &matches[0]);
                    return Ok(PathBuf::from(TICKETS_ITEMS_DIR).join(filename));
                }
                0 => { /* continue to filesystem scan */ }
                _ => return Err(JanusError::AmbiguousId(partial_id.to_string())),
            }
        }
    }

    // FALLBACK: Original file-based implementation
    let files = find_tickets();
    // ... existing logic ...
}

// NEW: Use cache for get_all_tickets
pub async fn get_all_tickets() -> Vec<TicketMetadata> {
    if let Some(cache) = get_or_init_cache().await {
        if let Ok(tickets) = cache.get_all_tickets().await {
            return tickets;
        }
        eprintln!("Warning: cache read failed, falling back to file reads");
    }

    // FALLBACK: Original implementation
    // ... existing logic ...
}

// NEW: Use cache for build_ticket_map
pub async fn build_ticket_map() -> HashMap<String, TicketMetadata> {
    if let Some(cache) = get_or_init_cache().await {
        if let Ok(map) = cache.build_ticket_map().await {
            return map;
        }
        eprintln!("Warning: cache read failed, falling back to file reads");
    }

    // FALLBACK: Original implementation
    // ... existing logic ...
}
```

---

## TUI Integration

### Modify `src/tui/state.rs`

```rust
use crate::cache::get_or_init_cache;

impl TuiState {
    pub async fn init() -> (Self, InitResult) {
        if !janus_dir_exists() {
            return (Self { all_tickets: vec![], .. }, InitResult::NoJanusDir);
        }

        // Load from cache first
        let all_tickets = if let Some(cache) = get_or_init_cache().await {
            match cache.get_all_tickets().await {
                Ok(tickets) => tickets,
                Err(e) => {
                    eprintln!("Warning: failed to load from cache: {}. Using file reads.", e);
                    crate::ticket::get_all_tickets_sync()  // Fallback
                }
            }
        } else {
            crate::ticket::get_all_tickets_sync()  // No cache, use file reads
        };

        let result = if all_tickets.is_empty() {
            InitResult::EmptyDir
        } else {
            InitResult::Ok
        };

        (
            Self {
                all_tickets,
                init_error: None,
            },
            result,
        )
    }

    /// Reload tickets (called when TUI refreshes)
    pub async fn reload(&mut self) {
        // Sync cache (incremental update)
        if let Some(cache) = get_or_init_cache().await {
            // Cache sync happens automatically on cache operations
            match cache.get_all_tickets().await {
                Ok(tickets) => {
                    self.all_tickets = tickets;
                    return;
                }
                Err(e) => {
                    eprintln!("Warning: cache reload failed: {}. Using file reads.", e);
                }
            }
        }

        // FALLBACK: Original implementation
        self.all_tickets = crate::ticket::get_all_tickets_sync();
    }
}
```

---

## New CLI Commands: `janus cache`

### Command Structure

```
janus cache          # Show cache status
janus cache clear    # Clear (delete) cache for current repo
janus cache rebuild  # Force full cache rebuild
janus cache path     # Print path to cache DB file
```

### Implementation: `src/commands/cache.rs`

```rust
use crate::cache::{TicketCache, Result as CacheResult};
use crate::error::Result;

/// Show cache status
pub async fn cmd_status() -> Result<()> {
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
                println!("  Last modified: {:?}", meta.modified().ok());
            }
        }
        Err(e) => {
            println!("Cache not available: {}", e);
            println!("Run 'janus cache rebuild' to create a cache.");
        }
    }
    Ok(())
}

/// Clear cache for current repo
pub async fn cmd_clear() -> Result<()> {
    let cache = TicketCache::open().await?;
    let db_path = cache.cache_db_path();
    
    println!("Deleting cache database: {}", db_path.display());
    fs::remove_file(&db_path)?;
    println!("Cache cleared successfully.");
    
    println!("\nNote: The cache will be rebuilt automatically on the next janus command.");
    Ok(())
}

/// Force full cache rebuild
pub async fn cmd_rebuild() -> Result<()> {
    println!("Rebuilding cache...");
    
    // Delete existing cache
    let db_path = TicketCache::open().await?.cache_db_path();
    if db_path.exists() {
        fs::remove_file(&db_path)?;
    }
    
    // Force re-sync (cache will be rebuilt from scratch)
    let mut cache = TicketCache::open().await?;
    let start = std::time::Instant::now();
    let _changed = cache.sync().await?;
    let duration = start.elapsed();
    
    let ticket_count = cache.get_all_tickets().await.unwrap_or_default().len();
    
    println!("Cache rebuilt successfully:");
    println!("  Tickets cached: {}", ticket_count);
    println!("  Time taken: {:?}", duration);
    Ok(())
}

/// Print cache DB path
pub async fn cmd_path() -> Result<()> {
    let cache = TicketCache::open().await?;
    println!("{}", cache.cache_db_path().display());
    Ok(())
}
```

### Add to `src/main.rs`

```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing commands ...
    
    /// Cache management
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
}

#[derive(Subcommand)]
enum CacheAction {
    /// Show cache status
    Status,
    /// Clear cache for current repo
    Clear,
    /// Force full cache rebuild
    Rebuild,
    /// Print path to cache database
    Path,
}

// In main():
    Commands::Cache { action } => match action {
        CacheAction::Status => commands::cache::cmd_status().await?,
        CacheAction::Clear => commands::cache::cmd_clear().await?,
        CacheAction::Rebuild => commands::cache::cmd_rebuild().await?,
        CacheAction::Path => commands::cache::cmd_path().await?,
    },
```

---

## Dependencies

### Current State (rusqlite - to be replaced)

```toml
# Currently in Cargo.toml (to be replaced)
rusqlite = { version = "0.31", features = ["bundled"] }
base64 = "0.22"
directories = "5"
once_cell = "1.19"
```

### Target State (turso)

```toml
[dependencies]
# ... existing dependencies ...

# SQLite cache (pure Rust implementation)
turso = "0.4"

# Already present for other features
# tokio = { version = "1", features = ["rt-multi-thread", "macros"] }

# Utility crates
base64 = "0.22"
directories = "5"
```

**Notes:**
- `turso`: Pure Rust SQLite implementation (v0.4.1), no C compiler required
- `tokio`: Already present in the project (v1 with rt-multi-thread, macros) for GitHub/Linear API clients
- `directories`: Cross-platform path resolution (`~/.local/share` on Linux, `~/Library` on macOS, etc.)

**Removed in migration:**
- `rusqlite`: Replaced by `turso`
- `once_cell`: Use `tokio::sync::OnceCell` instead for async initialization
- `futures-util`: Not needed - Turso's `Rows` has built-in async iteration via `next().await`

---

## Files to Create/Modify

### Current State

Files already created:

| File | Status | Notes |
|------|--------|-------|
| `src/cache.rs` | ✅ Created | ~718 lines, uses rusqlite (sync), Phase 1 & 2 complete, 13 tests pass |
| `src/cache_error.rs` | ✅ Created | ~28 lines, uses `rusqlite::Error` |
| `src/lib.rs` | ✅ Modified | Has `pub mod cache;` and `pub mod cache_error;` |

### Files to Create

| File | Purpose |
|------|---------|
| `src/commands/cache.rs` | `janus cache` subcommand implementations (Phase 5) |

### Files to Modify

| File | Changes |
|------|---------|
| `Cargo.toml` | Replace `rusqlite` with `turso = "0.4"`, remove `once_cell` |
| `src/cache_error.rs` | Replace `rusqlite::Error` with `turso::Error` |
| `src/cache.rs` | Rewrite for async using Turso API |
| `src/ticket.rs` | Modify `find_ticket_by_id()`, `get_all_tickets()`, `build_ticket_map()` to use cache (Phase 4) |
| `src/tui/state.rs` | Modify `TuiState::new()`, `TuiState::init()`, `TuiState::reload()` to use cache (Phase 4) |
| `src/commands/mod.rs` | Add `pub mod cache;` export (Phase 5) |
| `src/main.rs` | Add CLI commands for cache management, add `#[tokio::main]` if not present (Phase 5) |
| `tests/integration_test.rs` | Add cache-related integration tests (Phase 7) |

---

## Testing Strategy

### Current Unit Tests (`src/cache.rs`) - 13 tests using rusqlite (sync)

The following tests exist and pass with the current rusqlite implementation. They will be converted to async (`#[tokio::test]`) in Phase 2b:

```rust
#[cfg(test)]
mod tests {
    // Phase 1 tests (infrastructure)
    #[test] fn test_repo_hash_consistency()        // ✅ Verifies hash is deterministic and 22 chars
    #[test] fn test_cache_dir_creates_directory()  // ✅ Verifies cache dir exists
    #[test] fn test_cache_db_path_format()         // ✅ Verifies .db extension
    #[test] #[serial] fn test_cache_initialization()     // ✅ Verifies DB file created
    #[test] #[serial] fn test_database_tables_created()  // ✅ Verifies meta & tickets tables exist
    #[test] #[serial] fn test_repo_path_stored_in_meta() // ✅ Verifies repo path in meta table
    #[test] #[serial] fn test_cache_version_stored_in_meta() // ✅ Verifies version in meta table

    // Phase 2 tests (sync algorithm)
    #[test] #[serial] fn test_sync_creates_entries()       // ✅ Creates 3 tickets, verifies count
    #[test] #[serial] fn test_sync_detects_additions()     // ✅ Adds ticket, verifies detected
    #[test] #[serial] fn test_sync_detects_deletions()     // ✅ Deletes ticket, verifies detected
    #[test] #[serial] fn test_sync_detects_modifications() // ✅ Modifies title, verifies updated
    #[test] #[serial] fn test_serialize_deserialize_arrays() // ✅ JSON roundtrip for deps/links
    #[test] #[serial] fn test_scan_directory()             // ✅ Scans .md files, ignores .txt
}
```

### Phase 2b: Convert tests to async

All 13 tests will be converted to use `#[tokio::test]` and async/await:

```rust
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
```

### Phase 3: Add query operation tests

```rust
#[tokio::test]
#[serial]
async fn test_get_all_tickets() { ... }

#[tokio::test]
#[serial]
async fn test_get_ticket() { ... }

#[tokio::test]
#[serial]
async fn test_find_by_partial_id() { ... }

#[tokio::test]
#[serial]
async fn test_find_by_partial_id_ambiguous() { ... }

#[tokio::test]
#[serial]
async fn test_build_ticket_map() { ... }
```

### Integration Tests (`tests/integration_test.rs`)

Add new integration tests:

```rust
#[test]
fn test_cache_basic_workflow() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let janus_dir = temp_dir.path().join(".janus/items");
    fs::create_dir_all(&janus_dir).unwrap();

    // Create test tickets
    create_ticket(&janus_dir, "j-a1b2", "Bug fix");
    create_ticket(&janus_dir, "j-c3d4", "Feature add");

    // First command: builds cache
    let output = run_janus_with_dir(&temp_dir.path(), ["ls"]);
    assert!(output.contains("j-a1b2"));
    assert!(output.contains("j-c3d4"));

    // Second command: uses cache (should be faster, but hard to assert)
    let output = run_janus_with_dir(&temp_dir.path(), ["ls"]);
    assert!(output.contains("j-a1b2"));

    // Modify a ticket
    let ticket_path = janus_dir.join("j-a1b2.md");
    let content = fs::read_to_string(&ticket_path).unwrap();
    let new_content = content.replace("Bug fix", "Critical bug");
    fs::write(&ticket_path, new_content).unwrap();

    // Cache should detect change and update
    let output = run_janus_with_dir(&temp_dir.path(), ["show", "j-a1b2"]);
    assert!(output.contains("Critical bug"));

    // Add new ticket
    create_ticket(&janus_dir, "j-e5f6", "New task");
    let output = run_janus_with_dir(&temp_dir.path(), ["ls"]);
    assert!(output.contains("j-e5f6"));

    // Delete ticket
    fs::remove_file(&janus_dir.join("j-c3d4.md")).unwrap();
    let output = run_janus_with_dir(&temp_dir.path(), ["ls"]);
    assert!(!output.contains("j-c3d4"));
}

#[test]
fn test_cache_clear_command() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    setup_janus_repo(&temp_dir.path());
    create_ticket(&temp_dir.path().join(".janus/items"), "j-test", "Test");

    // Build cache
    run_janus_with_dir(&temp_dir.path(), ["ls"]);

    // Clear cache
    let output = run_janus_with_dir(&temp_dir.path(), ["cache", "clear"]);
    assert!(output.contains("Cache cleared"));

    // Still works (rebuilds automatically)
    let output = run_janus_with_dir(&temp_dir.path(), ["ls"]);
    assert!(output.contains("j-test"));
}

#[test]
fn test_cache_rebuild_command() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    setup_janus_repo(&temp_dir.path());
    create_ticket(&temp_dir.path().join(".janus/items"), "j-test", "Test");

    // Build cache
    run_janus_with_dir(&temp_dir.path(), ["ls"]);

    // Rebuild cache
    let output = run_janus_with_dir(&temp_dir.path(), ["cache", "rebuild"]);
    assert!(output.contains("Cache rebuilt"));

    // Verify it works
    let output = run_janus_with_dir(&temp_dir.path(), ["ls"]);
    assert!(output.contains("j-test"));
}

#[test]
fn test_cache_path_command() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    setup_janus_repo(&temp_dir.path());

    let output = run_janus_with_dir(&temp_dir.path(), ["cache", "path"]);
    let path_str = output.trim();
    let cache_path = PathBuf::from(path_str);

    assert!(cache_path.is_absolute());
    assert!(cache_path.to_string_lossy().contains("janus"));
    assert!(cache_path.extension().map(|ext| ext == "db").unwrap_or(false));
}

#[test]
fn test_cache_corrupted_handle_gracefully() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    setup_janus_repo(&temp_dir.path());
    create_ticket(&temp_dir.path().join(".janus/items"), "j-test", "Test");

    // Build cache
    run_janus_with_dir(&temp_dir.path(), ["ls"]);

    // Corrupt database
    let repo_hash = repo_hash(temp_dir.path());
    let cache_dir = cache_dir();
    let cache_path = cache_dir.join(format!("{}.db", repo_hash));
    fs::write(&cache_path, b"corrupt data").unwrap();

    // Should warn but still work
    let output = run_janus_with_dir(&temp_dir.path(), ["ls"]);
    assert!(output.contains("Warning") || output.contains("j-test"));  // Either warns or falls back
}
```

---

## Performance Benchmarks

### Expected Performance (10,000 tickets on SSD)

| Operation | Before | After |
|-----------|--------|-------|
| First run (cold cache, all reads) | ~1-5s | ~1-5s (same, builds cache) |
| Single ticket lookup | ~500ms | <5ms |
| `janus ls` (cache warm) | ~1-5s | ~25-50ms |
| TUI startup (cache warm) | ~1-5s | ~25-50ms |
| TUI reload (5 files changed) | ~1-5s | ~30-40ms (22ms scan + read 5 files) |
| `janus cache rebuild` | n/a | ~1-5s (same as cold start) |

### Cost Breakdown (cache warm, no changes)

| Step | Time |
|------|------|
| Directory scan + stat | ~22ms |
| SQLite query for mtimes | ~1ms |
| Diff computation | ~1ms |
| File reads (none changed) | ~0ms |
| SQLite query for ticket data | ~1ms |
| **Total** | **~25ms** |

### Cost Breakdown (cache warm, 10 files changed)

| Step | Time |
|------|------|
| Directory scan + stat | ~22ms |
| SQLite query for mtimes | ~1ms |
| Diff computation | ~1ms |
| File reads + parse (10 files) | ~10-20ms |
| SQLite upserts | ~1ms |
| SQLite query for ticket data | ~1ms |
| **Total** | **~35-45ms** |

---

## Implementation Phases

### Phase 1: Infrastructure (Foundation) ✅ COMPLETED

1. ✅ Add dependencies to `Cargo.toml` (rusqlite, base64, directories, once_cell)
2. ✅ Create `src/cache.rs` with basic structure
3. ✅ Implement `repo_hash()` and `cache_dir()` helper functions
4. ✅ Implement database initialization in `TicketCache::open()`
5. ✅ Create SQLite schema (meta table, tickets table, indexes)
6. ✅ Create `src/cache_error.rs` with CacheError enum

**Testing:** ✅ 6 unit tests pass (repo_hash, cache_dir, cache_db_path, cache_initialization, database_tables_created, repo_path_stored_in_meta, cache_version_stored_in_meta)

---

### Phase 2: Sync Algorithm (Core Logic) ✅ COMPLETED (with rusqlite)

1. ✅ Implement `scan_directory()` - scans `.janus/items/` for `.md` files and their mtimes
2. ✅ Implement `get_cached_mtimes()` - queries SQLite for cached ticket mtimes
3. ✅ Implement `sync()` - computes diff (added/modified/removed tickets), updates cache in transaction
4. ✅ Implement `read_and_parse_ticket()` - reads ticket file and parses YAML frontmatter
5. ✅ Implement `insert_or_replace_ticket()` - upserts ticket metadata into SQLite
6. ✅ Implement `serialize_array()` - converts Vec<String> to JSON for deps/links fields

**Testing:** ✅ 7 unit tests pass (sync_creates_entries, sync_detects_additions, sync_detects_deletions, sync_detects_modifications, serialize_deserialize_arrays, scan_directory)

**Current state:** 
- `src/cache.rs`: ~718 lines including tests, uses synchronous rusqlite API
- `src/cache_error.rs`: ~28 lines, uses `rusqlite::Error`
- All 13 unit tests pass with rusqlite
- Blocked by rusqlite C compilation issues during `cargo clippy` (hangs indefinitely)

**Note:** `CacheRow` struct exists but is unused - will be removed or used in Phase 3.

---

### Phase 2b: Migrate to Turso Database

Replace `rusqlite` with `turso` to eliminate C compilation requirements.

1. **Update `Cargo.toml`:**
   - Remove `rusqlite = { version = "0.31", features = ["bundled"] }`
   - Remove `once_cell = "1.19"`
   - Add `turso = "0.4"`

2. **Update `src/cache_error.rs`:**
   - Replace `rusqlite::Error` with `turso::Error`

3. **Rewrite `src/cache.rs` for async:**
   - Change `TicketCache` struct to hold `turso::Database` and `turso::Connection`
   - Convert all methods to `async fn`
   - Update SQL execution to use Turso's async API
   - Update transaction handling to use `conn.transaction().await`
   - Remove `CacheRow` struct (currently unused, will implement in Phase 3)

4. **Update tests to use `#[tokio::test]`:**
   - Add `async` to test functions
   - Add `.await` to all cache operations

**Key API differences (rusqlite → turso):**

| rusqlite | turso |
|----------|-------|
| `Connection::open(path)` | `Builder::new_local(path).build().await` + `db.connect()` |
| `conn.execute(sql, params![...])` | `conn.execute(sql, params).await` |
| `conn.execute(sql, [])` | `conn.execute(sql, ()).await` |
| `conn.prepare(sql)?.query_map([], \|row\| ...)` | `conn.query(sql, ()).await` + `rows.next().await` |
| `row.get::<_, String>(idx)` | `row.get::<String>(idx)` or `row.get_value(idx)` |
| `row.get::<_, i64>(idx)` | `row.get::<i64>(idx)` |
| `conn.execute("BEGIN TRANSACTION", [])` | `conn.transaction().await` (returns `Transaction`) |
| `conn.execute("COMMIT", [])` | `tx.commit().await` |
| `conn.prepare_cached(sql)` | `conn.prepare(sql).await` (no caching needed) |
| Sync | Async (all operations) |

**Turso API patterns (from docs.rs/turso/0.4.1):**

```rust
// Opening database
let db = Builder::new_local("path.db").build().await?;
let conn = db.connect()?;  // Note: connect() is sync!

// Execute (returns rows affected)
conn.execute("INSERT INTO ...", ["param1", "param2"]).await?;
conn.execute("CREATE TABLE ...", ()).await?;  // No params = ()

// Query with iteration
let mut rows = conn.query("SELECT * FROM tickets", ()).await?;
while let Some(row) = rows.next().await? {
    let id: String = row.get(0)?;          // Type-safe get
    let value = row.get_value(0)?;         // Returns Value enum
}

// Transactions (requires &mut Connection)
let tx = conn.transaction().await?;
tx.execute("INSERT ...", params).await?;
tx.execute("DELETE ...", params).await?;
tx.commit().await?;  // or tx.rollback().await?
```

**Testing:** Verify all existing 13 unit tests pass with Turso

---

### Phase 2c: Verify Turso Migration

1. Run `cargo check` to verify compilation without C toolchain issues
2. Run `cargo test cache --lib` to verify all 13 cache tests pass
3. Run `cargo clippy` to verify no warnings/issues
4. Manually test basic cache operations

**Testing:** Full test suite passes, no C compiler required

---

### Phase 3: Query Operations

1. Implement `get_all_tickets()`
2. Implement `get_ticket()`
3. Implement `find_by_partial_id()`
4. Implement `build_ticket_map()`
5. Helper function `deserialize_array()` for JSON → Vec

**Turso-specific implementation:**

```rust
pub async fn get_all_tickets(&self) -> CacheResult<Vec<TicketMetadata>> {
    let mut rows = self.conn.query(
        "SELECT ticket_id, status, title, priority, ticket_type, assignee,
                deps, links, parent, created, external_ref, remote
         FROM tickets",
        ()
    ).await?;

    let mut tickets = Vec::new();
    while let Some(row) = rows.next().await? {
        let metadata = self.row_to_metadata(&row)?;
        tickets.push(metadata);
    }
    Ok(tickets)
}

fn row_to_metadata(&self, row: &turso::Row) -> CacheResult<TicketMetadata> {
    Ok(TicketMetadata {
        id: row.get::<Option<String>>(0)?,
        status: row.get::<Option<String>>(1)?
            .and_then(|s| s.parse().ok()),
        title: row.get::<Option<String>>(2)?,
        priority: row.get::<Option<i64>>(3)?
            .and_then(|n| (n as u8).try_into().ok()),
        // ... etc
    })
}
```

**Testing:** Unit tests for all query operations

---

### Phase 4: Integration with Existing Code

1. Modify `src/ticket.rs` functions to use cache with fallbacks
2. Modify `src/tui/state.rs` to use cache
3. Test TUI startup and reload performance
4. Maintain sync fallbacks for graceful degradation

**Note:** Some existing sync functions may need async variants or tokio block_on wrappers.

**Testing:** Manual testing, verify CLI commands still work

---

### Phase 5: Cache Management Commands

1. Implement `src/commands/cache.rs` (async)
2. Add commands to `src/main.rs`
3. Test `status`, `clear`, `rebuild`, `path` commands

**Testing:** Integration tests for cache commands

---

### Phase 6: Error Handling & Edge Cases

1. Graceful degradation when cache unavailable
2. Handle corrupted databases (delete and rebuild)
3. Handle permission errors
4. Add appropriate warning messages

**Testing:** Integration tests for error cases

---

### Phase 7: Testing & Documentation

1. Complete unit test coverage
2. Complete integration test coverage
3. Performance benchmarking
4. Update AGENTS.md to reflect caching behavior
5. Consider adding "How Caching Works" section to README

---

## Open Questions / Decisions Needed

1. **TUI reload frequency:** How often does TUI currently call `reload()`? This affects the value of incremental sync.

2. **Body content caching:** For now, we're not caching the Markdown body (just frontmatter + title). If future features like full-text search are needed, body caching can be added as a later enhancement.

3. **Cache warming strategy:** Currently, `janus` commands always sync cache, even read-only ones like `janus help`. This keeps cache fresh at minimal cost (`~25ms`). Alternative: `janus --no-cache` flag to explicitly skip cache (useful for debugging).

4. **Parallel file reads:** Not included in initial implementation. Can be added later with `rayon` for faster initial cache build / full rebuild operations.

5. **Turso stability:** Turso is in beta. If we encounter issues, we have options:
   - Fall back gracefully to file-based operations
   - Cache can always be deleted and rebuilt
   - Consider revisiting rusqlite with system SQLite (no bundled) if Turso proves problematic

---

## Future Enhancements (Out of Scope for Initial Implementation)

1. **Parallel file reads with `rayon`** — Faster cold-start cache building

2. **Full-text search** — Cache ticket bodies, add FTS support

3. **Incremental TUI sync** — Background task maintains cache while TUI runs, updates reactively

4. **Memory-mapped I/O** — For very large ticket files

5. **Cache compression** — Compress ticket data in SQLite for smaller disk footprint

6. **Smart preloading** — Load frequently-accessed tickets into memory for even faster operations

7. **Cache metrics** — Track hit rates, sync times, query patterns for optimization guidance

---

## Summary

This implementation plan adds a SQLite-based caching layer to Janus that:

- **Respects the source of truth:** Markdown files remain authoritative
- **Is transparent and safe:** Gracefully degrades to file reads if cache fails
- **Provides significant performance gains:** ~100x faster for common operations
- **Is easy to manage:** Simple CLI commands for cache diagnostics and rebuild
- **Is well-tested:** Comprehensive unit and integration test coverage
- **Uses pure Rust:** Turso database eliminates C compilation dependencies

The cache lives outside the repository (`~/.local/share/janus/cache/`) so it doesn't pollute version control. Each repository has its own cache database identified by a hash of its canonical path.

The sync algorithm is simple and efficient:
1. Scan directory for mtimes (~22ms for 10k files)
2. Query cached mtimes (~1ms)
3. Re-read only files that changed
4. Return data from cache

This approach maintains the simplicity of plain-text issue tracking while providing the performance characteristics needed for scaling to thousands of tickets.

---

## Current Implementation Status

| Phase | Status | Notes |
|-------|--------|-------|
| Phase 1: Infrastructure | ✅ Complete | rusqlite impl, 7 tests pass |
| Phase 2: Sync Algorithm | ✅ Complete | rusqlite impl, 6 tests pass (13 total) |
| Phase 2b: Turso Migration | ⏳ Next | Replace rusqlite with turso, convert to async |
| Phase 2c: Verify Migration | ⏳ Pending | Verify tests pass, no C toolchain needed |
| Phase 3: Query Operations | ⏳ Pending | get_all_tickets, find_by_partial_id, etc. |
| Phase 4: Integration | ⏳ Pending | Wire cache into ticket.rs, tui/state.rs |
| Phase 5: CLI Commands | ⏳ Pending | janus cache status/clear/rebuild/path |
| Phase 6: Error Handling | ⏳ Pending | Graceful degradation, corruption handling |
| Phase 7: Testing & Docs | ⏳ Pending | Integration tests, documentation |
