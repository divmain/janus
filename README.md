# Janus

Plain-text issue tracking with TUI interfaces and remote synchronization.

## Features

- **Local-first storage**: Tickets stored as Markdown files with YAML frontmatter in `.janus/`
- **High-performance cache**: SQLite-based caching makes lookups ~100x faster
- **Rich ticket metadata**: Status, type, priority, assignee, dependencies, links, and more
- **Interactive TUI**: Browse issues with fuzzy search or view on a Kanban board
- **Remote sync**: Two-way sync with GitHub Issues and Linear
- **CLI-first**: All operations available via command line with aliases
- **JSON queries**: Export tickets as JSON with flexible filtering

## How Caching Works

Janus uses a SQLite-based cache to make common operations dramatically faster:

### Cache Benefits

- **~100x faster lookups** after cache warmup
- **Instant list operations** - `janus ls` completes in milliseconds instead of seconds
- **Automatic synchronization** - cache stays in sync on every command
- **Graceful degradation** - falls back to file reads if cache is unavailable
- **Per-repo isolation** - each repository has its own cache

### How It Works

1. **Cache location**: Stored outside the repository at `~/.local/share/janus/cache/<repo-hash>.db`
2. **Sync on every command**: Cache validates against filesystem and updates only changed tickets
3. **Source of truth**: Markdown files remain authoritative; cache is always derived from them
4. **Metadata only**: Cache stores YAML frontmatter; Markdown body is read on demand

### Cache Management

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

### Performance Characteristics

| Operation | Without Cache | With Cache | Improvement |
|-----------|---------------|------------|-------------|
| Single ticket lookup | ~500ms | <5ms | ~100x |
| List all tickets | ~1-5s | ~25-50ms | ~100x |
| TUI startup | ~1-5s | ~25-50ms | ~100x |

The cache is particularly valuable when working with large repositories (1000+ tickets) or using the TUI frequently.

## Installation

```bash
# Build from source
cargo build --release

# The binary will be at target/release/janus
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

## Setting Up Remote Sync

Janus supports bidirectional synchronization with GitHub Issues and Linear. This allows you to:

- **Adopt** existing remote issues as local tickets
- **Push** new local tickets to create remote issues
- **Sync** bidirectional changes between local and remote

### GitHub Setup

1. Get a GitHub personal access token:
   - Go to https://github.com/settings/tokens
   - Create a new token with `repo` scope
   - Copy the token (starts with `ghp_`)

2. Configure Janus:

```bash
# Method 1: Set directly
janus config set github.token ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx

# Method 2: Use environment variable
export GITHUB_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx

# Set default GitHub repository
janus config set default_remote github:myorg/myrepo
```

3. Start syncing:

```bash
# Create a new local ticket
janus create "Add OAuth flow" --description "Implement Google OAuth login"

# Push it to GitHub (creates a new issue)
janus push j-a1b2

# Or adopt an existing GitHub issue
janus adopt github:myorg/myrepo/123

# Sync changes between local and remote
janus sync j-a1b2
```

### Linear Setup

1. Get a Linear API key:
   - Go to https://linear.app/socketdev/settings/account/security
   - Create a personal API key
   - Copy the key (starts with `lin_api_`)

2. Configure Janus:

```bash
# Method 1: Set directly
janus config set linear.api_key lin_api_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx

# Method 2: Use environment variable
export LINEAR_API_KEY=lin_api_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx

# Set default Linear organization
janus config set default_remote linear:myorg
```

3. Start syncing:

```bash
# Create a new local ticket
janus create "Add user dashboard" --description "Build dashboard UI"

# Push it to Linear (creates a new issue)
janus push j-a1b2

# Or adopt an existing Linear issue
janus adopt linear:myorg/PROJ-123

# Sync changes between local and remote
janus sync j-a1b2
```

### Sync Workflows

#### Creating issues remotely

```bash
# Option 1: Push a local ticket to create a new remote issue
janus create "Fix authentication bug" --priority 1
janus push j-abc1
# Creates a new issue on GitHub/Linear and links it

# Option 2: Link an existing local ticket to an existing remote issue
janus create "Update API docs"
janus remote-link j-abc2 github:myorg/myrepo/456
```

#### Adapting existing issues

```bash
# Adopt an existing GitHub issue as a local ticket
janus adopt github:facebook/react/1234

# Adopt an existing Linear issue
janus adopt linear:mycompany/ENG-456
```

Both commands create a local ticket with:
- Remote reference stored in `remote:` field
- Title, description, status, priority, assignee imported
- URL displayed for easy reference

#### Bi-directional sync

```bash
# When local and remote get out of sync
janus sync j-abc1

# For each field that differs, you'll be prompted:
# [l]ocal->remote  - push local changes to remote
# [r]emote->local  - pull remote changes to local
# [s]kip           - keep them different
```

Sync currently supports:
- **Title**: Update title on either side
- **Status**: Sync status (with mapping for Linear's custom workflows)
- **Body/Description**: Update content

### Viewing Configuration

Check your current remote sync setup:

```bash
janus config show
```

Output:
```
Configuration:

default_remote:
  platform: github
  org: myorg
  repo: myrepo

auth:
  github.token: configured
  linear.api_key: configured

Config file: `.janus/config.yaml`
```

### Using Multiple Platforms

You can configure both GitHub and Linear simultaneously:

```bash
# Set both tokens
janus config set github.token ghp_xxxxxxxxxxxx
janus config set linear.api_key lin_api_xxxxxxxxxxxx

# Use full references to avoid ambiguity
janus adopt github:myorg/repo/123
janus adopt linear:myorg/PROJ-456
```

The `default_remote` setting only affects `janus push` (where platform must be inferred). Always use full references format for `janus adopt` and `janus remote-link`.

## First-Time Use Tutorial

### 1. Create a Project Ticket

Begin by creating a high-level epic or feature ticket:

```bash
janus create "User authentication system" \
  --type epic \
  --priority 1 \
  --description "Implement OAuth2 login with Google and GitHub providers"
```

Janus will output a ticket ID like `j-a1b2`. Save this ID for the parent ticket.

### 2. Break Down Into Tasks

Create child tickets related to the epic:

```bash
# This command uses the parent parameter to create a subtask
janus create "Set up OAuth2 credentials" \
  --type task \
  --parent j-a1b2 \
  --priority 0

janus create "Implement Google OAuth flow" \
  --type feature \
  --parent j-a1b2 \
  --priority 1

janus create "Implement GitHub OAuth flow" \
  --type feature \
  --parent j-a1b2 \
  --priority 1
```

### 3. Organize with Dependencies

Some tasks must be completed before others can begin:

```bash
# Store the IDs from the previous commands
janus create "Design OAuth flow diagrams" --priority 0
# Output: j-x1y2

# Make the implementation tasks depend on the design
janus dep add j-x3y4 j-x1y2
# j-x3y4 (Google OAuth) now depends on j-x1y2 (design) being complete
```

### 4. Track Progress

Use TUI interfaces to visualize progress:

```bash
# Kanban board view - organized by status
janus board

# Issue browser - fuzzy search through all tickets
janus view
```

In the **Kanban board**:
- Use `h/l` to move between columns (NEW, NEXT, IN PROGRESS, COMPLETE, CANCELLED)
- Use `j/k` to move between tickets in a column
- Press `s` to move selected ticket to the next status
- Press `S` (Shift+S) to move to previous status
- Use `1-5` to toggle column visibility

In the **Issue browser**:
- Type `/` to enter search mode and filter tickets
- Navigate with `j/k` (down/up) or `g/G` (top/bottom)
- Press `Tab` to switch between ticket list and detail view
- Press `e` to edit the current ticket inline
- Press `n` to create a new ticket

### 5. Add Notes and Updates

As you work, add timestamped notes to track progress:

```bash
janus add-note j-x3y4 "Started implementing Google OAuth flow"
janus add-note j-x3y4 "Completed initial auth code exchange endpoint"
```

### 6. Move Tickets to Next Status

When a task is ready to be worked on:

```bash
# Set status to "next"
janus start j-x3y4

# Or use the status command for explicit control
janus status j-x3y4 next
```

Once complete:

```bash
janus close j-x3y4
```

### 7. View Blocked and Ready Tickets

Check which tickets are blocked by dependencies:

```bash
# Tickets waiting for dependencies to be closed
janus blocked

# Tickets ready to be worked on (no unresolved dependencies)
janus ready
```

### 8. Query and Export

Get insights with JSON queries:

```bash
# All incomplete tickets
janus query '.status != "complete"'

# High-priority bugs
janus query '.priority <= 1 and .type == "bug"'

# Count tickets by status
janus query 'group_by(.status) | {status: .[0], count: length}'
```

### 9. Related Tickets

Link related tickets that don't have dependency relationships:

```bash
# Link two related features
janus link add j-x3y4 j-x4y5

# Remove a link
janus link remove j-x3y4 j-x4y5
```

---

## Commands

### Ticket Management

#### `janus create` / `janus c`

Create a new ticket.

```bash
janus create "Ticket title" [OPTIONS]

Options:
  -d, --description <TEXT>    Description text
      --design <TEXT>         Design notes
      --acceptance <TEXT>     Acceptance criteria
  -p, --priority <0-4>        Priority level (default: 2, 0 = highest)
  -t, --type <TYPE>           Type: bug, feature, task, epic, chore (default: task)
  -a, --assignee <NAME>       Assignee (defaults to git user.name)
      --external-ref <REF>    External reference (e.g., gh-123)
      --parent <ID>           Parent ticket ID
```

#### `janus show` / `janus s`

Display ticket details with dependencies, links, and relationships.

```bash
janus show <ID>
```

ID can be partial - first few unique characters are sufficient.

#### `janus edit`

Open ticket in `$EDITOR` for manual editing.

```bash
janus edit <ID>
```

#### `janus add-note`

Add a timestamped note to a ticket.

```bash
janus add-note <ID> [NOTE_TEXT]
```

If no note text provided, reads from stdin.

### Status Management

#### `janus start`

Mark ticket as in-progress.

```bash
janus start <ID>
```

#### `janus close`

Mark ticket as complete.

```bash
janus close <ID>
```

#### `janus reopen`

Reopen a closed ticket.

```bash
janus reopen <ID>
```

#### `janus status`

Set ticket to any status.

```bash
janus status <ID> <STATUS>

Valid statuses: new, next, in_progress, complete, cancelled
```

### Dependencies

#### `janus dep add`

Add a dependency - the second ticket must be completed before the first.

```bash
janus dep add <ID> <DEP_ID>

# Example: Feature j-1234 depends on task j-5678
janus dep add j-1234 j-5678
```

#### `janus dep remove`

Remove a dependency.

```bash
janus dep remove <ID> <DEP_ID>
```

#### `janus dep tree`

Show dependency tree for a ticket.

```bash
janus dep tree <ID> [--full]

# --full shows all nodes including duplicates
```

### Links

#### `janus link add`

Link tickets together (bidirectional relationship).

```bash
janus link add <ID1> <ID2> [ID3 ...]

# Example: Link related tickets
janus link add j-1234 j-5678 j-9012
```

#### `janus link remove`

Remove a link between tickets.

```bash
janus link remove <ID1> <ID2>
```

### Listing

#### `janus ls`

List all tickets, optionally filtered by status.

```bash
janus ls [--status <STATUS>]

# Examples
janus ls
janus ls --status next
janus ls --status in_progress
```

#### `janus ready`

List tickets ready to be worked on (no unresolved dependencies).

```bash
janus ready
```

#### `janus blocked`

List tickets blocked by dependencies.

```bash
janus blocked
```

#### `janus closed`

List recently closed tickets.

```bash
janus closed [--limit <N>]

# Default: show last 20 tickets
```

#### `janus query`

Output tickets as JSON, optionally filtered with jq syntax.

```bash
janus query [FILTER]

# Examples
janus query                               # all tickets as JSON
janus query '.status == "new"'            # filter by status
janus query '.type == "bug"'              # filter by type
janus query '.priority <= 1'              # high priority only
janus query '.assignee == "Alice Smith"'  # filter by assignee
```

### TUI Interfaces

#### `janus view`

Interactive issue browser with fuzzy search.

**Keyboard shortcuts:** `j/k` navigate, `g/G` top/bottom, `/` search, `e` edit, `s` cycle status, `n` new ticket, `Tab` switch pane, `q` quit

#### `janus board`

Kanban board view organized by status.

**Keyboard shortcuts:** `h/l` move column, `j/k` move card, `/` search, `e` edit, `s/S` move right/left, `n` new ticket, `1-5` toggle column, `q` quit

### Remote Sync

#### `janus adopt`

Adopt a remote issue and create a local ticket.

```bash
janus adopt <REMOTE_REF>

# Examples
janus adopt github:owner/repo/123
janus adopt linear:org/PROJ-123
```

#### `janus push`

Push a local ticket to create a remote issue.

```bash
janus push <ID>
```

#### `janus remote-link`

Link a local ticket to an existing remote issue.

```bash
janus remote-link <ID> <REMOTE_REF>

# Example
janus remote-link j-a1b2 github:myorg/myrepo/456
```

#### `janus sync`

Sync a local ticket with its remote issue.

```bash
janus sync <ID>
```

### Configuration

#### `janus config set`

Set a configuration value.

```bash
janus config set <KEY> <VALUE>

# GitHub token
janus config set github.token ghp_xxxxxxxxxxxx

# Linear API key
janus config set linear.api_key lin_api_xxxxxxxxxxxx

# Default remote (platform:org or platform:org/repo)
janus config set default_remote github:myorg/myrepo
janus config set default_remote linear:myorg
```

Tokens can also be set via environment variables:
- `GITHUB_TOKEN`
- `LINEAR_API_KEY`

#### `janus config get`

Get a configuration value.

```bash
janus config get <KEY>

# Valid keys: github.token, linear.api_key, default_remote
```

#### `janus config show`

Display current configuration.

```bash
janus config show
```

## Ticket Statuses

- **new**: Newly created ticket, not yet prioritized
- **next**: High-priority ticket ready to work on soon
- **in_progress**: Currently being worked on
- **complete**: Finished and tested
- **cancelled**: No longer relevant or applicable

## Ticket Types

- **bug**: Defect or error that needs fixing
- **feature**: New functionality or enhancement
- **task**: Specific work item
- **epic**: Large feature broken into smaller tickets
- **chore**: Maintenance or housekeeping task

## Priority Levels

- **P0**: Critical, must fix immediately
- **P1**: High priority, address soon
- **P2**: Default priority
- **P3**: Low priority
- **P4**: Nice to have

## Ticket File Format

Tickets are stored as Markdown files in `.janus/` with YAML frontmatter:

```markdown
---
id: j-a1b2
status: new
type: feature
priority: 1
assignee: John Doe
created: 2024-01-01T00:00:00Z
deps: []
links: []
remote: github:myorg/myrepo/123
---

# Design OAuth flow

## Description
Implement OAuth2 authentication flow for Google and GitHub login providers.

## Design Notes
- Use authorization code flow for security
- Store refresh tokens securely
- Implement token rotation

## Acceptance Criteria
- [ ] User can authenticate with Google
- [ ] User can authenticate with GitHub
- [ ] Tokens are refreshed automatically
- [ ] Logout clears all session data
```

## Tips

- **Partial IDs**: Only need first 2-3 characters of ticket ID (e.g., `j-a1` instead of `j-a1b2`)
- **Dependencies**: Use `janus dep tree` to visualize complex dependency chains
- **Aliases**: Common commands have short aliases (`janus c` for `create`, `janus s` for `show`)
- **TUI**: Use `janus view` for quick navigation and `janus board` for status management
- **Remote sync**: Use environment variables for sensitive credentials (`GITHUB_TOKEN`, `LINEAR_API_KEY`) instead of storing in config files
- **Bi-directional sync**: Run `janus sync` regularly when collaborating via GitHub/Linear to keep local and remote in sync
- **Short references**: once `default_remote` is set, use short formats (e.g., `ENG-123` for Linear instead of `linear:org/ENG-123`)
