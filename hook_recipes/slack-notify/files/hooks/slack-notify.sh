#!/bin/sh
set -e

# Slack notification hook for Janus
# Sends notifications on ticket and plan events
#
# Required environment variable:
#   SLACK_WEBHOOK_URL - Your Slack incoming webhook URL
#
# Janus provides these environment variables:
#   JANUS_EVENT      - Event type (ticket_created, ticket_updated, plan_created)
#   JANUS_ITEM_ID    - The ticket or plan ID
#   JANUS_ITEM_TYPE  - "ticket" or "plan"
#   JANUS_FILE_PATH  - Path to the item file
#   JANUS_FIELD_NAME - Field that was modified (for updates)
#   JANUS_OLD_VALUE  - Previous value (for updates)
#   JANUS_NEW_VALUE  - New value (for updates)

# Exit gracefully if webhook URL is not configured
if [ -z "$SLACK_WEBHOOK_URL" ]; then
    exit 0
fi

# Helper function to extract title from ticket/plan file
get_title() {
    if [ -f "$1" ]; then
        # Extract the first H1 heading from the markdown file
        grep -m1 '^# ' "$1" 2>/dev/null | sed 's/^# //' || echo "Untitled"
    else
        echo "Untitled"
    fi
}

# Build the Slack message based on event type
case "$JANUS_EVENT" in
    ticket_created)
        TITLE=$(get_title "$JANUS_FILE_PATH")
        COLOR="#2196F3"  # Blue
        MESSAGE="New ticket created: *${JANUS_ITEM_ID}* - ${TITLE}"
        ;;
    
    ticket_updated)
        # Only notify on status changes to complete
        if [ "$JANUS_FIELD_NAME" = "status" ] && [ "$JANUS_NEW_VALUE" = "complete" ]; then
            COLOR="#4CAF50"  # Green
            MESSAGE="Ticket completed: *${JANUS_ITEM_ID}*"
        else
            # Skip notification for other updates
            exit 0
        fi
        ;;
    
    plan_created)
        TITLE=$(get_title "$JANUS_FILE_PATH")
        COLOR="#2196F3"  # Blue
        MESSAGE="New plan created: *${JANUS_ITEM_ID}* - ${TITLE}"
        ;;
    
    *)
        # Unknown event, skip
        exit 0
        ;;
esac

# Send the Slack notification
curl -s -X POST "$SLACK_WEBHOOK_URL" \
    -H "Content-Type: application/json" \
    -d "{
        \"attachments\": [{
            \"color\": \"${COLOR}\",
            \"text\": \"${MESSAGE}\",
            \"mrkdwn_in\": [\"text\"]
        }]
    }" > /dev/null

exit 0
