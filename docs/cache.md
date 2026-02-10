# Caching

Janus uses an in-memory store with content-addressable embedding caching to make common operations fast.

## Architecture

### In-Memory Store

The store is kept in-process memory using `DashMap` concurrent hash maps:

- **Store Location**: In-process memory (no database files)
- **Initialization**: On process start, all tickets and plans are read from `.janus/items/` and `.janus/plans/` into `DashMap` structures
- **Concurrency**: `DashMap` provides lock-free concurrent reads and fine-grained locking for writes
- **Filesystem Watcher**: For long-running processes (TUI, MCP server), a `notify`-based watcher monitors `.janus/` recursively, debounces events (150ms), and updates the store automatically
- **Source of truth**: Markdown files remain authoritative; the store is always derived from them

### Embedding Storage

Semantic search embeddings are stored separately from the in-memory store:

- **Location**: `.janus/embeddings/` as `.bin` files
- **Key format**: `blake3(file_path + ":" + mtime_ns)` for content-addressable cache invalidation
- **Invalidation**: When a ticket file is modified, its mtime changes, producing a new hash key, automatically invalidating stale embeddings

## Benefits

- **Fast lookups** after loading tickets into memory
- **Quick list operations** - `janus ls` completes quickly by scanning the in-memory store
- **Automatic synchronization** - filesystem watcher keeps store updated for long-running processes
- **Graceful degradation** - falls back to file reads if needed
- **Per-repo isolation** - each repository has its own `.janus/` directory

## Performance Characteristics

| Operation | Performance |
|-----------|-------------|
| Single ticket lookup | <5ms (after load) |
| List all tickets | ~25-50ms |
| TUI startup | ~25-50ms |

Performance depends on the number of tickets and system I/O speed.

## Cache Commands

```bash
# Show embedding coverage, model name, dir size
janus cache status

# Delete orphaned embedding files
janus cache prune

# Regenerate all embeddings
janus cache rebuild
```

## Concurrency

Janus is designed to handle multiple concurrent processes safely:

### Multiple Processes

You can run multiple `janus` commands simultaneously (e.g., a TUI in one terminal and CLI commands in another). Each process maintains its own in-memory store loaded from the filesystem.

### Source of Truth

The Markdown files in `.janus/` are always authoritative. The in-memory store is a derived read-replica that accelerates lookups but never contains data that isn't in the files.

### Graceful Degradation

If the store encounters issues, Janus falls back to reading directly from the filesystem. Operations always succeed; only performance may be affected.

## Semantic Search

When semantic search is enabled, embeddings are stored as `.bin` files in `.janus/embeddings/`.

### How It Works

1. Embeddings are generated for each ticket's title and description
2. Embeddings are stored as `.bin` files in `.janus/embeddings/`
3. The filename is derived from `blake3(file_path + ":" + mtime_ns)` for automatic invalidation
4. Queries are converted to vectors and compared against stored embeddings using cosine similarity
5. Orphaned embedding files can be cleaned up with `janus cache prune`

See [Semantic Search Guide](semantic-search.md) for usage details.
