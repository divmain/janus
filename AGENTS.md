# AGENTS.md - Janus Project Guidelines

This document provides essential information for AI coding agents working in this repository.

## Project Overview

Janus is a plain-text issue tracking CLI tool written in Rust. It stores tickets as Markdown files with YAML frontmatter in a `.janus` directory. The project provides commands for creating, managing, and querying tickets, with optional sync to GitHub Issues and Linear.

## Technology Stack

- **Language**: Rust (Edition 2024), workspace with `crates/janus-schema` (Linear GraphQL schema)
- **CLI Framework**: clap 4 with derive macros
- **Async Runtime**: tokio (multi-thread) with futures, async-trait
- **Serialization**: serde, serde_json, serde_yaml_ng
- **Markdown Parsing**: comrak (AST-based, CommonMark + GFM)
- **Error Handling**: thiserror
- **TUI Framework**: iocraft (React-like declarative API, forked)
- **In-memory Store**: DashMap (concurrent hash maps), notify (fs watcher), blake3 (hashing)
- **Semantic Search**: fastembed
- **Remote APIs**: octocrab (GitHub), reqwest + cynic (Linear GraphQL)
- **MCP Server**: rmcp (Model Context Protocol via STDIO)

Additional smaller dependencies (regex, tabled, fuzzy-matcher, tracing, parking_lot, clipboard-rs, jiff, owo-colors, secrecy, etc.) are also used. Note that `tempfile` is both a regular dependency (editor temp file workflows) and a dev dependency (test isolation). Consult `Cargo.toml` for the full list and exact versions.

## Build Commands

```bash
cargo build                       # Build (debug)
cargo build --release             # Build (release)
cargo run -- <command>            # Run the CLI
cargo check                       # Check without building
```

## Test Commands

```bash
cargo test -- --format=terse 2>&1 # Run all tests
cargo test <test_name>            # Run a single test by name
cargo test test_create_basic      # Example: run one specific test
cargo test create                 # Run tests matching a pattern
cargo test --lib                  # Run only unit tests (in src/)
cargo test --test integration_test # Run a specific integration test file
cargo test -- --nocapture         # Run tests with stdout/stderr shown
```

## Lint and Format

```bash
cargo fmt                         # Format code
cargo fmt --check                 # Check formatting without changes
cargo clippy                      # Lint code
cargo clippy -- -D warnings       # Lint with all warnings as errors
```

**Note:** There is no CI for pull requests — only a release workflow (`.github/workflows/release.yml`) runs on `v*` tags. Always run `cargo test -- --format=terse 2>&1` and `cargo clippy -- -D warnings` locally before considering work complete.

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `JANUS_ROOT` | Override the `.janus` directory location |
| `JANUS_SKIP_EMBEDDINGS=1` | Skip eager embedding generation (used in tests and environments where semantic search is not needed) |
| `GITHUB_TOKEN` | GitHub API token; takes precedence over config file value |
| `LINEAR_API_KEY` | Linear API key; takes precedence over config file value |
| `RUST_LOG` | Standard tracing log level (e.g., `debug`, `info`) |

## Project Structure

```
src/
├── main.rs              # CLI entry point, tokio async runtime bootstrap
├── lib.rs               # Library root, module declarations, public re-exports
├── cli.rs               # Clap CLI definitions (Cli struct, Commands enum, all subcommand args)
├── config.rs            # App config (.janus/config.yaml): remote, auth, hooks, search settings
├── entity.rs            # Entity trait: shared async interface for Ticket and Plan (find/read/write/delete)
├── error.rs             # JanusError enum (100+ variants) and Result<T> type alias
├── graph.rs             # Dependency graph algorithms, circular dependency detection
├── locator.rs           # Path utilities: ticket_path(id), plan_path(id)
├── macros.rs            # enum_display_fromstr! macro for Display/FromStr on enums
├── next.rs              # NextWorkFinder: optimal work queue from priorities/deps/status
├── parser.rs            # Document parsing: YAML frontmatter extraction, markdown section manipulation
├── paths.rs             # Thread-local Janus root override (JanusRootGuard) for test isolation
├── types.rs             # Core domain types: TicketId, PlanId, TicketStatus, TicketType, TicketPriority,
│                        #   TicketSize, TicketMetadata, TicketData, ArrayField, path helpers
├── ticket/              # Ticket domain module
│   ├── mod.rs           #   Ticket struct (facade), Entity trait impl, re-exports
│   ├── builder.rs       #   TicketBuilder: builder pattern for creating tickets
│   ├── locator.rs       #   TicketLocator: find tickets by partial ID (store/filesystem lookup)
│   ├── manipulator.rs   #   FrontmatterEditor, update_field, remove_field, extract_body
│   ├── parser.rs        #   parse(): ticket file parsing (YAML frontmatter + markdown body)
│   ├── repository.rs    #   get_all_tickets, build_ticket_map, find_tickets (bulk operations)
│   └── validate.rs      #   enforce_filename_authority
├── plan/                # Plan domain module
│   ├── mod.rs           #   Plan facade, get_all_plans, ensure_plans_dir, generate_plan_id
│   ├── types.rs         #   PlanMetadata, Phase, PlanSection, PlanStatus, importable types
│   └── parser/          #   Plan file parsing and serialization
│       ├── import.rs    #     parse_importable_plan() for markdown plan import
│       ├── sections.rs  #     Structured vs free-form section parsing
│       └── serialize.rs #     Plan file serialization (write back to disk)
├── doc/                 # Document domain module (Project Knowledge Documents)
│   ├── mod.rs           #   Doc facade, Entity trait impl, re-exports
│   ├── types.rs         #   DocLabel, DocMetadata, DocChunk, DocLoadResult
│   ├── parser.rs        #   Document parsing (YAML frontmatter + markdown body)
│   └── chunker.rs       #   AST-based chunking with comrak
├── store/               # In-memory store (DashMap-backed, global singleton)
│   ├── mod.rs           #   TicketStore struct, OnceCell singleton, init from disk
│   ├── queries.rs       #   Search, filter, lookup, ticket map, depth computation
│   ├── embeddings.rs    #   Content-addressable embedding storage (.bin files, blake3-keyed)
│   ├── search.rs        #   Brute-force cosine similarity semantic search
│   └── watcher.rs       #   Filesystem watcher (notify, 150ms debounce) for live updates
├── commands/            # CLI command implementations (50+ cmd_* functions)
│   ├── mod.rs           #   Module root, re-exports, shared CommandOutput type
│   ├── add_note.rs      #   cmd_add_note
│   ├── board.rs         #   cmd_board (TUI kanban)
│   ├── cache.rs         #   cmd_cache_status, cmd_cache_prune, cmd_cache_rebuild
│   ├── config.rs        #   cmd_config_show, cmd_config_set, cmd_config_get
│   ├── create.rs        #   cmd_create
│   ├── dep.rs           #   cmd_dep_add, cmd_dep_remove, cmd_dep_tree
│   ├── dep_tree.rs      #   TreeBuilder helper (JSON/text dependency trees)
│   ├── doctor.rs        #   cmd_doctor
│   ├── edit.rs          #   cmd_edit (open in $EDITOR)
│   ├── events.rs        #   cmd_events_prune
│   ├── graph.rs         #   cmd_graph
│   ├── graph/           #   Graph submodule (builder, filter, formatter, types)
│   ├── hook.rs          #   cmd_hook_list, cmd_hook_install, cmd_hook_run, cmd_hook_enable/disable/log
│   ├── interactive.rs   #   Interactive prompting helpers (yes/no confirmation)
│   ├── link.rs          #   cmd_link_add, cmd_link_remove
│   ├── ls.rs            #   cmd_ls_with_options
│   ├── next.rs          #   cmd_next
│   ├── query.rs         #   cmd_query (JSON output)
│   ├── remote_browse.rs #   cmd_remote_browse
│   ├── search.rs        #   cmd_search (semantic search)
│   ├── set.rs           #   cmd_set
│   ├── show.rs          #   cmd_show
│   ├── status.rs        #   cmd_start, cmd_close, cmd_reopen, cmd_status
│   ├── view.rs          #   cmd_view (TUI issue browser)
│   ├── plan/            #   Plan subcommands (create, delete, rename, edit, import, show_import_spec,
│   │                    #     ls, next, add/remove phase, reorder, show, status,
│   │                    #     add/remove/move ticket, verify, formatters)
│   └── sync/            #   Remote sync (cmd_adopt, cmd_push, cmd_remote_link, cmd_sync,
│                        #     sanitize, sync_executor, sync_strategy, sync_ui)
├── display/             # CLI output formatting (colored badges, JSON output, formatters)
├── embedding/           # Embedding model for semantic search (fastembed)
├── events/              # Event logging (.janus/events.ndjson): Event, EventType, Actor
├── fs/                  # File I/O: atomic write (temp file + rename), read, delete with hooks
├── hooks/               # Hook execution: pre/post scripts via .janus/hooks/, timeout handling
├── mcp/                 # MCP server (Model Context Protocol via STDIO); grep for register_tool!
├── query/               # Query builder: composable ticket filters and sorting
├── remote/              # Remote sync: GitHub (octocrab) and Linear (cynic GraphQL) providers
├── status/              # Status computation for tickets and plans: predicates, aggregation
├── tui/                 # Terminal UI (see src/tui/**/*.rs): view, board, remote manager,
│                        #   components, services, theme, navigation, search, state, etc.
└── utils/               # Utilities: ID generation, IO helpers, text processing, dir scanning, validation
```

## Code Style Guidelines

### Naming Conventions

- **Functions/Variables**: `snake_case` (e.g., `find_ticket_by_id`, `ticket_type`)
- **Types/Enums**: `PascalCase` (e.g., `TicketStatus`, `TicketMetadata`)
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `VALID_STATUSES`, `EMBEDDING_MODEL_NAME`)
- **Modules**: `snake_case` (e.g., `add_note`, `status`)

#### Type Naming: Infrastructure vs Domain

- **"Janus" prefix**: Infrastructure types not tied to a specific domain concept:
  - `JanusError` — main error enum, `JanusRootGuard` — test path isolation, `JanusTools` — MCP adapter
- **Domain terms**: Business logic concepts:
  - `TicketStore`, `TicketMetadata`, `TicketBuilder` — ticket domain
  - `PlanMetadata`, `Phase`, `PlanStatus` — plan domain

When adding new types, use `Janus*` for project-level infrastructure and domain terms (`Ticket*`, `Plan*`) for business concepts.

### Imports

Preferred import ordering uses three groups separated by blank lines:
1. Standard library (`std::`)
2. External crates (alphabetically)
3. Crate-internal (`crate::`, `super::`)

Note: This convention is not uniformly enforced across all files, but new code should follow it.

```rust
use std::fs;
use std::path::PathBuf;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::{JanusError, Result};
use crate::types::TicketMetadata;
```

### Error Handling

- All errors defined in `src/error.rs` as `JanusError` enum variants (100+ variants)
- Type alias: `pub type Result<T> = std::result::Result<T, JanusError>;`
- Use `?` operator for propagation, `#[from]` for automatic conversion from external errors
- Add new error variants to `JanusError` rather than using ad-hoc error strings

```rust
#[derive(Error, Debug)]
pub enum JanusError {
    #[error("ticket '{0}' not found")]
    TicketNotFound(TicketId),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

### Types

- Use `#[derive(Debug, Clone, Serialize, Deserialize)]` for data types
- Implement `Default` for types with sensible defaults (e.g., `TicketStatus` → `New`, `TicketType` → `Task`, `TicketPriority` → `P2`)
- Use `enum_display_fromstr!` macro (in `src/macros.rs`) for enum `Display`/`FromStr` implementations
- Use `#[serde(rename_all = "lowercase")]` or `#[serde(rename_all = "snake_case")]` for enum serialization
- Use `#[serde(skip_serializing_if = "Option::is_none")]` for optional metadata fields
- Use `#[serde(transparent)]` for newtype wrappers (`TicketId`, `PlanId`)

### Command Functions

- Named `cmd_<name>` (e.g., `cmd_create`, `cmd_show`, `cmd_ls_with_options`)
- Most are `pub async fn ... -> Result<()>` (a few sync commands exist for simple operations)
- Each command lives in its own file under `src/commands/`
- Commands print output to stdout; errors propagate via `Result`

### Async Patterns

- **Finding entities is async**: `Ticket::find()`, `Plan::find()` require store initialization (`get_or_init_store().await`)
- **File I/O is sync by default**: `ticket.read()`, `ticket.write()`, `ticket.update_field()` use `std::fs`
- **Async file I/O variants exist**: `read_async()`, `read_content_async()`, `delete_async()` use `tokio::fs`
- **Repository functions are async**: `get_all_tickets()`, `build_ticket_map()` go through the store
- **Entity trait**: Shared interface (`find`, `read`, `write`, `delete`, `exists`) implemented by both `Ticket` and `Plan`

## Test Conventions

### Unit Tests

- Inline `#[cfg(test)]` modules within source files (90+ test modules across the codebase)
- Test function names: `test_<feature>_<scenario>` (e.g., `test_parse_basic_ticket`, `test_search_tickets_case_insensitive`)
- Use `#[serial]` from `serial_test` **only** for tests that touch process-global singletons (`OnceCell`/`OnceLock`), not for general filesystem tests

### Integration Tests

- Located in `tests/` with subdirectories: `commands/`, `plan/`, `hooks/`, `cache/`
- Use `JanusTest` helper (in `tests/common/mod.rs`) which creates a `TempDir` and spawns the `janus` binary as a subprocess
- Isolation via temp directories — `#[serial]` is NOT needed for integration tests
- Test subprocess invocations set `JANUS_SKIP_EMBEDDINGS=1` to avoid slow embedding operations
- Shared helpers in `tests/common/`: `fixtures.rs` (fixture paths), `mock_data.rs` (test data builders), `snapshot.rs` (TUI snapshot filters), `tui_helpers.rs` (key event constructors)

```rust
// Typical integration test pattern
let janus = JanusTest::new();
let output = janus.run_success(&["create", "Test ticket", "--prefix", "t"]);
assert!(output.contains("t-"));
```

### Snapshot Testing

- Uses `insta` with filters for TUI view model and board state tests
- Snapshot files in `tests/snapshots/`
- Snapshot filter infrastructure in `tests/common/snapshot.rs` can normalize timestamps → `[TIMESTAMP]` and durations → `[TIME]` for text-based snapshots via `tui_snapshot_filters()`
- Named snapshots: `insta::assert_debug_snapshot!("name", data)`

### Dev Dependencies

- `tempfile` — isolated temp directories
- `serial_test` — `#[serial]` for global singleton tests
- `insta` — snapshot testing with filters

## Domain Concepts

- **Ticket statuses**: `new`, `next`, `in_progress`, `complete`, `cancelled`
- **Ticket types**: `bug`, `feature`, `task`, `epic`, `chore`
- **Priorities**: 0-4 (P0 highest, P4 lowest, default P2)
- **Sizes**: `xsmall`/`xs`, `small`/`s`, `medium`/`m`, `large`/`l`, `xlarge`/`xl`
- **Dependencies**: Tickets can depend on other tickets (blocks/blocked-by)
- **Links**: Bidirectional relationships between tickets
- **Parent/Child**: Hierarchical ticket organization
- **ID Format**: `<prefix>-<hash>` where hash is 4-8 chars (e.g., `j-a1b2`)
- **Plan ID Format**: `plan-<hash>` where hash is 4-8 chars (e.g., `plan-a1b2`)
- **Document Label Format**: Free-form filesystem-safe string (e.g., `architecture`, `api-design`)

## Document File Format

Documents are stored as `.md` files in `.janus/docs/` with YAML frontmatter:

```markdown
---
label: architecture
description: System architecture overview
tags: ["architecture", "design"]
created: 2024-01-01T00:00:00Z
updated: 2024-01-15T00:00:00Z
---
# Architecture

Document content...
```

Documents are chunked at heading boundaries for semantic search. Each chunk tracks its heading path (e.g., `["Architecture", "API Design"]`), content, and line numbers.

## Ticket File Format

Tickets are stored as `.md` files in `.janus/items/` with YAML frontmatter:

```markdown
---
id: j-a1b2
uuid: 550e8400-e29b-41d4-a716-446655440000
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
size: medium
external-ref:              # optional, external reference string
remote:                    # optional, e.g. "github:owner/repo/123" or "linear:org/PROJ-123"
parent:                    # optional, TicketId for hierarchical organization
spawned-from:              # optional, TicketId of parent that spawned this ticket
spawn-context:             # optional, why this ticket was spawned from the parent
depth:                     # optional, auto-computed decomposition depth (0 = root)
triaged:                   # optional, bool indicating whether ticket has been triaged
---
# Ticket Title

Description and body content...
```

## In-Memory Store Architecture

Janus uses an in-memory store backed by `DashMap` concurrent hash maps. Key points:

- **Singleton**: Global `OnceCell<TicketStore>` initialized once per process via `get_or_init_store()`
- **Initialization**: Reads all `.md` files from `.janus/items/`, `.janus/plans/`, and `.janus/docs/` into `DashMap` structures, loads pre-computed embeddings from `.janus/embeddings/`
- **Concurrency**: `DashMap` provides lock-free concurrent reads and fine-grained locking for writes
- **Filesystem Watcher**: For long-running processes (TUI, MCP server), a `notify`-based watcher monitors `.janus/` recursively, debounces events (150ms), and updates the store
- **Source of truth**: Markdown files remain authoritative; the store is always derived from them
- **Embeddings**: Stored as `.bin` files in `.janus/embeddings/`, keyed by `blake3(repo_relative_path + ":" + mtime_ns)` for content-addressable cache invalidation (paths are made relative to the janus root and normalized to forward slashes before hashing)

## Plans

Plans organize tickets toward a larger goal. Stored in `.janus/plans/` as Markdown files.

**Plan types:**
- **Simple Plan**: Direct sequence of tickets (`## Tickets` section)
- **Phased Plan**: Phases each containing tickets (`## Phase N: Name` sections)

**Plan status** (derived from constituent tickets, never stored):
- All `complete` → `complete`; all `cancelled` → `cancelled`; mixed `complete`/`cancelled` → `complete`
- All `new` or `next` → `new`; otherwise → `in_progress`

**Section types:**
- **Structured**: `## Acceptance Criteria`, `## Tickets`, `## Phase N: Name` → parsed into data
- **Free-form**: Any other H2 → preserved verbatim

```rust
let plan = Plan::find("partial-id").await?;
let metadata = plan.read()?;
let all_tickets = metadata.all_tickets();
let status = compute_plan_status(&metadata, &ticket_map);
```

## Hooks

Hooks are scripts in `.janus/hooks/` that run in response to mutations. Configured via `.janus/config.yaml` under the `hooks` key. There are 9 hook events: `ticket_created`, `ticket_updated`, `plan_created`, `plan_updated`, `plan_deleted`, `pre_write`, `post_write`, `pre_delete`, `post_delete`. Pre-hooks (`pre_write`, `pre_delete`) can abort operations by returning non-zero exit codes; post-hook failures are logged but do not abort. Hook scripts receive context via environment variables (`JANUS_EVENT`, `JANUS_ITEM_TYPE`, `JANUS_ITEM_ID`, `JANUS_FILE_PATH`, `JANUS_FIELD_NAME`, `JANUS_OLD_VALUE`, `JANUS_NEW_VALUE`, `JANUS_ROOT`). See `src/hooks/types.rs` for `HookEvent` and `HookContext`. Installable recipes are in `hook_recipes/`.

## Event Logging

All mutations are logged as NDJSON to `.janus/events.ndjson` (appended with `O_APPEND` for atomicity; failures are non-fatal). Each record includes `timestamp`, `event_type`, `entity_type`, `entity_id`, `actor`, and `data`. The 3 actor types are `cli`, `mcp`, `hook` (see `src/events/types.rs` for `EventType` and `Actor` enums). When adding new mutations, call the appropriate event log helper (e.g., `log_ticket_created()`, `log_status_changed()`, `log_field_updated()`) from `src/events/`.

## MCP Server

The MCP server (`src/mcp/`) exposes Janus functionality to AI agents via STDIO transport. Tools are registered using the `register_tool!` macro in `src/mcp/tools.rs`, which handles argument extraction, deserialization, error wrapping, and result formatting. Grep for `register_tool!` to see all registered tools. To add a new tool:

1. Define a request struct with `schemars::JsonSchema` + `Deserialize`
2. Implement a `_impl` method on `JanusTools` that takes `Parameters<RequestType>`
3. Register it with `register_tool!(router, "name", "description", RequestType, method_name, optional_args_bool)`
4. Log the mutation event with `Actor::Mcp`

Resources (static and template-based) are defined in `src/mcp/resources.rs`.

## Additional Documentation

The `docs/` directory contains detailed user-facing documentation: `commands.md`, `hooks.md`, `mcp.md`, `plans.md`, `remote-sync.md`, `semantic-search.md`, `tui.md`, and more. `DEVELOPMENT.md` covers the release process.
