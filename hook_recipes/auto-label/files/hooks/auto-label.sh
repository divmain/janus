#!/usr/bin/env bash
#
# Auto-Label Hook
# Automatically adds labels to tickets based on content patterns
#
# Environment variables (provided by Janus):
#   JANUS_FILE_PATH - Path to the ticket file
#   JANUS_ROOT      - Path to the .janus directory
#   JANUS_ITEM_TYPE - Type of item (ticket or plan)
#
# Dependencies:
#   - yq (preferred) or falls back to sed/awk
#

set -euo pipefail

# Only process tickets, not plans
if [[ "${JANUS_ITEM_TYPE:-}" != "ticket" ]]; then
    exit 0
fi

# Validate required environment variables
if [[ -z "${JANUS_FILE_PATH:-}" ]]; then
    echo "Error: JANUS_FILE_PATH not set" >&2
    exit 1
fi

if [[ -z "${JANUS_ROOT:-}" ]]; then
    echo "Error: JANUS_ROOT not set" >&2
    exit 1
fi

TICKET_FILE="$JANUS_FILE_PATH"
RULES_FILE="${JANUS_ROOT}/../label-rules.yaml"

# Check if files exist
if [[ ! -f "$TICKET_FILE" ]]; then
    echo "Error: Ticket file not found: $TICKET_FILE" >&2
    exit 1
fi

if [[ ! -f "$RULES_FILE" ]]; then
    # No rules file, nothing to do
    exit 0
fi

# Read ticket content (lowercase for case-insensitive matching)
TICKET_CONTENT=$(cat "$TICKET_FILE" | tr '[:upper:]' '[:lower:]')

# Function to check if a label already exists in the ticket
label_exists() {
    local label="$1"
    local label_lower=$(echo "$label" | tr '[:upper:]' '[:lower:]')
    
    # Extract labels section from frontmatter and check
    if command -v yq &>/dev/null; then
        local existing=$(yq -r '.labels // [] | .[]' "$TICKET_FILE" 2>/dev/null | tr '[:upper:]' '[:lower:]')
        echo "$existing" | grep -qx "$label_lower" 2>/dev/null
    else
        # Fallback: use grep to find label in frontmatter
        sed -n '/^---$/,/^---$/p' "$TICKET_FILE" | grep -qi "^\s*-\s*$label\s*$" 2>/dev/null
    fi
}

# Function to add a label to the ticket
add_label() {
    local label="$1"
    
    if command -v yq &>/dev/null; then
        # Use yq to add label to array
        local temp_file=$(mktemp)
        
        # Check if labels field exists
        local has_labels=$(yq -r 'has("labels")' "$TICKET_FILE" 2>/dev/null)
        
        if [[ "$has_labels" == "true" ]]; then
            # Add to existing labels array
            yq -i ".labels += [\"$label\"]" "$TICKET_FILE"
        else
            # Create labels field with the new label
            yq -i ".labels = [\"$label\"]" "$TICKET_FILE"
        fi
    else
        # Fallback: use sed/awk
        add_label_with_sed "$label"
    fi
}

# Fallback function using sed to add labels
add_label_with_sed() {
    local label="$1"
    local temp_file=$(mktemp)
    
    # Check if labels field exists in frontmatter
    if grep -q "^labels:" "$TICKET_FILE"; then
        # Add to existing labels array
        awk -v label="$label" '
            /^labels:/ {
                print
                # Check if next line is a list item or empty array
                getline next_line
                if (next_line ~ /^\[\]/) {
                    # Empty array, replace with list
                    print "  - " label
                } else if (next_line ~ /^  -/) {
                    # Already has items, print existing and add new
                    print next_line
                    print "  - " label
                } else {
                    # labels: value format, convert to array
                    print "  - " label
                    if (next_line !~ /^$/ && next_line !~ /^[a-z_]+:/) {
                        print next_line
                    }
                }
                next
            }
            { print }
        ' "$TICKET_FILE" > "$temp_file"
        mv "$temp_file" "$TICKET_FILE"
    else
        # Insert labels field before the closing ---
        awk -v label="$label" '
            BEGIN { in_frontmatter = 0; frontmatter_count = 0 }
            /^---$/ {
                frontmatter_count++
                if (frontmatter_count == 2) {
                    print "labels:"
                    print "  - " label
                }
                print
                next
            }
            { print }
        ' "$TICKET_FILE" > "$temp_file"
        mv "$temp_file" "$TICKET_FILE"
    fi
}

# Parse rules and check patterns
# Using a simple approach that works with or without yq

parse_and_apply_rules() {
    local current_label=""
    local in_patterns=0
    
    while IFS= read -r line; do
        # Skip comments and empty lines
        [[ "$line" =~ ^[[:space:]]*# ]] && continue
        [[ -z "${line// /}" ]] && continue
        
        # Check for label definition
        if [[ "$line" =~ ^[[:space:]]*-[[:space:]]*label:[[:space:]]*(.+) ]]; then
            current_label="${BASH_REMATCH[1]}"
            current_label="${current_label//\"/}"  # Remove quotes
            current_label="${current_label// /}"   # Trim spaces
            in_patterns=0
            continue
        fi
        
        # Check for patterns section
        if [[ "$line" =~ ^[[:space:]]*patterns: ]]; then
            in_patterns=1
            continue
        fi
        
        # Check for pattern item (when in patterns section)
        if [[ $in_patterns -eq 1 && "$line" =~ ^[[:space:]]*-[[:space:]]*[\"\']?([^\"\']+)[\"\']?[[:space:]]*$ ]]; then
            local pattern="${BASH_REMATCH[1]}"
            pattern="${pattern// /}"  # Trim spaces
            local pattern_lower=$(echo "$pattern" | tr '[:upper:]' '[:lower:]')
            
            # Check if pattern matches ticket content (case-insensitive)
            if echo "$TICKET_CONTENT" | grep -qi "$pattern_lower" 2>/dev/null; then
                # Check if label already exists
                if ! label_exists "$current_label"; then
                    echo "Auto-labeling: Adding '$current_label' (matched pattern: $pattern)"
                    add_label "$current_label"
                fi
                # Move to next rule (don't check more patterns for this label)
                in_patterns=0
            fi
        fi
        
        # Reset in_patterns if we hit a new rule
        if [[ "$line" =~ ^[[:space:]]*-[[:space:]]*label: ]]; then
            in_patterns=0
        fi
    done < "$RULES_FILE"
}

# Main execution
parse_and_apply_rules

exit 0
