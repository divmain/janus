# Janus

Plain-text issue tracking with TUI interfaces and remote synchronization.

## Features

- **Local-first storage**: Tickets stored as Markdown files with YAML frontmatter in `.janus/`
- **High-performance cache**: SQLite-based caching makes lookups ~100x faster
- **Rich ticket metadata**: Status, type, priority, dependencies, links, and more
- **Interactive TUI**: Browse issues with fuzzy search or view on a Kanban board
- **Remote sync**: Two-way sync with GitHub Issues and Linear
- **CLI-first**: All operations available via command line with aliases
- **JSON queries**: Export tickets as JSON with flexible filtering
- **Semantic search** (optional): Find tickets by intent using AI-powered vector embeddings

## Installation

### Requirements

Janus requires a Unix-like shell environment (Linux, macOS, or Windows with WSL).

### Homebrew (macOS)

```bash
brew tap divmain/janus
brew install janus
```

### From Source

```bash
git clone https://github.com/divmain/janus.git
cd janus
cargo build --release
# Binary at target/release/janus
```

## Quick Start

```bash
# Create your first ticket
janus create "Fix login bug" \
  --type bug \
  --priority 1 \
  --description "Users cannot login after password reset"

# View ticket details
janus show j-a1b2

# Start working on it
janus start j-a1b2

# Mark as complete
janus close j-a1b2

# List all tickets
janus ls
```

## Core Concepts

### Ticket Lifecycle

```
new -> next -> in_progress -> complete
                          \-> cancelled
```

### Ticket Types

`bug` | `feature` | `task` | `epic` | `chore`

### Priority Levels

P0 (critical) through P4 (nice to have). Default is P2.

### Dependencies

Tickets can depend on other tickets. Use `janus ls --ready` to see tickets with all dependencies complete, or `janus ls --blocked` to see what's waiting.

## Key Commands

| Command | Alias | Description |
|---------|-------|-------------|
| `janus create "Title"` | `c` | Create a new ticket |
| `janus show <id>` | `s` | View ticket details |
| `janus edit <id>` | `e` | Edit in $EDITOR |
| `janus ls` | `l` | List tickets |
| `janus next` | `n` | Show next tickets to work on |
| `janus start <id>` | | Set status to in_progress |
| `janus close <id>` | | Mark complete |
| `janus view` | | Interactive issue browser |
| `janus board` | | Kanban board view |

Partial IDs work - use just the first few unique characters (e.g., `j-a1` instead of `j-a1b2`).

## TUI Interfaces

### Issue Browser (`janus view`)

Two-pane interface with ticket list and detail view.

- `j/k` navigate, `/` search, `e` edit, `n` new ticket, `q` quit

### Kanban Board (`janus board`)

Column-based view organized by status.

- `h/l` move columns, `j/k` navigate, `s/S` change status, `q` quit

See [TUI Guide](docs/tui.md) for full keyboard shortcuts.

## Remote Sync

Sync tickets with GitHub Issues or Linear:

```bash
# Configure GitHub
janus config set github.token ghp_xxxxxxxxxxxx
janus config set default_remote github:myorg/myrepo

# Push a local ticket to GitHub
janus remote push j-a1b2

# Adopt an existing GitHub issue
janus remote adopt github:myorg/myrepo/123

# Bi-directional sync
janus remote sync j-a1b2
```

See [Remote Sync Guide](docs/remote-sync.md) for full setup.

## Plans

Organize tickets into larger goals with optional phases:

```bash
janus plan create "Q1 Release" --phase "Design" --phase "Build" --phase "Ship"
janus plan add-ticket plan-a1b2 j-x1y2 --phase "Design"
janus plan status plan-a1b2
```

See [Plans Guide](docs/plans.md) for details.

## Hooks

Run custom scripts on ticket events (create, update, status change):

```bash
janus hook install git-sync
janus hook list
```

See [Hooks Guide](docs/hooks.md) for examples.

## MCP Server

Expose tickets to AI assistants via Model Context Protocol:

```bash
janus mcp
```

See [MCP Guide](docs/mcp.md) for integration setup.

## Documentation

| Guide | Description |
|-------|-------------|
| [Getting Started](docs/getting-started.md) | First-time tutorial and workflows |
| [Commands Reference](docs/commands.md) | Full CLI reference |
| [Remote Sync](docs/remote-sync.md) | GitHub and Linear integration |
| [Cache](docs/cache.md) | How caching works |
| [TUI Guide](docs/tui.md) | Keyboard shortcuts and modes |
| [Plans](docs/plans.md) | Organizing tickets into plans |
| [Hooks](docs/hooks.md) | Automation and scripting |
| [MCP Server](docs/mcp.md) | AI assistant integration |
| [Reference](docs/reference.md) | File formats, statuses, types |

## Tips

- **Partial IDs**: Use first 2-3 characters (e.g., `j-a1` for `j-a1b2`)
- **Aliases**: `c` for create, `s` for show, `e` for edit, `l` for ls, `n` for next
- **Dependencies**: Use `janus dep tree <id>` to visualize dependency chains
- **Environment variables**: Use `GITHUB_TOKEN` and `LINEAR_API_KEY` for credentials
