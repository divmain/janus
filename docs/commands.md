# CLI Commands Reference

Complete reference for all Janus CLI commands.

## Ticket Management

### `janus create` / `janus c`

Create a new ticket.

```bash
janus create "Ticket title" [OPTIONS]

Options:
  -d, --description <TEXT>    Description text
      --design <TEXT>         Design notes
      --acceptance <TEXT>     Acceptance criteria
  -p, --priority <0-4>        Priority level (default: 2, 0 = highest)
  -t, --type <TYPE>           Type: bug, feature, task, epic, chore (default: task)
      --size <SIZE>           Size estimate: xsmall, small, medium, large, xlarge (or xs, s, m, l, xl)
      --external-ref <REF>    External reference (e.g., gh-123)
      --parent <ID>           Parent ticket ID
      --prefix <PREFIX>       Custom prefix for ticket ID (e.g., 'perf' for 'perf-a982')
      --spawned-from <ID>     ID of ticket this was spawned from (decomposition tracking)
      --spawn-context <TEXT>  Context explaining why this ticket was spawned
```

### `janus show` / `janus s`

Display ticket details with dependencies, links, and relationships.

```bash
janus show <ID>
```

ID can be partial - first few unique characters are sufficient.

### `janus edit` / `janus e`

Open ticket in `$EDITOR` for manual editing.

```bash
janus edit <ID>
janus edit <ID> --json    # Print file path as JSON without opening editor
```

### `janus add-note`

Add a timestamped note to a ticket.

```bash
janus add-note <ID> [NOTE_TEXT]
```

If no note text provided, reads from stdin.

### `janus set`

Update any ticket field without opening an editor.

```bash
janus set <ID> <FIELD> <VALUE>

# Supported fields:
janus set <ID> priority <0-4>           # Update priority
janus set <ID> type <TYPE>              # Update type (bug/feature/task/epic/chore)
janus set <ID> size <SIZE>              # Update size (xs/s/m/l/xl)
janus set <ID> parent <ID>              # Update parent ticket
janus set <ID> external-ref <REF>       # Update external reference
janus set <ID> description <TEXT>       # Update description section
janus set <ID> design <TEXT>            # Update design notes section
janus set <ID> acceptance <TEXT>        # Update acceptance criteria section
```

## Status Management

### `janus start`

Mark ticket as in-progress.

```bash
janus start <ID>
```

### `janus close`

Mark ticket as complete or cancelled.

```bash
janus close <ID> [OPTIONS]

Options:
      --summary <TEXT>     Add completion summary when closing
      --no-summary         Close without adding a summary
      --cancel             Mark as cancelled instead of complete

# Examples
janus close j-a1b2 --summary "Implemented OAuth flow successfully"
janus close j-a1b2 --no-summary
janus close j-a1b2 --cancel
```

### `janus reopen`

Reopen a closed ticket.

```bash
janus reopen <ID>
```

### `janus status`

Set ticket to any status.

```bash
janus status <ID> <STATUS>

Valid statuses: new, next, in_progress, complete, cancelled
```

## Dependencies

### `janus dep add`

Add a dependency - the second ticket must be completed before the first.

```bash
janus dep add <ID> <DEP_ID>

# Example: Feature j-1234 depends on task j-5678
janus dep add j-1234 j-5678
```

### `janus dep remove`

Remove a dependency.

```bash
janus dep remove <ID> <DEP_ID>
```

### `janus dep tree`

Show dependency tree for a ticket.

```bash
janus dep tree <ID> [--full]

# --full shows all nodes including duplicates
```

## Links

### `janus link add`

Link tickets together (bidirectional relationship).

```bash
janus link add <ID1> <ID2> [ID3 ...]

# Example: Link related tickets
janus link add j-1234 j-5678 j-9012
```

### `janus link remove`

Remove a link between tickets.

```bash
janus link remove <ID1> <ID2>
```

## Decomposition (Spawning)

Track hierarchical ticket relationships - breaking down large tickets into smaller subtasks.

### Key Concepts

- **spawned-from**: Track the parent ticket that spawned this one
- **spawn-context**: Document why the ticket was created
- **depth**: Auto-computed decomposition level (0 = root, 1 = direct children, etc.)

### Creating Spawned Tickets

```bash
# Create a subtask with spawning metadata
janus create "Implement OAuth endpoints" \
  --spawned-from j-parent \
  --spawn-context "Breaking down authentication feature into smaller tasks"

# Or use the parent flag which also sets spawned-from
janus create "Subtask" --parent j-parent
```

### Querying Spawned Tickets

```bash
janus ls --spawned_from j-a1b2    # Direct children
janus ls --depth 0                 # Root tickets only
janus ls --depth 1                 # Direct children
janus ls --max-depth 2             # Up to 2 levels deep
janus graph --spawn --root j-a1b2  # Visualize relationships
```

## Listing and Querying

### `janus ls` / `janus l`

List tickets with optional filters.

```bash
janus ls [OPTIONS]

Options:
      --ready              Show tickets ready to work on (no incomplete deps, status=new|next)
      --blocked            Show tickets with incomplete dependencies
      --closed             Show recently closed/cancelled tickets
      --all                Include closed/cancelled tickets in output
      --active             Show only active tickets (exclude closed/cancelled)
      --status <STATUS>    Filter by specific status
      --triaged <BOOL>     Filter by triage status (true|false)
      --size <SIZE>        Filter by size (can specify multiple: --size small,medium)
      --spawned_from <ID>  Filter to show only tickets spawned from parent
      --depth <N>          Show tickets at specific decomposition depth (0 = root tickets)
      --max-depth <N>      Show tickets up to specified depth
      --limit <N>          Maximum tickets to show (defaults to 20 for --closed, unlimited otherwise)
      --sort_by <FIELD>    Sort by: priority (default), created, id
      --json               Output as JSON

# Examples
janus ls                              # All open tickets
janus ls --ready                      # Tickets ready to work on
janus ls --blocked                    # Tickets blocked by dependencies
janus ls --closed                     # Recently closed tickets (limit 20)
janus ls --closed --limit 50          # Recently closed tickets (limit 50)
janus ls --status next                # Filter by specific status
janus ls --triaged false              # Untriaged tickets (status=new|next, triaged=false)
janus ls --ready --blocked            # Show union of ready AND blocked tickets
janus ls --depth 0                    # Root tickets only
janus ls --spawned_from j-a1b2        # Direct children of j-a1b2
janus ls --sort_by created            # Sort by creation date
```

### `janus next` / `janus n`

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

Example output:

```
ID          Priority  Status     Title                          Reason
--------    --------  -------    -----------------------------  -----------------
j-abc1      P0        ready      Fix critical bug               ready
j-def2      P1        ready      Design OAuth flow              blocking j-ghi3
j-ghi3      P1        blocked    Implement OAuth flow           target (1 dep)
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

### `janus query`

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

### `janus search`

Search tickets using semantic similarity (requires `semantic-search` feature).

```bash
janus search <QUERY> [OPTIONS]

Options:
  -l, --limit <N>         Maximum results to return (default: 10)
      --threshold <0-1>   Minimum similarity threshold
      --json              Output as JSON

# Examples
janus search "authentication problems"
janus search "performance issues" --limit 5
janus search "database errors" --threshold 0.7
janus search "user login" --json
```

Unlike regular text search, semantic search matches by meaning. "authentication problems" will find tickets about "login failures" or "OAuth errors" even without those exact words.

See [Semantic Search Guide](semantic-search.md) for details.

### `janus graph`

Visualize ticket relationships as a graph.

```bash
janus graph [OPTIONS]

Options:
      --spawn          Show spawning (parent/child) relationships instead of dependencies
      --root <ID>      Show graph starting from a specific ticket
      --plan <ID>      Graph all tickets in a plan
      --format <FMT>   Output format: dot (default) or mermaid

# Examples
janus graph                               # Dependency graph in DOT format
janus graph --spawn                       # Spawning graph (parent/child)
janus graph --root j-a1b2                 # Subtree from j-a1b2
janus graph --plan plan-a1b2              # Graph all tickets in a plan
janus graph --format mermaid              # Mermaid format for diagrams
```

### `janus doctor`

Health check - scan all tickets for parsing errors or corruption.

```bash
janus doctor
janus doctor --json    # Output as JSON
```

## Configuration

### `janus config set`

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

### `janus config get`

Get a configuration value.

```bash
janus config get <KEY>

# Valid keys: github.token, linear.api_key, default_remote
```

### `janus config show`

Display current configuration.

```bash
janus config show
```

## Cache Management

See [Cache Guide](cache.md) for details.

```bash
janus cache           # Show cache status
janus cache clear     # Clear cache for current repo
janus cache rebuild   # Force full cache rebuild
janus cache path      # Show cache file location
```

## Shell Completions

### `janus completions`

Generate shell completion scripts.

```bash
janus completions <SHELL>

# Supported shells: bash, zsh, fish, powershell, elvish
```

### Installation

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

## Command Aliases

| Full Command | Alias |
|--------------|-------|
| `janus create` | `janus c` |
| `janus show` | `janus s` |
| `janus edit` | `janus e` |
| `janus ls` | `janus l` |
| `janus next` | `janus n` |
