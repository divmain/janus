# Hooks

Hooks allow you to run custom scripts before or after Janus operations. This enables automation like syncing tickets with Git, sending notifications, enforcing workflows, or integrating with external tools.

## How Hooks Work

- **Pre-hooks**: Run before an operation. If a pre-hook exits with non-zero status, the operation is aborted.
- **Post-hooks**: Run after an operation completes. Failures are logged as warnings but don't abort anything.
- **Context**: Hooks receive information about the operation via environment variables.

## Hook Events

| Event | When It Fires |
|-------|---------------|
| `pre_write` | Before any ticket/plan write |
| `post_write` | After any ticket/plan write |
| `pre_delete` | Before a ticket/plan is deleted |
| `post_delete` | After a ticket/plan is deleted |
| `ticket_created` | After a new ticket is created |
| `ticket_updated` | After a ticket is modified |
| `plan_created` | After a new plan is created |
| `plan_updated` | After a plan is modified |
| `plan_deleted` | After a plan is deleted |

## Environment Variables

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

## Configuring Hooks

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

## Hook Commands

### `janus hook list`

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

### `janus hook enable` / `janus hook disable`

Enable or disable hooks globally.

```bash
janus hook enable
janus hook disable
```

### `janus hook run`

Manually trigger a hook for testing.

```bash
janus hook run <EVENT> [--id <ITEM_ID>]

# Examples
janus hook run post_write --id j-a1b2
janus hook run ticket_created
```

### `janus hook install`

Install a pre-built hook recipe from the Janus repository.

```bash
janus hook install <RECIPE>

# Example
janus hook install git-sync
```

### `janus hook log`

View hook failure logs.

```bash
janus hook log
janus hook log --lines 10
janus hook log --json
```

## Writing Hook Scripts

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

## Example: Validation Hook

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

## Example: Logging Hook

Log all Janus operations to a file:

```bash
#!/usr/bin/env bash
# .janus/hooks/audit-log.sh

echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) $JANUS_EVENT $JANUS_ITEM_TYPE $JANUS_ITEM_ID" \
    >> "$JANUS_ROOT/audit.log"
```

## Git Sync Recipe

The `git-sync` recipe automatically commits and pushes ticket changes to a Git remote, enabling team collaboration.

### Installation

```bash
# Install the recipe
janus hook install git-sync

# Initialize with your remote repository
.janus/hooks/setup.sh git@github.com:yourorg/yourrepo-janus.git
```

### What It Does

- **Auto-commit**: After any Janus write operation, changes are committed with a descriptive message
- **Auto-push**: Changes are pushed to the remote (fails silently if offline)
- **Selective sync**: Only `items/` and `plans/` are synced; hooks and config stay local

### Manual Sync

To pull remote changes and push local changes:

```bash
.janus/hooks/sync.sh
```

### What Gets Synced

| Directory | Synced? | Notes |
|-----------|---------|-------|
| `items/` | Yes | All tickets |
| `plans/` | Yes | All plans |
| `hooks/` | No | Each machine has its own |
| `config.yaml` | No | Contains local settings/tokens |
