# Caching

Janus uses a SQLite-based cache to make common operations dramatically faster.

## Benefits

- **~100x faster lookups** after cache warmup
- **Instant list operations** - `janus ls` completes in milliseconds instead of seconds
- **Automatic synchronization** - cache stays in sync on every command
- **Graceful degradation** - falls back to file reads if cache is unavailable
- **Per-repo isolation** - each repository has its own cache

## How It Works

1. **Cache location**: Stored outside the repository at `~/.local/share/janus/cache/<repo-hash>.db`
2. **Sync on every command**: Cache validates against filesystem and updates only changed tickets
3. **Source of truth**: Markdown files remain authoritative; cache is always derived from them
4. **Metadata only**: Cache stores YAML frontmatter; Markdown body is read on demand

## Performance Characteristics

| Operation | Without Cache | With Cache | Improvement |
|-----------|---------------|------------|-------------|
| Single ticket lookup | ~500ms | <5ms | ~100x |
| List all tickets | ~1-5s | ~25-50ms | ~100x |
| TUI startup | ~1-5s | ~25-50ms | ~100x |

The cache is particularly valuable when working with large repositories (1000+ tickets) or using the TUI frequently.

## Cache Commands

```bash
# Show cache status
janus cache

# Clear cache for current repo
janus cache clear

# Force a full cache rebuild
janus cache rebuild

# Show cache file location
janus cache path
```

## Concurrency

Janus is designed to handle multiple concurrent processes safely:

### Multiple Processes

You can run multiple `janus` commands simultaneously (e.g., a TUI in one terminal and CLI commands in another). The cache uses SQLite's WAL mode with a busy timeout, allowing processes to wait briefly for locks rather than failing immediately.

### Source of Truth

The Markdown files in `.janus/` are always authoritative. The cache is a derived read-replica that accelerates lookups but never contains data that isn't in the files.

### Graceful Degradation

If a cache operation fails due to contention, Janus falls back to reading directly from the filesystem. Operations always succeed; only performance may be affected.

### Cache Consistency

The cache may become temporarily stale if concurrent syncs conflict, but it will never be corrupted. Running any `janus` command will re-sync the cache with the current filesystem state.

### What to Expect

In typical usage (occasional concurrent commands), you won't notice any issues. In heavy concurrent scenarios (many simultaneous writes), some commands may run slower due to cache fallback, but data integrity is always maintained.
