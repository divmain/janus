#!/bin/bash
# check-blocked.sh - Check for tickets blocked longer than threshold
#
# Usage: ./check-blocked.sh [hours]
#   hours - Threshold in hours (default: 48)
#
# Exit codes:
#   0 - No long-blocked tickets found
#   1 - One or more tickets blocked longer than threshold
#
# Output format suitable for alerting systems

set -e

# Default threshold: 48 hours
THRESHOLD_HOURS="${1:-48}"
THRESHOLD_SECONDS=$((THRESHOLD_HOURS * 3600))

# Find .janus directory
JANUS_ROOT=""
if [[ -d ".janus" ]]; then
    JANUS_ROOT=".janus"
elif [[ -d "../.janus" ]]; then
    JANUS_ROOT="../.janus"
else
    # Search up the directory tree
    dir="$PWD"
    while [[ "$dir" != "/" ]]; do
        if [[ -d "$dir/.janus" ]]; then
            JANUS_ROOT="$dir/.janus"
            break
        fi
        dir="$(dirname "$dir")"
    done
fi

if [[ -z "$JANUS_ROOT" ]]; then
    echo "Error: Could not find .janus directory" >&2
    exit 2
fi

TRACKING_DIR="$JANUS_ROOT/.blocked-tracking"

if [[ ! -d "$TRACKING_DIR" ]]; then
    # No tracking directory means no blocked tickets tracked yet
    exit 0
fi

NOW=$(date +%s)
blocked_tickets=()

# Check each tracked ticket
for tracking_file in "$TRACKING_DIR"/*; do
    [[ -f "$tracking_file" ]] || continue
    
    ticket_id=$(basename "$tracking_file")
    blocked_since=$(cat "$tracking_file")
    
    blocked_duration=$((NOW - blocked_since))
    
    if [[ $blocked_duration -ge $THRESHOLD_SECONDS ]]; then
        blocked_hours=$((blocked_duration / 3600))
        blocked_tickets+=("$ticket_id:$blocked_hours")
    fi
done

# Output results
if [[ ${#blocked_tickets[@]} -eq 0 ]]; then
    exit 0
fi

echo "BLOCKED TICKETS (> $THRESHOLD_HOURS hours):"
for entry in "${blocked_tickets[@]}"; do
    ticket_id="${entry%%:*}"
    hours="${entry##*:}"
    echo "- $ticket_id: blocked for $hours hours"
done

exit 1
