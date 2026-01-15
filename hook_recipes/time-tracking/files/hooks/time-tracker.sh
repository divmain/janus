#!/usr/bin/env bash
#
# time-tracker.sh - Track time spent on tickets by logging status transitions
#
# Environment variables (provided by Janus):
#   JANUS_ROOT       - Path to the .janus directory
#   JANUS_TICKET_ID  - ID of the updated ticket
#   JANUS_FIELD_NAME - Name of the field that changed
#   JANUS_OLD_VALUE  - Previous value of the field
#   JANUS_NEW_VALUE  - New value of the field

set -euo pipefail

# Only track status changes
if [[ "${JANUS_FIELD_NAME:-}" != "status" ]]; then
    exit 0
fi

TRACKING_DIR="$JANUS_ROOT/.time-tracking"
TIME_LOG="$JANUS_ROOT/time-log.csv"
TICKET_ID="${JANUS_TICKET_ID:-}"
OLD_STATUS="${JANUS_OLD_VALUE:-}"
NEW_STATUS="${JANUS_NEW_VALUE:-}"

# Validate required variables
if [[ -z "$TICKET_ID" ]]; then
    echo "time-tracker: Missing JANUS_TICKET_ID" >&2
    exit 1
fi

# Ensure tracking directory exists
mkdir -p "$TRACKING_DIR"

# Initialize CSV with header if it doesn't exist
if [[ ! -f "$TIME_LOG" ]]; then
    echo "timestamp,ticket_id,old_status,new_status,duration_minutes" > "$TIME_LOG"
fi

START_FILE="$TRACKING_DIR/$TICKET_ID"
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Handle transition FROM in_progress
if [[ "$OLD_STATUS" == "in_progress" ]]; then
    if [[ -f "$START_FILE" ]]; then
        START_TIME=$(cat "$START_FILE")
        
        # Calculate duration in minutes
        if [[ "$(uname)" == "Darwin" ]]; then
            # macOS
            START_EPOCH=$(date -j -f "%Y-%m-%dT%H:%M:%SZ" "$START_TIME" +%s 2>/dev/null || echo "")
            NOW_EPOCH=$(date +%s)
        else
            # Linux
            START_EPOCH=$(date -d "$START_TIME" +%s 2>/dev/null || echo "")
            NOW_EPOCH=$(date +%s)
        fi
        
        if [[ -n "$START_EPOCH" ]]; then
            DURATION_SECONDS=$((NOW_EPOCH - START_EPOCH))
            DURATION_MINUTES=$((DURATION_SECONDS / 60))
            
            # Log the transition with duration
            echo "$TIMESTAMP,$TICKET_ID,$OLD_STATUS,$NEW_STATUS,$DURATION_MINUTES" >> "$TIME_LOG"
        else
            # Could not parse start time, log without duration
            echo "$TIMESTAMP,$TICKET_ID,$OLD_STATUS,$NEW_STATUS," >> "$TIME_LOG"
            echo "time-tracker: Warning - could not parse start time for $TICKET_ID" >&2
        fi
        
        # Remove the start time file
        rm -f "$START_FILE"
    else
        # No start time recorded, log without duration
        echo "$TIMESTAMP,$TICKET_ID,$OLD_STATUS,$NEW_STATUS," >> "$TIME_LOG"
        echo "time-tracker: Warning - no start time recorded for $TICKET_ID" >&2
    fi
fi

# Handle transition TO in_progress
if [[ "$NEW_STATUS" == "in_progress" ]]; then
    # Record the start time
    echo "$TIMESTAMP" > "$START_FILE"
fi
