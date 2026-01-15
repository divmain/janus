#!/bin/bash
# track-blocked.sh - Track when tickets become blocked
# Called on: ticket_updated
#
# A ticket is considered "blocked" if:
# - It has dependencies (deps array is non-empty)
# - Status is "new" or "next" (not actively being worked on)
#
# Environment variables from Janus:
#   JANUS_ROOT - Path to .janus directory
#   JANUS_TICKET_ID - The ticket ID
#   JANUS_TICKET_STATUS - Current status
#   JANUS_TICKET_DEPS - Comma-separated list of dependencies

set -e

TRACKING_DIR="$JANUS_ROOT/.blocked-tracking"

# Create tracking directory if it doesn't exist
mkdir -p "$TRACKING_DIR"

TRACKING_FILE="$TRACKING_DIR/$JANUS_TICKET_ID"

# Check if ticket should be removed from tracking (completed or cancelled)
if [[ "$JANUS_TICKET_STATUS" == "complete" || "$JANUS_TICKET_STATUS" == "cancelled" ]]; then
    if [[ -f "$TRACKING_FILE" ]]; then
        rm "$TRACKING_FILE"
    fi
    exit 0
fi

# Check if ticket is blocked:
# - Has dependencies
# - Status is new or next (not in_progress)
is_blocked=false

if [[ -n "$JANUS_TICKET_DEPS" && "$JANUS_TICKET_DEPS" != "[]" ]]; then
    if [[ "$JANUS_TICKET_STATUS" == "new" || "$JANUS_TICKET_STATUS" == "next" ]]; then
        is_blocked=true
    fi
fi

if [[ "$is_blocked" == "true" ]]; then
    # Only create tracking file if it doesn't exist (preserve original blocked time)
    if [[ ! -f "$TRACKING_FILE" ]]; then
        echo "$(date +%s)" > "$TRACKING_FILE"
    fi
else
    # Not blocked - remove tracking if exists
    if [[ -f "$TRACKING_FILE" ]]; then
        rm "$TRACKING_FILE"
    fi
fi
