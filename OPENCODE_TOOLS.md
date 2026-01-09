# OpenCode Tools Integration Plan

This document outlines the implementation plan for integrating Janus with OpenCode through custom tools.

## Overview

Janus will expose its functionality to OpenCode agents through custom tools placed in `.opencode-staging/tool/`. Once implementation is complete and manually verified, these tools can be moved to `.opencode/tool/` for production use. The tools shell out to the `janus` CLI with `--json` flags for structured output, enabling agents to create, manage, and query tickets and plans programmatically.

## Implementation Phases

### Phase 0: Janus CLI Enhancements

Before building OpenCode tools, the following changes must be made to Janus itself to ensure consistent, complete CLI support.

#### 0.1 Standardize JSON Flag

Refactor commands that use `--format json` to use `--json` for consistency:

| Command | Current | Target |
|---------|---------|--------|
| `janus plan show` | `--format json` | `--json` |
| `janus plan ls` | `--format json` | `--json` |

#### 0.2 Add Ticket Field Update Command

Currently, only `status` can be updated via CLI (`janus status`). Add a general-purpose field update command:

```
janus set <ticket_id> <field> <value>
```

Supported fields:
- `priority` - 0-4
- `type` - bug, feature, task, epic, chore
- `parent` - ticket ID or empty string to clear

The command should:
- Accept `--json` flag for structured output
- Validate field values before updating
- Return the updated ticket metadata

#### 0.3 Remove Assignee References

Remove all `assignee` field references from:
- CLI arguments in `janus create`
- Any existing tool designs
- Documentation

---

### Phase 1: Ticket Tools (`janus_ticket.ts`)

**File:** `.opencode-staging/tool/janus_ticket.ts`

#### `janus_ticket_create`

Create a new ticket.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `title` | string | Yes | Ticket title |
| `description` | string | No | Description text |
| `type` | enum | No | bug, feature, task, epic, chore |
| `priority` | number | No | 0-4 (0=highest) |
| `parent` | string | No | Parent ticket ID |
| `design` | string | No | Design notes |
| `acceptance` | string | No | Acceptance criteria |
| `prefix` | string | No | Custom ID prefix |

**CLI:** `janus create <title> [--description ...] [--type ...] [--priority ...] [--parent ...] [--design ...] [--acceptance ...] [--prefix ...] --json`

#### `janus_ticket_get`

Get ticket details with computed relationships.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `id` | string | Yes | Ticket ID (can be partial) |

**CLI:** `janus show <id> --json`

#### `janus_ticket_update`

Update ticket fields.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `id` | string | Yes | Ticket ID (can be partial) |
| `status` | enum | No | new, next, in_progress, complete, cancelled |
| `priority` | number | No | 0-4 |
| `type` | enum | No | bug, feature, task, epic, chore |
| `parent` | string | No | Parent ticket ID (empty to clear) |

**CLI:** 
- Status: `janus status <id> <status> --json`
- Other fields: `janus set <id> <field> <value> --json` (requires Phase 0.2)

#### `janus_ticket_list`

List tickets with filters.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `ready` | boolean | No | Show ready tickets (no incomplete deps) |
| `blocked` | boolean | No | Show blocked tickets |
| `closed` | boolean | No | Show closed/cancelled tickets |
| `all` | boolean | No | Include closed in results |
| `status` | enum | No | Filter by specific status |
| `limit` | number | No | Maximum tickets to return |

**CLI:** `janus ls [--ready] [--blocked] [--closed] [--all] [--status ...] [--limit ...] --json`

#### `janus_ticket_add_note`

Add a timestamped note to a ticket.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `id` | string | Yes | Ticket ID (can be partial) |
| `note` | string | Yes | Note text |

**CLI:** `janus add-note <id> <note> --json`

#### `janus_ticket_search`

Search tickets by field values.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `status` | enum | No | Filter by status |
| `type` | enum | No | Filter by type |
| `priority` | number | No | Filter by priority |
| `title_contains` | string | No | Filter by title substring |

**CLI:** `janus query` (returns all tickets as JSON lines)
**Post-processing:** Filter in TypeScript based on provided criteria.

---

### Phase 2: Plan Tools (`janus_plan.ts`)

**File:** `.opencode-staging/tool/janus_plan.ts`

#### `janus_plan_create`

Create a new plan.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `title` | string | Yes | Plan title |
| `phases` | string[] | No | Phase names (creates phased plan if provided) |

**CLI:** `janus plan create <title> [--phase <name>...] --json`

#### `janus_plan_get`

Get plan details with computed status and progress.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `id` | string | Yes | Plan ID (can be partial) |
| `tickets_only` | boolean | No | Return only ticket list |
| `phases_only` | boolean | No | Return only phase summary |

**CLI:** `janus plan show <id> [--tickets-only] [--phases-only] --json`

#### `janus_plan_list`

List all plans.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `status` | enum | No | Filter by computed status |

**CLI:** `janus plan ls [--status ...] --json`

#### `janus_plan_update`

Rename a plan.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `id` | string | Yes | Plan ID (can be partial) |
| `new_title` | string | Yes | New plan title |

**CLI:** `janus plan rename <id> <new_title> --json`

#### `janus_plan_delete`

Delete a plan.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `id` | string | Yes | Plan ID (can be partial) |

**CLI:** `janus plan delete <id> --force --json`

#### `janus_plan_add_ticket`

Add a ticket to a plan.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `plan_id` | string | Yes | Plan ID |
| `ticket_id` | string | Yes | Ticket ID to add |
| `phase` | string | No | Target phase (required for phased plans) |
| `after` | string | No | Insert after this ticket |
| `position` | number | No | Insert at position (1-indexed) |

**CLI:** `janus plan add-ticket <plan_id> <ticket_id> [--phase ...] [--after ...] [--position ...] --json`

#### `janus_plan_remove_ticket`

Remove a ticket from a plan.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `plan_id` | string | Yes | Plan ID |
| `ticket_id` | string | Yes | Ticket ID to remove |

**CLI:** `janus plan remove-ticket <plan_id> <ticket_id> --json`

#### `janus_plan_move_ticket`

Move a ticket between phases.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `plan_id` | string | Yes | Plan ID |
| `ticket_id` | string | Yes | Ticket ID to move |
| `to_phase` | string | Yes | Target phase |
| `after` | string | No | Insert after this ticket |
| `position` | number | No | Insert at position (1-indexed) |

**CLI:** `janus plan move-ticket <plan_id> <ticket_id> --to-phase <phase> [--after ...] [--position ...] --json`

#### `janus_plan_add_phase`

Add a phase to a plan.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `plan_id` | string | Yes | Plan ID |
| `phase_name` | string | Yes | Name for new phase |
| `after` | string | No | Insert after this phase |
| `position` | number | No | Insert at position (1-indexed) |

**CLI:** `janus plan add-phase <plan_id> <phase_name> [--after ...] [--position ...] --json`

#### `janus_plan_remove_phase`

Remove a phase from a plan.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `plan_id` | string | Yes | Plan ID |
| `phase` | string | Yes | Phase name or number |
| `force` | boolean | No | Force removal even if phase has tickets |
| `migrate_to` | string | No | Move tickets to this phase before removing |

**CLI:** `janus plan remove-phase <plan_id> <phase> [--force] [--migrate ...] --json`

#### `janus_plan_next`

Get next actionable items from a plan.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `id` | string | Yes | Plan ID |
| `count` | number | No | Number of items to return (default: 1) |
| `all_phases` | boolean | No | Show next from all incomplete phases |

**CLI:** `janus plan next <id> [--count ...] [--all] --json`

#### `janus_plan_status`

Get plan status summary with phase breakdown.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `id` | string | Yes | Plan ID |

**CLI:** `janus plan status <id> --json`

#### `janus_plan_import`

Import a plan from markdown content, creating tickets.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `content` | string | Yes | Markdown content to import |
| `title` | string | No | Override extracted title |
| `type` | enum | No | Ticket type for created tasks |
| `prefix` | string | No | Custom ticket ID prefix |
| `dry_run` | boolean | No | Validate only, don't create |

**CLI:** `echo "<content>" | janus plan import - [--title ...] [--type ...] [--prefix ...] [--dry-run] --json`

#### `janus_plan_import_spec`

Get the importable plan format specification.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| (none) | | | |

**CLI:** `janus plan import-spec`

**Returns:** The format specification as a string for the agent to reference when generating importable plans.

---

### Phase 3: Dependency Tools (`janus_deps.ts`)

**File:** `.opencode-staging/tool/janus_deps.ts`

#### `janus_deps_add`

Add a dependency to a ticket.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `ticket_id` | string | Yes | The dependent ticket |
| `depends_on` | string | Yes | Ticket that must be completed first |

**CLI:** `janus dep add <ticket_id> <depends_on> --json`

#### `janus_deps_remove`

Remove a dependency from a ticket.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `ticket_id` | string | Yes | The dependent ticket |
| `depends_on` | string | Yes | Dependency to remove |

**CLI:** `janus dep remove <ticket_id> <depends_on> --json`

#### `janus_deps_tree`

Get the dependency tree for a ticket.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `ticket_id` | string | Yes | Root ticket |
| `full` | boolean | No | Show all occurrences (including duplicates) |

**CLI:** `janus dep tree <ticket_id> [--full] --json`

#### `janus_deps_link`

Create bidirectional links between tickets.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `ticket_ids` | string[] | Yes | Two or more ticket IDs to link |

**CLI:** `janus link add <id1> <id2> [<id3>...] --json`

#### `janus_deps_unlink`

Remove bidirectional link between two tickets.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `ticket_id_1` | string | Yes | First ticket |
| `ticket_id_2` | string | Yes | Second ticket |

**CLI:** `janus link remove <id1> <id2> --json`

---

### Phase 4: Bulk Tools (`janus_bulk.ts`)

**File:** `.opencode-staging/tool/janus_bulk.ts`

#### `janus_bulk_update_status`

Update status of multiple tickets.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `ticket_ids` | string[] | Yes | Array of ticket IDs |
| `status` | enum | Yes | new, next, in_progress, complete, cancelled |

**Implementation:** Loop over `ticket_ids`, call `janus status <id> <status> --json` for each.

**Returns:** `{ succeeded: string[], failed: { id: string, error: string }[] }`

#### `janus_bulk_add_deps`

Add multiple dependencies to a ticket.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `ticket_id` | string | Yes | The dependent ticket |
| `depends_on` | string[] | Yes | Array of tickets it depends on |

**Implementation:** Loop over `depends_on`, call `janus dep add <ticket_id> <dep> --json` for each.

**Returns:** `{ succeeded: string[], failed: { id: string, error: string }[] }`

#### `janus_bulk_create_tickets`

Create multiple tickets at once.

| Arg | Type | Required | Description |
|-----|------|----------|-------------|
| `tickets` | object[] | Yes | Array of ticket specifications |

Each ticket object:
```typescript
{
  title: string
  description?: string
  type?: "bug" | "feature" | "task" | "epic" | "chore"
  priority?: 0 | 1 | 2 | 3 | 4
  parent?: string
}
```

**Implementation:** Loop over `tickets`, call `janus create ... --json` for each.

**Returns:** `{ created: { id: string, title: string }[], failed: { title: string, error: string }[] }`

---

## Tool Descriptions

Each tool's `description` field should be a single concise sentence. Examples:

| Tool | Description |
|------|-------------|
| `janus_ticket_create` | Create a new ticket with title, description, and optional metadata. |
| `janus_ticket_get` | Get ticket details including status, dependencies, and relationships. |
| `janus_ticket_update` | Update ticket fields such as status, priority, or type. |
| `janus_ticket_list` | List tickets with optional filters for status, readiness, or blocked state. |
| `janus_ticket_add_note` | Add a timestamped note to a ticket. |
| `janus_ticket_search` | Search tickets by field values like status, type, or title. |
| `janus_plan_create` | Create a new simple or phased plan. |
| `janus_plan_get` | Get plan details with computed status and progress. |
| `janus_plan_list` | List all plans with optional status filter. |
| `janus_plan_update` | Rename a plan. |
| `janus_plan_delete` | Delete a plan (does not delete contained tickets). |
| `janus_plan_add_ticket` | Add a ticket to a plan or phase. |
| `janus_plan_remove_ticket` | Remove a ticket from a plan. |
| `janus_plan_move_ticket` | Move a ticket between phases in a phased plan. |
| `janus_plan_add_phase` | Add a new phase to a plan. |
| `janus_plan_remove_phase` | Remove a phase from a plan. |
| `janus_plan_next` | Get the next actionable items from a plan. |
| `janus_plan_status` | Get plan status summary with phase breakdown. |
| `janus_plan_import` | Import a plan from markdown, creating tickets automatically. |
| `janus_plan_import_spec` | Get the format specification for importable plan documents. |
| `janus_deps_add` | Add a dependency (ticket A depends on ticket B). |
| `janus_deps_remove` | Remove a dependency between tickets. |
| `janus_deps_tree` | Get the dependency tree for a ticket. |
| `janus_deps_link` | Create bidirectional links between tickets. |
| `janus_deps_unlink` | Remove a bidirectional link between tickets. |
| `janus_bulk_update_status` | Update status of multiple tickets at once. |
| `janus_bulk_add_deps` | Add multiple dependencies to a ticket. |
| `janus_bulk_create_tickets` | Create multiple tickets at once. |

---

## Error Handling

All tools follow a consistent error handling pattern:

```typescript
async execute(args) {
  try {
    const result = await Bun.$`janus ${cmd} --json`.text()
    return JSON.parse(result)
  } catch (e: any) {
    // Janus CLI returns human-readable errors to stderr
    throw new Error(e.stderr?.trim() || e.message || "Unknown error")
  }
}
```

Errors bubble up to the agent, which can then decide how to proceed.

---

## Manual Testing Guide

OpenCode does not provide a direct mechanism to invoke custom tools outside of an LLM conversation. Use the following approaches to test tools:

### 1. CLI Testing with `opencode run`

```bash
# Test ticket creation
opencode run "Use the janus_ticket_create tool to create a ticket titled 'Test ticket' with type 'task'"

# Test ticket listing
opencode run "Use the janus_ticket_list tool to show all ready tickets"

# Test plan creation
opencode run "Use the janus_plan_create tool to create a phased plan titled 'Test Plan' with phases 'Phase 1' and 'Phase 2'"
```

### 2. Interactive Testing with TUI

1. Start OpenCode: `opencode`
2. Enable verbose tool output: `/details`
3. Prompt the agent to use specific tools explicitly
4. Observe tool execution in the output

### 3. Log File Inspection

Check logs at `~/.local/share/opencode/log/` for detailed execution traces.

### 4. Unit Testing Tool Logic

Since tools are TypeScript, the `execute` function can be tested directly:

```typescript
// Example test
import { describe, it, expect } from 'bun:test'
import ticketTools from './.opencode-staging/tool/janus_ticket'

describe('janus_ticket_create', () => {
  it('should create a ticket', async () => {
    const result = await ticketTools.create.execute(
      { title: 'Test ticket', type: 'task' },
      { agent: 'test', sessionID: 'test', messageID: 'test' }
    )
    expect(result.id).toMatch(/^j-/)
  })
})
```

### Testing Checklist

For each tool, verify:

- [ ] Tool executes successfully with required args only
- [ ] Tool executes successfully with all optional args
- [ ] Tool returns expected JSON structure
- [ ] Tool throws descriptive error on invalid input
- [ ] Tool throws descriptive error when janus command fails

---

## Implementation Checklist

### Phase 0: Janus CLI Enhancements
- [x] Refactor `janus plan show` from `--format json` to `--json`
- [x] Refactor `janus plan ls` from `--format json` to `--json`
- [x] Add `janus set <id> <field> <value> --json` command
- [x] Remove assignee references from CLI and codebase
- [x] Run `cargo test` to ensure all tests pass
- [ ] Run `cargo clippy` to ensure no warnings

### Phase 1: Ticket Tools
- [x] Create `.opencode-staging/tool/janus_ticket.ts`
- [x] Implement `create` export
- [x] Implement `get` export
- [x] Implement `update` export
- [x] Implement `list` export
- [x] Implement `add_note` export
- [x] Implement `search` export
- [ ] Manual test all ticket tools

### Phase 2: Plan Tools
- [x] Create `.opencode-staging/tool/janus_plan.ts`
- [x] Implement `create` export
- [x] Implement `get` export
- [x] Implement `list` export
- [x] Implement `update` export
- [x] Implement `delete` export
- [x] Implement `add_ticket` export
- [x] Implement `remove_ticket` export
- [x] Implement `move_ticket` export
- [x] Implement `add_phase` export
- [x] Implement `remove_phase` export
- [x] Implement `next` export
- [x] Implement `status` export
- [x] Implement `import` export
- [x] Implement `import_spec` export
- [ ] Manual test all plan tools

### Phase 3: Dependency Tools
- [x] Create `.opencode-staging/tool/janus_deps.ts`
- [x] Implement `add` export
- [x] Implement `remove` export
- [x] Implement `tree` export
- [x] Implement `link` export
- [x] Implement `unlink` export
- [ ] Manual test all dependency tools

### Phase 4: Bulk Tools
- [x] Create `.opencode-staging/tool/janus_bulk.ts`
- [x] Implement `update_status` export
- [x] Implement `add_deps` export
- [x] Implement `create_tickets` export
- [ ] Manual test all bulk tools

---

## File Summary

| File | Tools | Est. Lines |
|------|-------|------------|
| `.opencode-staging/tool/janus_ticket.ts` | 6 | ~150 |
| `.opencode-staging/tool/janus_plan.ts` | 14 | ~350 |
| `.opencode-staging/tool/janus_deps.ts` | 5 | ~100 |
| `.opencode-staging/tool/janus_bulk.ts` | 3 | ~100 |
| **Total** | **28** | **~700** |
