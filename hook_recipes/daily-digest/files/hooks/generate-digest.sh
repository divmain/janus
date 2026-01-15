#!/usr/bin/env bash
# Generate a daily activity digest from activity-log.jsonl
#
# Usage: ./generate-digest.sh [days]
#   days - Number of days to include (default: 1)
#
# Output: Markdown formatted digest suitable for standups

set -euo pipefail

DAYS="${1:-1}"

# Find JANUS_ROOT by looking for .janus directory
find_janus_root() {
    local dir="$PWD"
    while [[ "$dir" != "/" ]]; do
        if [[ -d "$dir/.janus" ]]; then
            echo "$dir/.janus"
            return 0
        fi
        dir="$(dirname "$dir")"
    done
    echo "Error: Not in a janus repository (no .janus directory found)" >&2
    return 1
}

JANUS_ROOT="${JANUS_ROOT:-$(find_janus_root)}"
LOG_FILE="${JANUS_ROOT}/activity-log.jsonl"

if [[ ! -f "$LOG_FILE" ]]; then
    echo "No activity log found at $LOG_FILE"
    echo "Activity will be logged once hooks are triggered."
    exit 0
fi

# Calculate cutoff date (beginning of day N days ago)
if [[ "$(uname)" == "Darwin" ]]; then
    CUTOFF=$(date -v-"${DAYS}"d +"%Y-%m-%d")
else
    CUTOFF=$(date -d "${DAYS} days ago" +"%Y-%m-%d")
fi

# Get ticket title from file if possible
get_ticket_title() {
    local item_id="$1"
    local item_type="$2"
    
    if [[ "$item_type" == "ticket" ]]; then
        local ticket_file="${JANUS_ROOT}/items/${item_id}.md"
        if [[ -f "$ticket_file" ]]; then
            # Extract title from first H1 after frontmatter
            sed -n '/^---$/,/^---$/d; /^# /p' "$ticket_file" | head -1 | sed 's/^# //'
        fi
    elif [[ "$item_type" == "plan" ]]; then
        local plan_file="${JANUS_ROOT}/plans/${item_id}.md"
        if [[ -f "$plan_file" ]]; then
            sed -n '/^---$/,/^---$/d; /^# /p' "$plan_file" | head -1 | sed 's/^# //'
        fi
    fi
}

# Arrays to collect activities by category
declare -A created_items
declare -A completed_items
declare -A in_progress_items
declare -A other_updates
declare -A dates_seen

# Process log file
while IFS= read -r line; do
    # Extract timestamp and check if within range
    timestamp=$(echo "$line" | grep -o '"timestamp":"[^"]*"' | cut -d'"' -f4)
    event_date="${timestamp:0:10}"
    
    # Skip if before cutoff
    if [[ "$event_date" < "$CUTOFF" ]]; then
        continue
    fi
    
    dates_seen["$event_date"]=1
    
    # Extract fields
    event=$(echo "$line" | grep -o '"event":"[^"]*"' | cut -d'"' -f4)
    item_type=$(echo "$line" | grep -o '"item_type":"[^"]*"' | cut -d'"' -f4)
    item_id=$(echo "$line" | grep -o '"item_id":"[^"]*"' | cut -d'"' -f4)
    field=$(echo "$line" | grep -o '"field":"[^"]*"' | cut -d'"' -f4 2>/dev/null || echo "")
    new_value=$(echo "$line" | grep -o '"new_value":"[^"]*"' | cut -d'"' -f4 2>/dev/null || echo "")
    
    # Get title
    title=$(get_ticket_title "$item_id" "$item_type")
    title="${title:-"(no title)"}"
    
    # Build entry key for deduplication
    entry_key="${event_date}|${item_id}"
    entry="- **${item_id}**: ${title}"
    
    # Categorize
    case "$event" in
        ticket_created|plan_created)
            created_items["$entry_key"]="$entry"
            ;;
        ticket_updated)
            if [[ "$field" == "status" ]]; then
                case "$new_value" in
                    complete|cancelled)
                        completed_items["$entry_key"]="$entry (${new_value})"
                        ;;
                    in_progress)
                        in_progress_items["$entry_key"]="$entry"
                        ;;
                    *)
                        other_updates["$entry_key"]="$entry (status: ${new_value})"
                        ;;
                esac
            else
                other_updates["$entry_key"]="$entry (${field} updated)"
            fi
            ;;
    esac
done < "$LOG_FILE"

# Sort dates in reverse order
sorted_dates=($(printf '%s\n' "${!dates_seen[@]}" | sort -r))

if [[ ${#sorted_dates[@]} -eq 0 ]]; then
    echo "# Activity Digest"
    echo ""
    echo "No activity found in the last ${DAYS} day(s)."
    exit 0
fi

echo "# Activity Digest"
echo ""
echo "_Generated: $(date '+%Y-%m-%d %H:%M')_"
echo "_Period: Last ${DAYS} day(s) (since ${CUTOFF})_"
echo ""

for date in "${sorted_dates[@]}"; do
    echo "## Activity for ${date}"
    echo ""
    
    has_content=false
    
    # Created items
    created_for_date=()
    for key in "${!created_items[@]}"; do
        if [[ "$key" == "${date}|"* ]]; then
            created_for_date+=("${created_items[$key]}")
        fi
    done
    if [[ ${#created_for_date[@]} -gt 0 ]]; then
        echo "### Created"
        printf '%s\n' "${created_for_date[@]}" | sort -u
        echo ""
        has_content=true
    fi
    
    # Completed items
    completed_for_date=()
    for key in "${!completed_items[@]}"; do
        if [[ "$key" == "${date}|"* ]]; then
            completed_for_date+=("${completed_items[$key]}")
        fi
    done
    if [[ ${#completed_for_date[@]} -gt 0 ]]; then
        echo "### Completed"
        printf '%s\n' "${completed_for_date[@]}" | sort -u
        echo ""
        has_content=true
    fi
    
    # In Progress items
    in_progress_for_date=()
    for key in "${!in_progress_items[@]}"; do
        if [[ "$key" == "${date}|"* ]]; then
            in_progress_for_date+=("${in_progress_items[$key]}")
        fi
    done
    if [[ ${#in_progress_for_date[@]} -gt 0 ]]; then
        echo "### In Progress"
        printf '%s\n' "${in_progress_for_date[@]}" | sort -u
        echo ""
        has_content=true
    fi
    
    # Other updates
    other_for_date=()
    for key in "${!other_updates[@]}"; do
        if [[ "$key" == "${date}|"* ]]; then
            other_for_date+=("${other_updates[$key]}")
        fi
    done
    if [[ ${#other_for_date[@]} -gt 0 ]]; then
        echo "### Other Updates"
        printf '%s\n' "${other_for_date[@]}" | sort -u
        echo ""
        has_content=true
    fi
    
    if [[ "$has_content" == false ]]; then
        echo "_No activity_"
        echo ""
    fi
done
