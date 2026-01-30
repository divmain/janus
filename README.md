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

### Concurrency

Janus is designed to handle multiple concurrent processes safely:

- **Multiple processes**: You can run multiple `janus` commands simultaneously (e.g., a TUI in one terminal and CLI commands in another). The cache uses SQLite's WAL mode with a busy timeout, allowing processes to wait briefly for locks rather than failing immediately.

- **Source of truth**: The Markdown files in `.janus/` are always authoritative. The cache is a derived read-replica that accelerates lookups but never contains data that isn't in the files.

- **Graceful degradation**: If a cache operation fails due to contention, janus falls back to reading directly from the filesystem. Operations always succeed; only performance may be affected.

- **Cache consistency**: The cache may become temporarily stale if concurrent syncs conflict, but it will never be corrupted. Running any `janus` command will re-sync the cache with the current filesystem state.

- **What to expect**: In typical usage (occasional concurrent commands), you won't notice any issues. In heavy concurrent scenarios (many simultaneous writes), some commands may run slower due to cache fallback, but data integrity is always maintained.

## Installation

### Requirements

Janus requires a Unix-like shell environment (Linux, macOS, or Windows with WSL). The `janus edit` and `janus plan edit` commands require `sh` to open files in your configured `$EDITOR`.

### Homebrew (macOS)

```bash
# Add the tap
brew tap divmain/janus

# Install Janus
brew install janus
```

### From Source

```bash
# Clone the repository
git clone https://github.com/divmain/janus.git
cd janus

# Build release binary
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
janus remote push j-a1b2

# Or adopt an existing GitHub issue
janus remote adopt github:myorg/myrepo/123

# Sync changes between local and remote
janus remote sync j-a1b2
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
janus remote push j-a1b2

# Or adopt an existing Linear issue
janus remote adopt linear:myorg/PROJ-123

# Sync changes between local and remote
janus remote sync j-a1b2
```

### Sync Workflows

#### Creating issues remotely

```bash
# Option 1: Push a local ticket to create a new remote issue
janus create "Fix authentication bug" --priority 1
janus remote push j-abc1
# Creates a new issue on GitHub/Linear and links it

# Option 2: Link an existing local ticket to an existing remote issue
janus create "Update API docs"
janus remote link j-abc2 github:myorg/myrepo/456
```

#### Adopting existing issues

```bash
# Adopt an existing GitHub issue as a local ticket
janus remote adopt github:facebook/react/1234

# Adopt an existing Linear issue
janus remote adopt linear:mycompany/ENG-456
```

Both commands create a local ticket with:
- Remote reference stored in `remote:` field
- Title, description, status, priority imported
- URL displayed for easy reference

#### Bi-directional sync

```bash
# When local and remote get out of sync
janus remote sync j-abc1

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
janus remote adopt github:myorg/repo/123
janus remote adopt linear:myorg/PROJ-456
```

The `default_remote` setting only affects `janus remote push` (where platform must be inferred). Always use full references format for `janus remote adopt` and `janus remote link`.

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
janus ls --blocked

# Tickets ready to be worked on (no unresolved dependencies)
janus ls --ready
```

### 8. Find Next Work (Dependency-Aware)

Get a prioritized list of tickets to work on next, with dependency resolution:

```bash
# Show next 5 tickets to work on (default)
janus next

# Show next 10 tickets
janus next --limit 10

# Output as JSON for scripting
janus next --json
```

The `janus next` command is dependency-aware - it analyzes the dependency graph and returns tickets in the optimal order:

1. **Ready tickets** with no incomplete dependencies appear first
2. **Blocking dependencies** are shown before the tickets that depend on them
3. **Blocked tickets** are included with context about what's blocking them

Example output:

```
ID          Priority  Status     Title                          Reason
────────    ────────  ───────    ─────────────────────────────  ─────────────────
j-abc1      P0        ready      Fix critical bug               ready
j-def2      P1        ready      Design OAuth flow              blocking j-ghi3
j-ghi3      P1        blocked    Implement OAuth flow           target (1 dep)
```

This helps you prioritize work by showing what needs to be done first to unblock other tickets.

### 9. Query and Export

Get insights with JSON queries:

```bash
# All incomplete tickets
janus query '.status != "complete"'

# High-priority bugs
janus query '.priority <= 1 and .type == "bug"'

# Count tickets by status
janus query 'group_by(.status) | {status: .[0], count: length}'
```

### 10. Related Tickets

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
      --external-ref <REF>    External reference (e.g., gh-123)
      --parent <ID>           Parent ticket ID
      --prefix <PREFIX>       Custom prefix for ticket ID (e.g., 'perf' for 'perf-a982')
```

#### `janus show` / `janus s`

Display ticket details with dependencies, links, and relationships.

```bash
janus show <ID>
```

ID can be partial - first few unique characters are sufficient.

#### `janus edit` / `janus e`

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

#### `janus ls` / `janus l`

List tickets with optional filters.

```bash
janus ls [OPTIONS]

Options:
      --ready          Show tickets ready to work on (no incomplete deps, status=new|next)
      --blocked        Show tickets with incomplete dependencies
      --closed         Show recently closed/cancelled tickets
      --all            Include closed/cancelled tickets in output
      --status <STATUS> Filter by specific status
      --triaged <BOOL> Filter by triage status (true|false)
      --limit <N>      Maximum tickets to show (defaults to 20 for --closed, unlimited otherwise)
      --json           Output as JSON

# Examples
janus ls                              # All open tickets
janus ls --ready                      # Tickets ready to work on
janus ls --blocked                    # Tickets blocked by dependencies
janus ls --closed                     # Recently closed tickets (limit 20)
janus ls --closed --limit 50          # Recently closed tickets (limit 50)
janus ls --status next                # Filter by specific status
janus ls --triaged false              # Untriaged tickets (status=new|next, triaged=false)
janus ls --triaged true               # Triaged tickets
janus ls --ready --blocked            # Show union of ready AND blocked tickets
janus ls --limit 10                   # Any tickets (limit 10)
```

#### `janus next` / `janus n`

Show next ticket(s) to work on with dependency-aware prioritization.

```bash
janus next [OPTIONS]

Options:
  -l, --limit <N>    Maximum tickets to show (default: 5)
      --json         Output as JSON
```

The `next` command analyzes dependencies and returns tickets in optimal work order:

- **Ready tickets** (no incomplete deps) sorted by priority
- **Blocking dependencies** shown before their dependents
- **Blocked tickets** included with blocking context

Examples:

```bash
# Show default 5 next tickets
janus next

# Show 10 next tickets
janus next --limit 10

# JSON output for scripting
janus next --json
```

Example JSON output:

```json
[
  {
    "id": "j-abc1",
    "priority": 0,
    "status": "ready",
    "title": "Fix critical bug",
    "reason": "ready"
  },
  {
    "id": "j-def2",
    "priority": 1,
    "status": "ready",
    "title": "Design OAuth flow",
    "reason": "blocking",
    "blocks": "j-ghi3"
  }
]
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
janus query '.type == "feature"'          # filter by type
```

### TUI Interfaces

#### `janus view`

Interactive issue browser with fuzzy search.

**Keyboard shortcuts:** `j/k` navigate, `g/G` top/bottom, `/` search, `e` edit, `s` cycle status, `n` new ticket, `Ctrl+T` triage mode, `Tab` switch pane, `q` quit

**Triage Mode:** Press `Ctrl+T` to toggle triage mode, which filters to show only untriaged tickets (status `new` or `next`, `triaged: false`). In triage mode:

| Key | Action |
|-----|--------|
| `t` | Mark ticket as triaged (sets `triaged: true`) |
| `c` | Cancel ticket (press twice to confirm) |
| `/` | Search/filter tickets |
| `j/k` | Navigate tickets |
| `q` | Quit triage mode (or press `Ctrl+T` again) |

Search and filter functionality remain fully available in triage mode.

#### `janus board`

Kanban board view organized by status.

**Keyboard shortcuts:** `h/l` move column, `j/k` move card, `/` search, `e` edit, `s/S` move right/left, `n` new ticket, `1-5` toggle column, `q` quit

### Remote Sync

#### `janus remote`

Manage remote issues (use --help for subcommands).

```bash
janus remote [COMMAND]

Commands:
  browse  Browse remote issues in TUI
  adopt   Import a remote issue and create a local ticket
  push    Push a local ticket to create a remote issue
  link    Link a local ticket to an existing remote issue
  sync    Sync a local ticket with its remote issue
```

**Note:** When `janus remote` is invoked without a subcommand in an interactive terminal, it launches the TUI browser (equivalent to `janus remote browse`).

#### `janus remote adopt`

Import a remote issue and create a local ticket.

```bash
janus remote adopt [OPTIONS] <REMOTE_REF>

Options:
      --prefix <PREFIX>  Custom prefix for ticket ID (e.g., 'perf' for 'perf-a982')
      --json             Output as JSON

# Examples
janus remote adopt github:owner/repo/123
janus remote adopt linear:org/PROJ-123
```

#### `janus remote push`

Push a local ticket to create a remote issue.

```bash
janus remote push [OPTIONS] <ID>

Options:
      --json   Output as JSON
```

#### `janus remote link`

Link a local ticket to an existing remote issue.

```bash
janus remote link [OPTIONS] <ID> <REMOTE_REF>

Options:
      --json   Output as JSON

# Example
janus remote link j-a1b2 github:myorg/myrepo/456
```

#### `janus remote sync`

Sync a local ticket with its remote issue.

```bash
janus remote sync [OPTIONS] <ID>

Options:
      --json   Output as JSON
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

### Shell Completions

#### `janus completions`

Generate shell completion scripts for bash, zsh, fish, and PowerShell.

```bash
janus completions <SHELL>

# Supported shells: bash, zsh, fish, powershell, elvish
```

#### Installation

**Bash**
```bash
# Add to ~/.bashrc
eval "$(janus completions bash)"

# Or save to file
janus completions bash > ~/.local/share/bash-completion/completions/janus
```

**Zsh**
```bash
# Add to ~/.zshrc (before compinit)
eval "$(janus completions zsh)"

# Or save to fpath
janus completions zsh > ~/.zfunc/_janus
# Then in ~/.zshrc: fpath=(~/.zfunc $fpath); autoload -Uz compinit; compinit
```

**Fish**
```bash
janus completions fish > ~/.config/fish/completions/janus.fish
```

**PowerShell**
```powershell
# Add to $PROFILE
Invoke-Expression (& janus completions powershell | Out-String)
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

## Plans

Plans organize tickets into larger goals with optional phases. They provide computed status tracking and progress summaries.

### Plan Types

- **Simple Plan**: A flat sequence of tickets
- **Phased Plan**: Tickets organized into sequential phases

### Creating Plans

```bash
# Create a simple plan
janus plan create "Q1 Feature Release"

# Create a phased plan
janus plan create "Database Migration" \
  --phase "Preparation" \
  --phase "Migration" \
  --phase "Validation"
```

### Managing Plan Tickets

```bash
# Add ticket to simple plan
janus plan add-ticket plan-a1b2 j-x1y2

# Add ticket to specific phase
janus plan add-ticket plan-a1b2 j-x1y2 --phase "Preparation"

# Remove ticket from plan
janus plan remove-ticket plan-a1b2 j-x1y2

# Move ticket between phases
janus plan move-ticket plan-a1b2 j-x1y2 --to-phase "Migration"
```

### Viewing Plans

```bash
# List all plans
janus plan ls

# Show plan details with progress
janus plan show plan-a1b2

# Show raw file content
janus plan show plan-a1b2 --raw

# Show plan status summary
janus plan status plan-a1b2

# Show next actionable items
janus plan next plan-a1b2
janus plan next plan-a1b2 --all    # Next for each phase
janus plan next plan-a1b2 --count 3
```

### Managing Phases

```bash
# Add a new phase
janus plan add-phase plan-a1b2 "Testing"

# Remove an empty phase
janus plan remove-phase plan-a1b2 "Testing"

# Remove phase with tickets (requires --force or --migrate)
janus plan remove-phase plan-a1b2 "Old Phase" --force
janus plan remove-phase plan-a1b2 "Old Phase" --migrate "New Phase"
```

### Plan Status

Plan status is **computed** from constituent ticket statuses:

| Condition | Status |
|-----------|--------|
| All tickets `complete` | `complete` |
| All tickets `cancelled` | `cancelled` |
| Mixed `complete`/`cancelled` | `complete` |
| All tickets `new` or `next` | `new` |
| Some started, some not | `in_progress` |

### Plan File Format

Plans are stored in `.janus/plans/` as Markdown with YAML frontmatter:

```markdown
---
id: plan-a1b2
uuid: 550e8400-e29b-41d4-a716-446655440000
created: 2024-01-01T00:00:00Z
---
# Database Migration Plan

Overview of the migration project.

## Acceptance Criteria

- Zero downtime during migration
- All data validated post-migration

## Phase 1: Preparation

Set up migration infrastructure.

### Tickets

1. j-prep1
2. j-prep2

## Phase 2: Migration

Execute the migration.

### Tickets

1. j-migrate1
```

Plans can also include free-form sections (e.g., `## Technical Details`, `## Open Questions`) that are preserved verbatim.

## Hooks

Hooks allow you to run custom scripts before or after Janus operations. This enables automation like syncing tickets with Git, sending notifications, enforcing workflows, or integrating with external tools.

### How Hooks Work

- **Pre-hooks**: Run before an operation. If a pre-hook exits with non-zero status, the operation is aborted.
- **Post-hooks**: Run after an operation completes. Failures are logged as warnings but don't abort anything.
- **Context**: Hooks receive information about the operation via environment variables.

### Hook Events

| Event | When It Fires |
|-------|---------------|
| `pre_write` | Before any ticket/plan write |
| `post_write` | After any ticket/plan write |
| `pre_delete` | Before a plan is deleted |
| `post_delete` | After a plan is deleted |
| `ticket_created` | After a new ticket is created |
| `ticket_updated` | After a ticket is modified |
| `ticket_deleted` | After a ticket is deleted |
| `plan_created` | After a new plan is created |
| `plan_updated` | After a plan is modified |
| `plan_deleted` | After a plan is deleted |

### Environment Variables

Hooks receive context via environment variables:

| Variable | Description |
|----------|-------------|
| `JANUS_EVENT` | The event name (e.g., `post_write`, `ticket_created`) |
| `JANUS_ITEM_TYPE` | Either `ticket` or `plan` |
| `JANUS_ITEM_ID` | The ticket or plan ID |
| `JANUS_FILE_PATH` | Path to the item's markdown file |
| `JANUS_ROOT` | Path to the `.janus/` directory |
| `JANUS_FIELD_NAME` | Field being modified (if applicable) |
| `JANUS_OLD_VALUE` | Previous field value (if applicable) |
| `JANUS_NEW_VALUE` | New field value (if applicable) |

### Configuring Hooks

Hooks are configured in `.janus/config.yaml`:

```yaml
hooks:
  enabled: true          # Enable/disable all hooks (default: true)
  timeout: 30            # Timeout in seconds (0 = no timeout, default: 30)
  scripts:
    # Map event names to script paths (relative to .janus/hooks/)
    pre_write: validate.sh
    post_write: post-write.sh
    ticket_created: notify-slack.sh
    plan_created: notify-team.sh
```

Hook scripts should be placed in `.janus/hooks/` and must be executable (`chmod +x`).

### Hook Commands

#### `janus hook list`

Show configured hooks and their status.

```bash
janus hook list [--json]

# Example output:
# Hooks: enabled
# Timeout: 30s
#
# Configured scripts:
#   post_write → post-write.sh
#   ticket_created → notify.sh
```

#### `janus hook enable` / `janus hook disable`

Enable or disable hooks globally.

```bash
janus hook enable
janus hook disable
```

#### `janus hook run`

Manually trigger a hook for testing.

```bash
janus hook run <EVENT> [--id <ITEM_ID>]

# Examples
janus hook run post_write --id j-a1b2
janus hook run ticket_created
```

#### `janus hook install`

Install a pre-built hook recipe from the Janus repository.

```bash
janus hook install <RECIPE>

# Example
janus hook install git-sync
```

### Writing Hook Scripts

Hook scripts are regular shell scripts. Here's an example that sends a Slack notification:

```bash
#!/usr/bin/env bash
# .janus/hooks/notify-slack.sh

# Only notify for ticket creation
if [ "$JANUS_EVENT" != "ticket_created" ]; then
    exit 0
fi

curl -X POST -H 'Content-type: application/json' \
    --data "{\"text\":\"New ticket created: $JANUS_ITEM_ID\"}" \
    "$SLACK_WEBHOOK_URL"
```

Make the script executable:

```bash
chmod +x .janus/hooks/notify-slack.sh
```

### Git Sync Recipe

The `git-sync` recipe automatically commits and pushes ticket changes to a Git remote, enabling team collaboration.

#### Installation

```bash
# Install the recipe
janus hook install git-sync

# Initialize with your remote repository
.janus/hooks/setup.sh git@github.com:yourorg/yourrepo-janus.git
```

#### What It Does

- **Auto-commit**: After any Janus write operation, changes are committed with a descriptive message
- **Auto-push**: Changes are pushed to the remote (fails silently if offline)
- **Selective sync**: Only `items/` and `plans/` are synced; hooks and config stay local

#### Manual Sync

To pull remote changes and push local changes:

```bash
.janus/hooks/sync.sh
```

#### What Gets Synced

| Directory | Synced? | Notes |
|-----------|---------|-------|
| `items/` | Yes | All tickets |
| `plans/` | Yes | All plans |
| `hooks/` | No | Each machine has its own |
| `config.yaml` | No | Contains local settings/tokens |

### Example: Validation Hook

Prevent tickets from being created without a description:

```bash
#!/usr/bin/env bash
# .janus/hooks/validate.sh

if [ "$JANUS_EVENT" != "pre_write" ]; then
    exit 0
fi

if [ "$JANUS_ITEM_TYPE" != "ticket" ]; then
    exit 0
fi

# Read the ticket file and check for description
if ! grep -q "## Description" "$JANUS_FILE_PATH"; then
    echo "Error: Tickets must have a ## Description section" >&2
    exit 1
fi
```

### Example: Logging Hook

Log all Janus operations to a file:

```bash
#!/usr/bin/env bash
# .janus/hooks/audit-log.sh

echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) $JANUS_EVENT $JANUS_ITEM_TYPE $JANUS_ITEM_ID" \
    >> "$JANUS_ROOT/audit.log"
```

## Tips

- **Partial IDs**: Only need first 2-3 characters of ticket ID (e.g., `j-a1` instead of `j-a1b2`)
- **Dependencies**: Use `janus dep tree` to visualize complex dependency chains
- **Aliases**: Common commands have short aliases (`janus c` for `create`, `janus s` for `show`, `janus e` for `edit`, `janus l` for `ls`)
- **TUI**: Use `janus view` for quick navigation and `janus board` for status management
- **Remote sync**: Use environment variables for sensitive credentials (`GITHUB_TOKEN`, `LINEAR_API_KEY`) instead of storing in config files
- **Bi-directional sync**: Run `janus remote sync` regularly when collaborating via GitHub/Linear to keep local and remote in sync
- **Short references**: once `default_remote` is set, use short formats (e.g., `ENG-123` for Linear instead of `linear:org/ENG-123`)
