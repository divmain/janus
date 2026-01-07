# AGENTS.md - Janus Project Guidelines

This document provides essential information for AI coding agents working in this repository.

## Project Overview

Janus is a plain-text issue tracking CLI tool written in Rust. It stores tickets as Markdown files with YAML frontmatter in a `.janus` directory. The project provides commands for creating, managing, and querying tickets.

## Technology Stack

- **Language**: Rust (Edition 2024)
- **CLI Framework**: clap 4 with derive macros
- **Serialization**: serde, serde_json, serde_yaml_ng
- **Date/Time**: jiff 0.2
- **Error Handling**: thiserror 2
- **Terminal Colors**: owo-colors 4
- **Database**: Turso (pure Rust SQLite for caching)

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
cargo test

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
├── ticket.rs            # Ticket operations (read, write, find)
├── types.rs             # Core types (TicketStatus, TicketType, etc.)
├── utils.rs             # Utility functions (ID generation, dates)
└── commands/
    ├── mod.rs           # Command module exports and shared formatting
    ├── add_note.rs      # Add timestamped notes
    ├── create.rs        # Create new tickets
    ├── dep.rs           # Dependency management
    ├── edit.rs          # Open ticket in $EDITOR
    ├── link.rs          # Link management
    ├── ls.rs            # List commands (ls, ready, blocked, closed)
    ├── query.rs         # JSON query output
    ├── show.rs          # Display ticket details
    └── status.rs        # Status transitions
tests/
└── integration_test.rs  # Integration tests (~850 lines)
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

## Caching Architecture

Janus uses a SQLite-based caching layer (via Turso) that acts as a read replica of the `.janus/items/` directory. Key points:

- **Cache Location**: `~/.local/share/janus/cache/<repo-hash>.db` (per-repo isolation)
- **Auto-sync**: Cache is validated and updated on every `janus` command invocation
- **Graceful degradation**: Falls back to file reads if cache is unavailable
- **Performance**: ~100x faster for common operations after cache warm
- **Source of truth**: Markdown files remain authoritative; cache is always derived from them

### Cache Commands

```bash
janus cache          # Show cache status
janus cache clear    # Clear (delete) cache for current repo
janus cache rebuild  # Force full cache rebuild
janus cache path     # Print path to cache DB file
```

### Cache Implementation

The cache is implemented in:
- `src/cache.rs` - Core caching logic with Turso async API
- `src/cache_error.rs` - Cache-specific error types
- `src/commands/cache.rs` - CLI command handlers

The cache:
1. Scans `.janus/items/` directory for mtime changes
2. Computes diff (added/modified/deleted tickets)
3. Updates only changed tickets in a single transaction
4. Returns data from cache for fast lookups

All cache operations are async and use `tokio` runtime.

## Domain Concepts

- **Ticket statuses**: `new`, `complete`, `cancelled`
- **Ticket types**: `bug`, `feature`, `task`, `epic`, `chore`
- **Priorities**: 0-4 (P0 highest, P4 lowest, default P2)
- **Dependencies**: Tickets can depend on other tickets (blocks/blocked-by)
- **Links**: Bidirectional relationships between tickets
- **Parent/Child**: Hierarchical ticket organization
- **ID Format**: `<prefix>-<4-char-hash>` (e.g., `j-a1b2`)

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
assignee: John Doe
---
# Ticket Title

Description and body content...
```

## Common Patterns

### Finding Tickets by ID

```rust
let ticket = Ticket::find("partial-id")?;
let metadata = ticket.read()?;
```

### Updating Ticket Fields

```rust
ticket.update_field("status", "complete")?;
ticket.add_to_array_field("deps", "other-id")?;
```

### Getting All Tickets

```rust
let tickets = get_all_tickets();
let ticket_map = build_ticket_map(); // HashMap<String, TicketMetadata>
```
