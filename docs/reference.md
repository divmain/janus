# Reference

Quick reference for Janus concepts, file formats, and conventions.

## Ticket Statuses

| Status | Description |
|--------|-------------|
| `new` | Newly created ticket, not yet prioritized |
| `next` | High-priority ticket ready to work on soon |
| `in_progress` | Currently being worked on |
| `complete` | Finished and tested |
| `cancelled` | No longer relevant or applicable |

## Ticket Types

| Type | Description |
|------|-------------|
| `bug` | Defect or error that needs fixing |
| `feature` | New functionality or enhancement |
| `task` | Specific work item |
| `epic` | Large feature broken into smaller tickets |
| `chore` | Maintenance or housekeeping task |

## Priority Levels

| Priority | Description |
|----------|-------------|
| P0 | Critical, must fix immediately |
| P1 | High priority, address soon |
| P2 | Default priority |
| P3 | Low priority |
| P4 | Nice to have |

## Ticket Sizes

Size estimates for ticket complexity (optional field):

| Size | Alias | Description |
|------|-------|-------------|
| `xsmall` | `xs` | Trivial change, minutes to complete |
| `small` | `s` | Quick task, a few hours |
| `medium` | `m` | Standard task, 1-2 days |
| `large` | `l` | Significant effort, 3-5 days |
| `xlarge` | `xl` | Major undertaking, a week or more |

## ID Formats

### Ticket IDs

Format: `<prefix>-<4-char-hash>`

Examples:
- `j-a1b2` (default prefix)
- `perf-x9z3` (custom prefix via `--prefix`)

Partial IDs work - use just the first few unique characters (e.g., `j-a1` instead of `j-a1b2`).

### Plan IDs

Format: `plan-<4-char-hash>`

Example: `plan-a1b2`

## Ticket File Format

Tickets are stored as Markdown files in `.janus/items/` with YAML frontmatter:

```markdown
---
id: j-a1b2
status: new
type: feature
priority: 1
size: medium
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

### Frontmatter Fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Unique ticket identifier |
| `status` | string | Current status |
| `type` | string | Ticket type |
| `priority` | number | 0-4, lower is higher priority |
| `size` | string | Complexity estimate |
| `created` | datetime | Creation timestamp |
| `deps` | array | IDs of tickets this depends on |
| `links` | array | IDs of related tickets |
| `parent` | string | Parent ticket ID |
| `remote` | string | Remote issue reference |
| `external_ref` | string | External reference |
| `spawned_from` | string | Parent ticket in decomposition |
| `spawn_context` | string | Why this was spawned |
| `triaged` | boolean | Whether ticket has been triaged |

### Body Sections

Common sections in the ticket body:

- `## Description` - Main description
- `## Design Notes` - Technical design details
- `## Acceptance Criteria` - Definition of done
- `## Notes` - Timestamped notes (auto-managed by `add-note`)

## Plan File Format

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

### Plan Frontmatter Fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Unique plan identifier |
| `uuid` | string | UUID for external references |
| `created` | datetime | Creation timestamp |

### Plan Section Types

- **Structured**: `## Acceptance Criteria`, `## Tickets`, `## Phase N: Name` are parsed
- **Free-form**: Any other H2 sections are preserved verbatim

## Directory Structure

```
.janus/
├── items/           # Ticket files (*.md)
├── plans/           # Plan files (*.md)
├── hooks/           # Hook scripts
├── embeddings/      # Embedding cache (*.bin files)
└── config.yaml      # Configuration
```

### Store Architecture

Janus uses an in-memory store (DashMap) for fast queries:
- **No database**: Store is rebuilt from Markdown files on process start
- **Embeddings**: Stored as `.bin` files in `.janus/embeddings/`, keyed by `blake3(file_path + ":" + mtime_ns)`
- **Filesystem watcher**: Live updates for long-running processes (TUI, MCP server)

## Command Aliases

| Full Command | Alias |
|--------------|-------|
| `janus create` | `janus c` |
| `janus show` | `janus s` |
| `janus edit` | `janus e` |
| `janus ls` | `janus l` |
| `janus next` | `janus n` |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `GITHUB_TOKEN` | GitHub personal access token |
| `LINEAR_API_KEY` | Linear API key |
| `EDITOR` | Editor for `janus edit` commands |

## Remote Reference Formats

| Platform | Format | Example |
|----------|--------|---------|
| GitHub | `github:org/repo/number` | `github:myorg/myrepo/123` |
| Linear | `linear:org/ID` | `linear:myorg/ENG-456` |
