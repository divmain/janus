#!/usr/bin/env bash
#
# discord-notify.sh - Send Discord notifications for Janus events
#
# Environment variables:
#   DISCORD_WEBHOOK_URL - Required. Discord webhook URL
#   JANUS_HOOK_EVENT    - The event type (ticket_created, ticket_updated, plan_created)
#   JANUS_ITEM_PATH     - Path to the ticket/plan markdown file
#   JANUS_TICKET_ID     - Ticket ID (for ticket events)
#   JANUS_TICKET_STATUS - Ticket status (for ticket_updated)
#   JANUS_PLAN_ID       - Plan ID (for plan events)

set -euo pipefail

# Exit gracefully if webhook URL is not configured
if [[ -z "${DISCORD_WEBHOOK_URL:-}" ]]; then
    echo "DISCORD_WEBHOOK_URL not set, skipping Discord notification" >&2
    exit 0
fi

# Extract title from markdown file (first H1 heading)
get_title() {
    local file="$1"
    if [[ -f "$file" ]]; then
        grep -m1 '^# ' "$file" 2>/dev/null | sed 's/^# //' || echo "Untitled"
    else
        echo "Untitled"
    fi
}

# Extract field from YAML frontmatter
get_frontmatter_field() {
    local file="$1"
    local field="$2"
    if [[ -f "$file" ]]; then
        sed -n '/^---$/,/^---$/p' "$file" | grep "^${field}:" | sed "s/^${field}: *//" | tr -d '"' || echo ""
    else
        echo ""
    fi
}

# Discord embed colors (decimal values)
COLOR_BLUE=3447003      # #3498db - New ticket
COLOR_GREEN=3066993     # #2ecc71 - Completed
COLOR_PURPLE=10181046   # #9b59b6 - New plan

# Build and send Discord message based on event type
send_notification() {
    local event="${JANUS_HOOK_EVENT:-unknown}"
    local item_path="${JANUS_ITEM_PATH:-}"
    local title=""
    local embed_title=""
    local embed_color=""
    local fields=""
    
    case "$event" in
        ticket_created)
            local ticket_id="${JANUS_TICKET_ID:-unknown}"
            title=$(get_title "$item_path")
            local ticket_type=$(get_frontmatter_field "$item_path" "type")
            local priority=$(get_frontmatter_field "$item_path" "priority")
            
            embed_title="New Ticket Created"
            embed_color=$COLOR_BLUE
            fields=$(cat <<EOF
[
    {"name": "ID", "value": "\`${ticket_id}\`", "inline": true},
    {"name": "Type", "value": "${ticket_type:-task}", "inline": true},
    {"name": "Priority", "value": "P${priority:-2}", "inline": true}
]
EOF
)
            ;;
        
        ticket_updated)
            local ticket_id="${JANUS_TICKET_ID:-unknown}"
            local status="${JANUS_TICKET_STATUS:-}"
            title=$(get_title "$item_path")
            
            # Only send notification for completed tickets
            if [[ "$status" != "complete" ]]; then
                exit 0
            fi
            
            local ticket_type=$(get_frontmatter_field "$item_path" "type")
            local priority=$(get_frontmatter_field "$item_path" "priority")
            
            embed_title="Ticket Completed"
            embed_color=$COLOR_GREEN
            fields=$(cat <<EOF
[
    {"name": "ID", "value": "\`${ticket_id}\`", "inline": true},
    {"name": "Type", "value": "${ticket_type:-task}", "inline": true},
    {"name": "Priority", "value": "P${priority:-2}", "inline": true}
]
EOF
)
            ;;
        
        plan_created)
            local plan_id="${JANUS_PLAN_ID:-unknown}"
            title=$(get_title "$item_path")
            
            embed_title="New Plan Created"
            embed_color=$COLOR_PURPLE
            fields=$(cat <<EOF
[
    {"name": "Plan ID", "value": "\`${plan_id}\`", "inline": true}
]
EOF
)
            ;;
        
        *)
            echo "Unknown event type: $event" >&2
            exit 0
            ;;
    esac
    
    # Build the Discord webhook payload
    local payload=$(cat <<EOF
{
    "embeds": [{
        "title": "${embed_title}",
        "description": "${title}",
        "color": ${embed_color},
        "fields": ${fields},
        "footer": {
            "text": "Janus Issue Tracker"
        },
        "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    }]
}
EOF
)
    
    # Send to Discord
    curl -s -X POST \
        -H "Content-Type: application/json" \
        -d "$payload" \
        "$DISCORD_WEBHOOK_URL" > /dev/null
    
    echo "Discord notification sent for ${event}: ${title}"
}

send_notification
