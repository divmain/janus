# Discord Notify Hook Recipe

Send Discord notifications when important ticket events occur in your Janus issue tracker.

## What It Does

This recipe posts rich embed messages to a Discord channel when:

- **New tickets are created** - Blue embed showing ticket ID, type, and priority
- **Tickets are completed** - Green embed celebrating the completion
- **New plans are created** - Purple embed announcing the new plan

## Setup

### 1. Create a Discord Webhook

1. Open your Discord server settings
2. Go to **Integrations** > **Webhooks**
3. Click **New Webhook**
4. Choose the channel for notifications
5. Copy the webhook URL

### 2. Set Environment Variable

Add to your shell profile (`~/.bashrc`, `~/.zshrc`, etc.):

```bash
export DISCORD_WEBHOOK_URL="https://discord.com/api/webhooks/YOUR_WEBHOOK_ID/YOUR_WEBHOOK_TOKEN"
```

Or set it in your CI/CD environment variables.

### 3. Install the Recipe

```bash
janus hook install discord-notify
```

## Example Messages

### New Ticket Created

```
+------------------------------------------+
|  New Ticket Created                      |
|  ----------------------------------------|
|  Fix login timeout on slow connections   |
|                                          |
|  ID: j-a1b2    Type: bug    Priority: P1 |
|                                          |
|  Janus Issue Tracker         2024-01-15  |
+------------------------------------------+
```

### Ticket Completed

```
+------------------------------------------+
|  Ticket Completed                        |
|  ----------------------------------------|
|  Add user avatar upload feature          |
|                                          |
|  ID: j-c3d4    Type: feature  Priority: P2|
|                                          |
|  Janus Issue Tracker         2024-01-15  |
+------------------------------------------+
```

### New Plan Created

```
+------------------------------------------+
|  New Plan Created                        |
|  ----------------------------------------|
|  Q1 2024 Performance Improvements        |
|                                          |
|  Plan ID: plan-e5f6                      |
|                                          |
|  Janus Issue Tracker         2024-01-15  |
+------------------------------------------+
```

## Customization

### Change Embed Colors

Edit the color constants in `discord-notify.sh`:

```bash
COLOR_BLUE=3447003      # #3498db - New ticket
COLOR_GREEN=3066993     # #2ecc71 - Completed
COLOR_PURPLE=10181046   # #9b59b6 - New plan
```

Use decimal values (convert hex with: `echo $((16#3498db))`)

### Add More Events

To notify on other events, add cases to the `send_notification()` function:

```bash
ticket_status_changed)
    # Custom handling for status changes
    ;;
```

### Add Mentions

To mention a role or user, add to the payload:

```bash
local payload=$(cat <<EOF
{
    "content": "<@&ROLE_ID> New ticket needs attention!",
    "embeds": [{
        ...
    }]
}
EOF
)
```

### Filter by Priority

To only notify for high-priority tickets, add a check:

```bash
ticket_created)
    local priority=$(get_frontmatter_field "$item_path" "priority")
    if [[ "${priority:-2}" -gt 1 ]]; then
        exit 0  # Skip P2 and lower
    fi
    ...
```

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `DISCORD_WEBHOOK_URL` | Yes | Discord webhook URL |
| `JANUS_HOOK_EVENT` | Auto | Event type (set by Janus) |
| `JANUS_ITEM_PATH` | Auto | Path to ticket/plan file |
| `JANUS_TICKET_ID` | Auto | Ticket ID (ticket events) |
| `JANUS_TICKET_STATUS` | Auto | Current ticket status |
| `JANUS_PLAN_ID` | Auto | Plan ID (plan events) |

## Troubleshooting

**No notifications appearing?**
- Verify `DISCORD_WEBHOOK_URL` is set: `echo $DISCORD_WEBHOOK_URL`
- Check webhook is valid: `curl -X POST -H "Content-Type: application/json" -d '{"content":"Test"}' "$DISCORD_WEBHOOK_URL"`
- Ensure script is executable: `chmod +x .janus/hooks/discord-notify.sh`

**Rate limiting?**
- Discord webhooks are limited to 30 messages per minute
- Consider batching notifications for bulk operations
