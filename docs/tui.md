# TUI Interfaces

Janus includes two interactive terminal interfaces for browsing and managing tickets.

## Issue Browser (`janus view`)

A two-pane interface with a ticket list on the left and ticket details on the right.

```bash
janus view
```

### Navigation

| Key | Action |
|-----|--------|
| `j` / `Down` | Move down in list |
| `k` / `Up` | Move up in list |
| `g` | Go to top of list |
| `G` | Go to bottom of list |
| `Tab` | Switch focus between list and detail pane |
| `PageUp` / `PageDown` | Scroll in detail view |

### Search and Filter

| Key | Action |
|-----|--------|
| `/` | Enter search mode |
| `Esc` | Exit search mode |

Type to filter tickets by title, ID, or content.

### Ticket Actions

| Key | Action |
|-----|--------|
| `e` | Edit ticket inline |
| `n` | Create new ticket |
| `s` | Cycle status forward |
| `y` | Copy ticket ID to clipboard |

### Triage Mode

Press `Ctrl+T` to toggle triage mode, which filters to show only untriaged tickets (status `new` or `next`, `triaged: false`).

| Key | Action |
|-----|--------|
| `Ctrl+T` | Toggle triage mode |
| `t` | Mark ticket as triaged |
| `c` | Cancel ticket (press twice to confirm) |
| `n` | Add note to ticket |

### Quit

| Key | Action |
|-----|--------|
| `q` | Quit |
| `Ctrl+Q` | Quit (works in all modes) |

## Kanban Board (`janus board`)

A column-based view organized by ticket status.

```bash
janus board
```

### Column Layout

The board displays five columns by default:
1. **NEW** - Newly created tickets
2. **NEXT** - Ready to work on soon
3. **IN PROGRESS** - Currently being worked on
4. **COMPLETE** - Finished tickets
5. **CANCELLED** - No longer relevant

### Navigation

| Key | Action |
|-----|--------|
| `h` / `Left` | Move to column on left |
| `l` / `Right` | Move to column on right |
| `j` / `Down` | Move down in current column |
| `k` / `Up` | Move up in current column |

### Column Visibility

| Key | Action |
|-----|--------|
| `1` | Toggle NEW column |
| `2` | Toggle NEXT column |
| `3` | Toggle IN PROGRESS column |
| `4` | Toggle COMPLETE column |
| `5` | Toggle CANCELLED column |

### Ticket Actions

| Key | Action |
|-----|--------|
| `s` | Move ticket to next status (right) |
| `S` | Move ticket to previous status (left) |
| `e` | Edit ticket |
| `n` | Create new ticket |

### Search

| Key | Action |
|-----|--------|
| `/` | Enter search mode |
| `Esc` | Exit search mode |

### Quit

| Key | Action |
|-----|--------|
| `q` | Quit |

## Tips

- Use `janus view` for quick navigation and detailed ticket inspection
- Use `janus board` for visual status management and workflow tracking
- Both interfaces support inline editing with `e`
- Search works in both interfaces with `/`
- The TUI benefits significantly from the cache - see [Cache Guide](cache.md)
