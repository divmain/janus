# Semantic Search

Semantic search lets you find tickets by meaning rather than exact keywords. Instead of searching for "login bug", you can search for "authentication problems" and find relevant tickets even if they don't contain those exact words.

## Requirements

Semantic search is an optional feature that requires:

1. **Compile-time feature flag**: Janus must be built with `--features semantic-search`
2. **SQLite cache**: Embeddings are stored in the cache database (see [Cache Guide](cache.md))

If you installed via Homebrew, semantic search is included by default.

## Building with Semantic Search

```bash
# From source
cargo build --release --features semantic-search

# Verify the feature is enabled
janus --version
# Should show "semantic-search" in the features list
```

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

1. **Embedding generation**: When the cache syncs, each ticket's title and description are converted to a vector embedding using a local AI model (fastembed)
2. **Storage**: Embeddings are stored in the SQLite cache database
3. **Query processing**: Search queries are converted to vectors using the same model
4. **Similarity matching**: Results are ranked by cosine similarity between query and ticket embeddings

All processing happens locally - no data is sent to external services.

## Performance

- **First sync**: Generating embeddings for all tickets takes a few seconds (depends on ticket count)
- **Incremental sync**: Only new/modified tickets need embedding generation
- **Search**: Sub-second for most queries

## Troubleshooting

### "No ticket embeddings available"

Run `janus cache rebuild` to regenerate embeddings:

```bash
janus cache rebuild
```

### Semantic search not available

Verify Janus was built with the feature:

```bash
janus --version
```

If `semantic-search` is not listed, you need to rebuild:

```bash
cargo build --release --features semantic-search
```

### Cache version mismatch

Different builds use separate cache files. If you switch between builds, run:

```bash
janus cache rebuild
```

## Tips

- **Be descriptive**: Semantic search works best with natural language queries
- **Combine with fuzzy**: In the TUI, semantic results are merged with fuzzy matches for comprehensive results
- **Use thresholds**: Set `--threshold` to filter out low-confidence matches
- **Check scores**: Higher similarity scores (closer to 1.0) indicate better matches
