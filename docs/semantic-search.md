# Semantic Search

Semantic search lets you find tickets by meaning rather than exact keywords. Instead of searching for "login bug", you can search for "authentication problems" and find relevant tickets even if they don't contain those exact words.

## Enabling/Disabling Semantic Search

Semantic search is enabled by default. To disable it (to avoid model downloads or embedding generation):

```bash
janus config set semantic_search.enabled false
```

To re-enable:

```bash
janus config set semantic_search.enabled true
```

When disabled, semantic search commands will show a helpful message explaining how to enable it.

## Usage

### CLI Search

Use `janus search` for semantic search from the command line:

```bash
# Basic search
janus search "authentication problems"

# Limit results
janus search "performance issues" --limit 5

# Set minimum similarity threshold (0.0-1.0)
janus search "database errors" --threshold 0.7

# Output as JSON
janus search "user login" --json
```

Example output:

```
ID          Score   Title
--------    -----   ----------------------------------------
j-a1b2      0.89    Fix OAuth token refresh
j-c3d4      0.82    Login fails after password reset
j-e5f6      0.76    Session timeout not working correctly
```

### TUI Search

In the TUI (`janus view` or `janus board`), prefix your search with `~` to use semantic search:

```
/~authentication problems
```

The search box border changes color to indicate semantic search mode.

Results are merged: fuzzy matches appear first, followed by semantic matches (deduplicated).

### MCP Tool

The MCP server exposes a `semantic_search` tool for AI assistants:

```
-> semantic_search({"query": "authentication problems", "limit": 5})
<- Returns matching tickets with similarity scores
```

See [MCP Guide](mcp.md) for integration details.

## How It Works

1. **Embedding generation**: Each ticket's title and description are converted to a vector embedding using a local AI model (fastembed). Embeddings are generated on-demand or during cache rebuild operations.
2. **Storage**: Embeddings are stored as binary files in `.janus/embeddings/`. Each embedding file is content-addressable, keyed by `blake3(file_path + ":" + mtime_ns)` for automatic cache invalidation when ticket files change.
3. **Query processing**: Search queries are converted to vectors using the same model
4. **Similarity matching**: Results are ranked by brute-force cosine similarity between query and ticket embeddings using the in-memory store

All processing happens locally - no data is sent to external services.

## Performance

- **Initial embedding generation**: Generating embeddings for all tickets takes a few seconds (depends on ticket count)
- **Incremental updates**: Only new/modified tickets need embedding generation. The store automatically detects changes via filesystem watching in long-running processes (TUI, MCP server)
- **Search**: Sub-second for most queries using brute-force cosine similarity on the in-memory store

## Troubleshooting

### "No ticket embeddings available"

Run `janus cache rebuild` to regenerate embeddings:

```bash
janus cache rebuild
```

### Orphaned embedding files

Over time, deleted tickets may leave behind orphaned embedding files. To clean up:

```bash
janus cache prune
```

This removes embedding files for tickets that no longer exist.

## Tips

- **Be descriptive**: Semantic search works best with natural language queries
- **Combine with fuzzy**: In the TUI, semantic results are merged with fuzzy matches for comprehensive results
- **Use thresholds**: Set `--threshold` to filter out low-confidence matches
- **Check scores**: Higher similarity scores (closer to 1.0) indicate better matches
