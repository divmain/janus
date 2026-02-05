# Blocked Alert Hook Recipe

Alerts when tickets have been blocked for too long. Useful for identifying stalled work and ensuring blockers get attention.

## What It Does

This recipe tracks tickets that become blocked and provides a check script to identify tickets blocked longer than a configurable threshold.

## How Blocking Is Detected

A ticket is considered "blocked" when:

1. **It has dependencies** - The `deps` array is non-empty
2. **Status is `new` or `next`** - The ticket is not actively being worked on

Tickets with status `in_progress` are not considered blocked, even if they have dependencies. The assumption is that if someone is actively working on a ticket, they're either working around the blocker or the dependency is no longer actually blocking.

Tracking is cleared when a ticket reaches `complete` or `cancelled` status.

## Installation

```bash
janus hook install blocked-alert
```

## Usage

### Tracking (Automatic)

The `track-blocked.sh` hook runs automatically on every ticket update. It:

- Creates tracking files in `.janus/.blocked-tracking/` when tickets become blocked
- Removes tracking files when tickets are completed, cancelled, or unblocked
- Preserves the original "blocked since" timestamp if a ticket remains blocked

### Checking for Long-Blocked Tickets

Run the check script manually or via cron:

```bash
# Check with default threshold (48 hours)
./check-blocked.sh

# Check with custom threshold (24 hours)
./check-blocked.sh 24

# Check with longer threshold (1 week = 168 hours)
./check-blocked.sh 168
```

### Exit Codes

- `0` - No tickets blocked longer than threshold
- `1` - One or more tickets blocked longer than threshold
- `2` - Error (e.g., could not find .janus directory)

### Output Format

```
BLOCKED TICKETS (> 48 hours):
- j-a1b2: blocked for 72 hours
- j-c3d4: blocked for 50 hours
```

## Example Cron Job Setup

Add to your crontab (`crontab -e`):

```bash
# Check for blocked tickets every morning at 9am
0 9 * * * cd /path/to/project && .janus/hooks/check-blocked.sh 48 | mail -s "Blocked Tickets Alert" team@example.com

# Check every 4 hours and log to file
0 */4 * * * cd /path/to/project && .janus/hooks/check-blocked.sh 24 >> /var/log/janus-blocked.log 2>&1
```

## Integrating with Alerting Systems

### Slack via Webhook

```bash
#!/bin/bash
output=$(.janus/hooks/check-blocked.sh 48)
if [[ $? -eq 1 ]]; then
    curl -X POST -H 'Content-type: application/json' \
        --data "{\"text\":\"$output\"}" \
        "$SLACK_WEBHOOK_URL"
fi
```

### PagerDuty

```bash
#!/bin/bash
if ! .janus/hooks/check-blocked.sh 72 > /dev/null 2>&1; then
    # Trigger PagerDuty alert for tickets blocked > 3 days
    curl -X POST https://events.pagerduty.com/v2/enqueue \
        -H 'Content-Type: application/json' \
        -d '{
            "routing_key": "YOUR_ROUTING_KEY",
            "event_action": "trigger",
            "payload": {
                "summary": "Tickets blocked for more than 72 hours",
                "severity": "warning",
                "source": "janus"
            }
        }'
fi
```

### GitHub Actions

```yaml
name: Check Blocked Tickets
on:
  schedule:
    - cron: '0 9 * * *'  # Daily at 9am UTC

jobs:
  check-blocked:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Check for blocked tickets
        run: |
          chmod +x .janus/hooks/check-blocked.sh
          .janus/hooks/check-blocked.sh 48
```

## Data Storage

Tracking data is stored in `.janus/.blocked-tracking/`:

```
.janus/.blocked-tracking/
├── j-a1b2    # Contains Unix timestamp when ticket became blocked
├── j-c3d4
└── j-e5f6
```

Each file contains a single Unix timestamp (seconds since epoch) indicating when the ticket was first detected as blocked.

## Troubleshooting

### No alerts even though tickets are blocked

1. Ensure the hook is properly installed and configured
2. Check that tickets have dependencies (`deps` field)
3. Verify ticket status is `new` or `next`
4. Run `ls .janus/.blocked-tracking/` to see tracked tickets

### Incorrect blocked duration

The tracked time is when the ticket *became* blocked (dependencies added while status was new/next), not when it was created. If a ticket's dependencies change frequently, the timer resets each time it becomes unblocked and re-blocked.
