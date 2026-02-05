# Plans

Plans organize tickets into larger goals with optional phases. They provide computed status tracking and progress summaries.

## Plan Types

- **Simple Plan**: A flat sequence of tickets
- **Phased Plan**: Tickets organized into sequential phases

## Creating Plans

```bash
# Create a simple plan
janus plan create "Q1 Feature Release"

# Create a phased plan
janus plan create "Database Migration" \
  --phase "Preparation" \
  --phase "Migration" \
  --phase "Validation"
```

## Managing Plan Tickets

### Adding Tickets

```bash
# Add ticket to simple plan
janus plan add-ticket plan-a1b2 j-x1y2

# Add ticket to specific phase
janus plan add-ticket plan-a1b2 j-x1y2 --phase "Preparation"

# Add at specific position
janus plan add-ticket plan-a1b2 j-x1y2 --position 3
janus plan add-ticket plan-a1b2 j-x1y2 --after j-xyz9
```

### Removing Tickets

```bash
janus plan remove-ticket plan-a1b2 j-x1y2
```

### Moving Tickets

```bash
# Move ticket between phases
janus plan move-ticket plan-a1b2 j-x1y2 --to-phase "Migration"

# Move to specific position
janus plan move-ticket plan-a1b2 j-x1y2 --to-phase "Migration" --position 3
janus plan move-ticket plan-a1b2 j-x1y2 --to-phase "Migration" --after j-xyz9
```

## Viewing Plans

```bash
# List all plans
janus plan ls

# Show plan details with progress
janus plan show plan-a1b2

# Show raw file content
janus plan show plan-a1b2 --raw

# Show only ticket list
janus plan show plan-a1b2 --tickets-only

# Show only phase summary
janus plan show plan-a1b2 --phases-only

# Show full completion summaries for a phase
janus plan show plan-a1b2 --verbose-phase "Phase 1"

# Show plan status summary
janus plan status plan-a1b2

# Show next actionable items
janus plan next plan-a1b2
janus plan next plan-a1b2 --all      # Next for each phase
janus plan next plan-a1b2 --count 3
```

## Managing Phases

### Adding Phases

```bash
# Add a new phase at the end
janus plan add-phase plan-a1b2 "Testing"

# Add phase at specific position
janus plan add-phase plan-a1b2 "Testing" --after "Phase 1"
janus plan add-phase plan-a1b2 "Testing" --position 2
```

### Removing Phases

```bash
# Remove an empty phase
janus plan remove-phase plan-a1b2 "Testing"

# Remove phase with tickets (requires --force or --migrate)
janus plan remove-phase plan-a1b2 "Old Phase" --force
janus plan remove-phase plan-a1b2 "Old Phase" --migrate "New Phase"
```

### Reordering

```bash
# Interactive reorder of tickets in a phase
janus plan reorder plan-a1b2 --phase "Phase 1"

# Reorder phases themselves
janus plan reorder plan-a1b2 --reorder-phases
```

## Other Plan Commands

```bash
# Delete a plan
janus plan delete plan-a1b2
janus plan delete plan-a1b2 --force

# Rename a plan
janus plan rename plan-a1b2 "New Title"

# Edit plan in $EDITOR
janus plan edit plan-a1b2

# Validate all plan files
janus plan verify
janus plan verify --json

# View import format specification
janus plan import-spec
```

## Plan Status

Plan status is **computed** from constituent ticket statuses (never stored):

| Condition | Status |
|-----------|--------|
| All tickets `complete` | `complete` |
| All tickets `cancelled` | `cancelled` |
| Mixed `complete`/`cancelled` | `complete` |
| All tickets `new` or `next` | `new` |
| Some started, some not | `in_progress` |

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

### Section Types

- **Structured sections**: `## Acceptance Criteria`, `## Tickets`, `## Phase N: Name` are parsed into data structures
- **Free-form sections**: Any other H2 (e.g., `## Overview`, `## Technical Details`) are preserved verbatim

## Visualizing Plans

```bash
# Graph all tickets in a plan
janus graph --plan plan-a1b2

# Output as Mermaid diagram
janus graph --plan plan-a1b2 --format mermaid
```
