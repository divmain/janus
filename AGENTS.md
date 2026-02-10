# AGENTS.md - Janus Project Guidelines

This document provides essential information for AI coding agents working in this repository.

## Project Overview

Janus is a plain-text issue tracking CLI tool written in Rust. It stores tickets as Markdown files with YAML frontmatter in a `.janus` directory. The project provides commands for creating, managing, and querying tickets.

## Technology Stack

- **Language**: Rust (Edition 2024)
- **CLI Framework**: clap 4 with derive macros
- **Serialization**: serde, serde_json, serde_yaml_ng
- **Markdown Parsing**: comrak 0.34 (AST-based markdown parser)
- **Date/Time**: jiff 0.2
- **Error Handling**: thiserror 2
- **Terminal Colors**: owo-colors 4
- **In-memory Store**: DashMap (concurrent hash maps for ticket/plan data)
- **Filesystem Watching**: notify 8 (cross-platform fs event watcher)
- **Hashing**: blake3 (content-addressable embedding keys)

## Build Commands

```bash
# Build (debug)
cargo build

# Build (release)
cargo build --release

# Run the CLI
cargo run -- <command>

# Check without building
cargo check
```

## Test Commands

```bash
# Run all tests
cargo test -- --format=terse 2>&1

# Run a single test by name
cargo test <test_name>
cargo test test_create_basic

# Run tests matching a pattern
cargo test create

# Run only unit tests (in src/)
cargo test --lib

# Run only integration tests
cargo test --test integration_test

# Run tests with output shown
cargo test -- --nocapture
```

## Lint and Format

```bash
# Format code (uses rustfmt)
cargo fmt

# Check formatting without changing files
cargo fmt --check

# Lint code (uses clippy)
cargo clippy

# Lint with all warnings as errors
cargo clippy -- -D warnings
```

## Project Structure

```
src/
├── main.rs              # CLI entry point, clap command definitions
├── lib.rs               # Library exports
├── error.rs             # Custom error types using thiserror
├── parser.rs            # Ticket file parsing (YAML frontmatter + Markdown)
├── ticket.rs            # Ticket operations (separated into focused types)
│   ├── TicketLocator    # Path + ID resolution
│   ├── TicketFile       # File I/O operations
│   ├── TicketContent    # Parsing/serialization
│   ├── TicketEditor     # Field manipulation with hooks
│   ├── Ticket           # Facade for common operations
│   └── TicketRepository # Orchestrates components
├── types.rs             # Core types (TicketStatus, TicketType, etc.)
├── utils.rs             # Utility functions (ID generation, dates)
├── plan/                # Plan module
│   ├── mod.rs           # Plan operations (find, read, write, status computation)
│   ├── parser.rs        # Plan file parsing and serialization
│   └── types.rs         # Plan data structures (PlanMetadata, Phase, etc.)
├── store/               # In-memory store module
│   ├── mod.rs           # TicketStore struct, singleton, init
│   ├── queries.rs       # Query operations (search, filter, lookup)
│   ├── embeddings.rs    # Content-addressable embedding storage
│   ├── search.rs        # Brute-force cosine similarity search
│   └── watcher.rs       # Filesystem watcher (notify-based)
└── commands/
    ├── mod.rs           # Command module exports and shared formatting
    ├── add_note.rs      # Add timestamped notes
    ├── create.rs        # Create new tickets
    ├── cache.rs         # Cache CLI commands
    ├── dep.rs           # Dependency management
    ├── edit.rs          # Open ticket in $EDITOR
    ├── link.rs          # Link management
    ├── ls.rs            # List commands (ls with --ready, --blocked, --closed flags)
    ├── plan.rs          # Plan CLI commands
    ├── query.rs         # JSON query output
    ├── show.rs          # Display ticket details
    └── status.rs        # Status transitions
tests/                   # Integration tests
```

## Code Style Guidelines

### Naming Conventions

- **Functions/Variables**: `snake_case` (e.g., `find_ticket_by_id`, `ticket_type`)
- **Types/Enums**: `PascalCase` (e.g., `TicketStatus`, `TicketMetadata`)
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `TICKETS_DIR`, `VALID_TYPES`)
- **Modules**: `snake_case` (e.g., `add_note`, `status`)

### Imports

Order imports as follows:
1. Standard library (`std::`)
2. External crates (alphabetically)
3. Crate-internal (`crate::`)

```rust
use std::fs;
use std::path::PathBuf;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::{JanusError, Result};
use crate::types::TicketMetadata;
```

### Error Handling

- Use `thiserror` for custom error types
- Define all errors in `src/error.rs` as `JanusError` enum variants
- Use `Result<T>` type alias which maps to `Result<T, JanusError>`
- Use `?` operator for error propagation
- Wrap external errors with `#[from]` for automatic conversion

```rust
#[derive(Error, Debug)]
pub enum JanusError {
    #[error("ticket '{0}' not found")]
    TicketNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

### Types

- Use `Option<T>` for optional fields
- Use `#[derive(Debug, Clone, Serialize, Deserialize)]` for data types
- Implement `Default` for types with sensible defaults
- Implement `Display` and `FromStr` for enum types
- Use `#[serde(rename_all = "lowercase")]` for enum serialization

### Command Functions

- Command functions are named `cmd_<name>` (e.g., `cmd_create`, `cmd_show`)
- Commands return `Result<()>` and print output to stdout
- Error messages go to stderr via the error type
- Each command lives in its own file under `src/commands/`

### Tests

- Unit tests use `#[cfg(test)]` modules within source files
- Integration tests spawn the compiled binary in temp directories
- Use `tempfile` crate for isolated test environments
- Test function names: `test_<feature>_<scenario>`
- **Important**: Tests that make changes to the filesystem (create/delete files, databases, etc.) must use the `#[serial]` attribute from the `serial_test` crate to prevent race conditions when running tests in parallel

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_parse_basic_ticket() {
        // test implementation
    }

    #[test]
    #[serial]  // Required for tests that modify filesystem
    fn test_cache_initialization() {
        // test implementation that creates files/databases
    }
}
```

## In-Memory Store Architecture

Janus uses an in-memory store backed by `DashMap` concurrent hash maps. There is no external database. Key points:

- **Store Location**: In-process memory (no database files)
- **Initialization**: On process start, all tickets and plans are read from `.janus/items/` and `.janus/plans/` into `DashMap` structures
- **Singleton**: The store is a global `OnceCell<TicketStore>` initialized once per process via `get_or_init_store()`
- **Concurrency**: `DashMap` provides lock-free concurrent reads and fine-grained locking for writes
- **Filesystem Watcher**: For long-running processes (TUI, MCP server), a `notify`-based watcher monitors `.janus/` recursively, debounces events (150ms), and updates the store automatically
- **Source of truth**: Markdown files remain authoritative; the store is always derived from them
- **Embeddings**: Stored as `.bin` files in `.janus/embeddings/`, keyed by `blake3(file_path + ":" + mtime_ns)` for content-addressable cache invalidation

### Cache Commands

```bash
janus cache status   # Show embedding coverage, model name, dir size
janus cache prune    # Delete orphaned embedding files
janus cache rebuild  # Regenerate all embeddings
```

### Store Implementation

The store is implemented in:
- `src/store/mod.rs` - `TicketStore` struct, singleton, initialization from disk
- `src/store/queries.rs` - Query operations (search, filter, lookup, ticket map)
- `src/store/embeddings.rs` - Content-addressable embedding storage (save/load/prune `.bin` files)
- `src/store/search.rs` - Brute-force cosine similarity semantic search
- `src/store/watcher.rs` - Filesystem watcher with debounced event processing
- `src/commands/cache.rs` - CLI command handlers for cache status/prune/rebuild

The store:
1. Reads all `.md` files from `.janus/items/` and `.janus/plans/` at startup
2. Parses YAML frontmatter + markdown body into `TicketMetadata` / `PlanMetadata`
3. Populates `DashMap<String, TicketMetadata>` and `DashMap<String, PlanMetadata>`
4. Loads pre-computed embeddings from `.janus/embeddings/` for semantic search
5. Optionally starts a filesystem watcher for live updates

## Domain Concepts

- **Ticket statuses**: `new`, `next`, `in_progress`, `complete`, `cancelled`
- **Ticket types**: `bug`, `feature`, `task`, `epic`, `chore`
- **Priorities**: 0-4 (P0 highest, P4 lowest, default P2)
- **Sizes**: `xsmall`, `small`, `medium`, `large`, `xlarge` (for complexity estimation)
- **Dependencies**: Tickets can depend on other tickets (blocks/blocked-by)
- **Links**: Bidirectional relationships between tickets
- **Parent/Child**: Hierarchical ticket organization
- **ID Format**: `<prefix>-<4-char-hash>` (e.g., `j-a1b2`)
- **Plans**: Hierarchical structures organizing tickets toward a larger goal
- **Plan ID Format**: `plan-<4-char-hash>` (e.g., `plan-a1b2`)

## Ticket File Format

Tickets are stored as `.md` files in `.janus/` with YAML frontmatter:

```markdown
---
id: j-a1b2
status: new
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: task
priority: 2
size: medium
---
# Ticket Title

Description and body content...
```

## Common Patterns

### Finding Tickets by ID

```rust
// Async API (preferred)
let ticket = Ticket::find("partial-id").await?;
let metadata = ticket.read()?;
```

### Updating Ticket Fields

```rust
ticket.update_field("status", "complete")?;
ticket.add_to_array_field("deps", "other-id")?;
```

### Getting All Tickets

```rust
// Async API (preferred)
let tickets = get_all_tickets().await;
let ticket_map = build_ticket_map().await; // HashMap<String, TicketMetadata>
```

## Plans

Plans are hierarchical structures organizing tickets toward a larger goal. They are stored in `.janus/plans/` as Markdown files with YAML frontmatter.

**Plan types:**
- **Simple Plan**: Direct sequence of tickets (has `## Tickets` section)
- **Phased Plan**: Sequence of phases, each with tickets (has `## Phase N: Name` sections)

**Plan status computation** (derived from constituent tickets, never stored):
- All `complete` → `complete`
- All `cancelled` → `cancelled`  
- Mixed `complete`/`cancelled` → `complete`
- All `new` or `next` → `new`
- Otherwise → `in_progress`

**Section types in plan files:**
- **Structured**: `## Acceptance Criteria`, `## Tickets`, `## Phase N: Name` → parsed into data structures
- **Free-form**: Any other H2 (e.g., `## Overview`) → preserved verbatim

### Working with Plans in Code

```rust
use crate::plan::{Plan, compute_plan_status, get_all_plans};
use crate::plan::types::{PlanMetadata, Phase, PlanSection};

// Find and read a plan
let plan = Plan::find("partial-id").await?;
let metadata = plan.read()?;

// Check plan type
if metadata.is_phased() {
    for phase in metadata.phases() {
        println!("Phase {}: {}", phase.number, phase.name);
    }
}

// Get all tickets in a plan
let all_tickets = metadata.all_tickets();

// Compute plan status
let ticket_map = build_ticket_map().await;
let status = compute_plan_status(&metadata, &ticket_map);
println!("Progress: {}", status.progress_string()); // e.g., "5/12 (41%)"

// Get all plans
let plans = get_all_plans().await;
```

### Plan Import

The plan import feature (`src/commands/plan.rs`: `cmd_plan_import`) parses markdown documents and creates plans with tickets. Key implementation details:

- **Parser**: `parse_importable_plan()` in `src/plan/parser.rs` handles document parsing
- **Types**: `ImportablePlan`, `ImportablePhase`, `ImportableTask` in `src/plan/types.rs`
- **Validation errors**: Use `JanusError::ImportFailed` with descriptive `issues` vector
- **Format spec**: Embedded in `PLAN_FORMAT_SPEC` constant, shown via `janus plan import-spec`

```rust
use crate::plan::{parse_importable_plan, ImportablePlan};

let plan = parse_importable_plan(&content)?;
println!("Tasks: {}", plan.task_count());
```

## TUI Component Organization

### Shared Components

The TUI uses a set of reusable components in `src/tui/components/`:

- **SearchBox / InlineSearchBox** - Single-line text input using iocraft's `TextInput`
- **Select** - Cycle through enum values (status, type, priority)
- **TicketCard** - Compact ticket display
- **TicketDetail** - Full ticket info with scrollable body (read-only)
- **TicketList** - Left pane list view
- **Toast** - Error/success notifications
- **TextViewer** - Read-only multiline text viewer with scroll indicators
- **TextEditor** - Editable multiline text input with full cursor support

### Component Patterns

#### Multi-line Text Display

For showing read-only multiline text with scroll indicators:

```rust
TextViewer(
    text: content,
    scroll_offset: scroll_state,
    has_focus: false,
    placeholder: None,      // Optional
)
```

#### Multi-line Text Editing

For editing multiline text with full cursor support:

```rust
TextEditor(
    value: text_state,
    has_focus: focused_field.get() == EditField::Body,
)
```

**Note**: TextEditor uses iocraft's `TextInput(multiline: true)` which provides:
- Full cursor positioning (insert/delete anywhere)
- Arrow key navigation
- Automatic scrolling
- No vim-style j/k support (use arrows)
