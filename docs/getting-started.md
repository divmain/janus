# Getting Started with Janus

This guide walks you through your first project with Janus, from creating tickets to tracking progress.

## Prerequisites

- Janus installed (see [Installation](../README.md#installation))
- A terminal with a Unix-like shell (Linux, macOS, or WSL)

## Your First Ticket

Create a ticket with:

```bash
janus create "Fix login bug" \
  --type bug \
  --priority 1 \
  --size small \
  --description "Users cannot login after password reset"
```

Janus outputs a ticket ID like `j-a1b2`. Use this ID (or just the first few characters) to reference the ticket.

## Basic Workflow

### View Ticket Details

```bash
janus show j-a1b2
```

### Start Working

```bash
janus start j-a1b2
```

This sets the status to `in_progress`.

### Mark Complete

```bash
janus close j-a1b2
```

### List Tickets

```bash
janus ls              # All open tickets
janus ls --ready      # Ready to work on
janus ls --blocked    # Blocked by dependencies
```

## Building a Project

### 1. Create a High-Level Epic

```bash
janus create "User authentication system" \
  --type epic \
  --priority 1 \
  --description "Implement OAuth2 login with Google and GitHub providers"
```

Save the output ID (e.g., `j-a1b2`) as your parent ticket.

### 2. Break Down Into Tasks

Create child tickets using `--parent`:

```bash
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

### 3. Add Dependencies

Some tasks must complete before others can start:

```bash
# Create a design task
janus create "Design OAuth flow diagrams" --priority 0
# Output: j-x1y2

# Make implementation depend on design
janus dep add j-x3y4 j-x1y2
```

Now `j-x3y4` is blocked until `j-x1y2` is complete.

### 4. Track Progress

Add notes as you work:

```bash
janus add-note j-x3y4 "Started implementing Google OAuth flow"
janus add-note j-x3y4 "Completed initial auth code exchange endpoint"
```

### 5. View Blocked and Ready Tickets

```bash
janus ls --blocked    # Waiting for dependencies
janus ls --ready      # Ready to work on
```

### 6. Find Next Work

Get a prioritized, dependency-aware list:

```bash
janus next            # Show next 5 tickets
janus next --limit 10 # Show more
```

Output shows ready tickets first, then blocking dependencies, then blocked tickets:

```
ID          Priority  Status     Title                          Reason
--------    --------  -------    -----------------------------  -----------------
j-abc1      P0        ready      Fix critical bug               ready
j-def2      P1        ready      Design OAuth flow              blocking j-ghi3
j-ghi3      P1        blocked    Implement OAuth flow           target (1 dep)
```

## Using the TUI

Janus includes two interactive interfaces:

### Issue Browser

```bash
janus view
```

- `j/k` - Navigate up/down
- `/` - Search/filter
- `e` - Edit ticket inline
- `n` - Create new ticket
- `s` - Cycle status
- `Tab` - Switch between list and detail pane
- `q` - Quit

### Kanban Board

```bash
janus board
```

- `h/l` - Move between columns
- `j/k` - Navigate within column
- `s/S` - Move ticket right/left (change status)
- `1-5` - Toggle column visibility
- `q` - Quit

See [TUI Guide](tui.md) for full keyboard shortcuts.

## Linking Related Tickets

For tickets that are related but don't have dependency relationships:

```bash
janus link add j-x3y4 j-x4y5
```

Links are bidirectional and appear in `janus show` output.

## Querying Tickets

Export tickets as JSON with optional filters:

```bash
janus query                              # All tickets
janus query '.status != "complete"'      # Open tickets
janus query '.priority <= 1 and .type == "bug"'  # High-priority bugs
```

## Next Steps

- [Remote Sync](remote-sync.md) - Sync with GitHub Issues or Linear
- [Plans](plans.md) - Organize tickets into larger goals
- [TUI Guide](tui.md) - Master the interactive interfaces
- [Commands Reference](commands.md) - Full CLI reference
