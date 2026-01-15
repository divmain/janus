# Daily Digest Hook Recipe

Automatically log ticket and plan activity to generate daily standup digests.

## What It Does

This recipe:

1. **Logs activity** - Captures ticket and plan events to a JSON Lines file
2. **Generates digests** - Produces markdown summaries grouped by date and activity type

## Installation

```bash
# Copy hooks to your .janus directory
cp -r files/hooks/* /path/to/repo/.janus/hooks/

# Copy config or merge with existing
cp config.yaml /path/to/repo/.janus/hooks.yaml
```

## Usage

### Automatic Logging

Once installed, activity is automatically logged when:
- Tickets are created
- Tickets are updated (status changes, field edits)
- Plans are created

### Generating a Digest

```bash
# Generate digest for the last day (default)
./generate-digest.sh

# Generate digest for the last 3 days
./generate-digest.sh 3

# Generate digest for the last week
./generate-digest.sh 7
```

## JSON Lines Format

Activity is logged to `.janus/activity-log.jsonl` in [JSON Lines](https://jsonlines.org/) format - one JSON object per line:

```jsonl
{"timestamp":"2024-01-15T10:30:00Z","event":"ticket_created","item_type":"ticket","item_id":"j-a1b2","item_path":"/path/to/.janus/items/j-a1b2.md"}
{"timestamp":"2024-01-15T11:45:00Z","event":"ticket_updated","item_type":"ticket","item_id":"j-a1b2","field":"status","old_value":"new","new_value":"in_progress"}
{"timestamp":"2024-01-15T14:20:00Z","event":"ticket_updated","item_type":"ticket","item_id":"j-a1b2","field":"status","old_value":"in_progress","new_value":"complete"}
```

### Fields

| Field | Description |
|-------|-------------|
| `timestamp` | ISO8601 UTC timestamp |
| `event` | Event type: `ticket_created`, `ticket_updated`, `plan_created` |
| `item_type` | `ticket` or `plan` |
| `item_id` | The item's ID (e.g., `j-a1b2`) |
| `item_path` | Full filesystem path to the item |
| `field` | Field that changed (updates only) |
| `old_value` | Previous value (updates only) |
| `new_value` | New value (updates only) |

## Example Digest Output

```markdown
# Activity Digest

_Generated: 2024-01-15 09:00_
_Period: Last 1 day(s) (since 2024-01-14)_

## Activity for 2024-01-15

### Created
- **j-x9y8**: Add user authentication
- **j-z7w6**: Fix login redirect bug

### Completed
- **j-a1b2**: Implement password reset (complete)
- **j-c3d4**: Update email templates (complete)

### In Progress
- **j-e5f6**: Refactor session handling

### Other Updates
- **j-g7h8**: Update documentation (priority updated)
```

## Tips for Standup Workflow

### Morning Digest

Add to your shell profile or create an alias:

```bash
alias standup='cd /path/to/repo && .janus/hooks/generate-digest.sh'
```

### Weekly Review

Generate a week's digest for sprint reviews:

```bash
./generate-digest.sh 7 > weekly-digest.md
```

### Filtering with jq

The JSON Lines format works well with `jq`:

```bash
# Count events by type
cat .janus/activity-log.jsonl | jq -s 'group_by(.event) | map({event: .[0].event, count: length})'

# Find all status changes to complete
cat .janus/activity-log.jsonl | jq 'select(.new_value == "complete")'

# Get activity for a specific ticket
cat .janus/activity-log.jsonl | jq 'select(.item_id == "j-a1b2")'
```

### Log Rotation

For long-running projects, consider rotating the log:

```bash
# Archive old entries (keep last 30 days)
mv .janus/activity-log.jsonl .janus/activity-log-$(date +%Y%m).jsonl
```

## Environment Variables

The logging hook uses these janus environment variables:

| Variable | Description |
|----------|-------------|
| `JANUS_ROOT` | Path to `.janus` directory |
| `JANUS_EVENT` | Event type |
| `JANUS_ITEM_TYPE` | Item type (ticket/plan) |
| `JANUS_ITEM_ID` | Item ID |
| `JANUS_ITEM_PATH` | Full path to item file |
| `JANUS_FIELD` | Changed field (updates) |
| `JANUS_OLD_VALUE` | Previous value (updates) |
| `JANUS_NEW_VALUE` | New value (updates) |
