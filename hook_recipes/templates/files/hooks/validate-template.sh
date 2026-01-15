#!/usr/bin/env bash
set -e

# Only validate tickets, not plans
if [ "$JANUS_ITEM_TYPE" != "ticket" ]; then
    exit 0
fi

# Only validate on ticket creation or new files
if [ "$JANUS_EVENT" != "ticket_created" ] && [ "$JANUS_IS_NEW" != "true" ]; then
    exit 0
fi

# Read the file content from stdin (pre_write receives content via stdin)
CONTENT=$(cat)

# Extract ticket type from frontmatter
TICKET_TYPE=$(echo "$CONTENT" | sed -n '/^---$/,/^---$/p' | grep '^type:' | sed 's/type:[[:space:]]*//')

# Validate based on ticket type
case "$TICKET_TYPE" in
    bug)
        if ! echo "$CONTENT" | grep -q '^## Steps to Reproduce'; then
            echo "Error: Bug tickets must have a '## Steps to Reproduce' section." >&2
            echo "" >&2
            echo "Add this section to your ticket:" >&2
            echo "  ## Steps to Reproduce" >&2
            echo "  1. First step" >&2
            echo "  2. Second step" >&2
            echo "  3. Expected vs actual behavior" >&2
            exit 1
        fi
        ;;
    feature)
        if ! echo "$CONTENT" | grep -q '^## Acceptance Criteria'; then
            echo "Error: Feature tickets must have an '## Acceptance Criteria' section." >&2
            echo "" >&2
            echo "Add this section to your ticket:" >&2
            echo "  ## Acceptance Criteria" >&2
            echo "  - [ ] First requirement" >&2
            echo "  - [ ] Second requirement" >&2
            exit 1
        fi
        ;;
    epic)
        if ! echo "$CONTENT" | grep -q '^## Acceptance Criteria'; then
            echo "Error: Epic tickets must have an '## Acceptance Criteria' section." >&2
            echo "" >&2
            echo "Add this section to your ticket:" >&2
            echo "  ## Acceptance Criteria" >&2
            echo "  - [ ] First requirement" >&2
            echo "  - [ ] Second requirement" >&2
            exit 1
        fi
        ;;
    task|chore)
        # No special requirements for task/chore types
        ;;
    *)
        # Unknown type - allow it (don't block on edge cases)
        ;;
esac

# Validation passed
exit 0
