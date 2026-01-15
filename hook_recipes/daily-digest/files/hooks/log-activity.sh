#!/usr/bin/env bash
# Log activity to activity-log.jsonl for daily digest generation
#
# This script is called by janus hooks for:
#   - ticket_created
#   - ticket_updated
#   - plan_created
#
# Environment variables available:
#   JANUS_ROOT       - Path to .janus directory
#   JANUS_EVENT      - Event type (ticket_created, ticket_updated, plan_created)
#   JANUS_ITEM_TYPE  - Type of item (ticket, plan)
#   JANUS_ITEM_ID    - ID of the item
#   JANUS_ITEM_PATH  - Full path to the item file
#   JANUS_FIELD      - Field that changed (for updates)
#   JANUS_OLD_VALUE  - Previous value (for updates)
#   JANUS_NEW_VALUE  - New value (for updates)

set -euo pipefail

LOG_FILE="${JANUS_ROOT}/activity-log.jsonl"

# Get current timestamp in ISO8601 format
timestamp=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Build JSON object with available data
# Using printf to ensure proper JSON escaping
json_escape() {
    local str="$1"
    # Escape backslashes, quotes, and control characters
    printf '%s' "$str" | sed 's/\\/\\\\/g; s/"/\\"/g; s/	/\\t/g' | tr -d '\n'
}

# Start building JSON
json="{"
json+="\"timestamp\":\"${timestamp}\""
json+=",\"event\":\"${JANUS_EVENT:-}\""
json+=",\"item_type\":\"${JANUS_ITEM_TYPE:-}\""
json+=",\"item_id\":\"${JANUS_ITEM_ID:-}\""

# Add optional fields if present
if [[ -n "${JANUS_ITEM_PATH:-}" ]]; then
    json+=",\"item_path\":\"$(json_escape "${JANUS_ITEM_PATH}")\""
fi

if [[ -n "${JANUS_FIELD:-}" ]]; then
    json+=",\"field\":\"$(json_escape "${JANUS_FIELD}")\""
fi

if [[ -n "${JANUS_OLD_VALUE:-}" ]]; then
    json+=",\"old_value\":\"$(json_escape "${JANUS_OLD_VALUE}")\""
fi

if [[ -n "${JANUS_NEW_VALUE:-}" ]]; then
    json+=",\"new_value\":\"$(json_escape "${JANUS_NEW_VALUE}")\""
fi

json+="}"

# Append to log file (create if doesn't exist)
echo "$json" >> "$LOG_FILE"
