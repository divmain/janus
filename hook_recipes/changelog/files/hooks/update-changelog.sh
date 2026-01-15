#!/bin/bash
# update-changelog.sh
# Automatically appends completed tickets to CHANGELOG.md

set -e

# Only run when status is changed to complete
if [[ "$JANUS_FIELD_NAME" != "status" ]] || [[ "$JANUS_NEW_VALUE" != "complete" ]]; then
    exit 0
fi

# Get ticket info from environment
TICKET_ID="$JANUS_TICKET_ID"
TICKET_FILE="$JANUS_TICKET_PATH"

if [[ -z "$TICKET_FILE" ]] || [[ ! -f "$TICKET_FILE" ]]; then
    echo "Error: Ticket file not found: $TICKET_FILE" >&2
    exit 1
fi

# Read ticket title (first H1 line)
TICKET_TITLE=$(grep -m 1 '^# ' "$TICKET_FILE" | sed 's/^# //')

if [[ -z "$TICKET_TITLE" ]]; then
    echo "Error: Could not extract ticket title from $TICKET_FILE" >&2
    exit 1
fi

# Read ticket type from frontmatter
TICKET_TYPE=$(sed -n '/^---$/,/^---$/p' "$TICKET_FILE" | grep '^type:' | sed 's/^type:[[:space:]]*//')

# Default to task if type not found
if [[ -z "$TICKET_TYPE" ]]; then
    TICKET_TYPE="task"
fi

# Determine changelog section based on ticket type
case "$TICKET_TYPE" in
    bug)
        SECTION="### Fixed"
        ;;
    feature)
        SECTION="### Added"
        ;;
    task|chore|*)
        SECTION="### Changed"
        ;;
esac

# Find project root (parent of .janus/)
JANUS_DIR=$(dirname "$TICKET_FILE")
while [[ "$JANUS_DIR" != "/" ]] && [[ ! -d "$JANUS_DIR/../.janus" ]]; do
    JANUS_DIR=$(dirname "$JANUS_DIR")
done

if [[ -d "$JANUS_DIR/../.janus" ]]; then
    PROJECT_ROOT=$(dirname "$JANUS_DIR")
elif [[ -d "$JANUS_DIR/.janus" ]]; then
    PROJECT_ROOT="$JANUS_DIR"
else
    # Fallback: use JANUS_ROOT if available, otherwise two levels up from ticket
    PROJECT_ROOT="${JANUS_ROOT:-$(dirname "$(dirname "$TICKET_FILE")")}"
fi

CHANGELOG="$PROJECT_ROOT/CHANGELOG.md"
TODAY=$(date +%Y-%m-%d)
ENTRY="- [$TICKET_ID] $TICKET_TITLE"

# Create CHANGELOG.md if it doesn't exist
if [[ ! -f "$CHANGELOG" ]]; then
    cat > "$CHANGELOG" << 'EOF'
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

EOF
fi

# Check if Unreleased section exists
if ! grep -q '## \[Unreleased\]' "$CHANGELOG"; then
    # Add Unreleased section after the header
    sed -i.bak '/^# Changelog/a\
\
## [Unreleased]\
' "$CHANGELOG"
    rm -f "$CHANGELOG.bak"
fi

# Function to add entry under the correct section
add_changelog_entry() {
    local changelog="$1"
    local section="$2"
    local entry="$3"
    local today="$4"
    
    # Create a temporary file
    local tmpfile=$(mktemp)
    
    # Track state while processing
    local in_unreleased=0
    local section_found=0
    local entry_added=0
    local found_next_version=0
    
    while IFS= read -r line || [[ -n "$line" ]]; do
        # Detect Unreleased section
        if [[ "$line" =~ ^##[[:space:]]*\[Unreleased\] ]]; then
            in_unreleased=1
            echo "$line" >> "$tmpfile"
            continue
        fi
        
        # Detect next version section (end of Unreleased)
        if [[ $in_unreleased -eq 1 ]] && [[ "$line" =~ ^##[[:space:]]*\[ ]] && [[ ! "$line" =~ \[Unreleased\] ]]; then
            # If we haven't added the entry yet, add it before the next version
            if [[ $entry_added -eq 0 ]]; then
                if [[ $section_found -eq 0 ]]; then
                    echo "" >> "$tmpfile"
                    echo "$section" >> "$tmpfile"
                fi
                echo "$entry" >> "$tmpfile"
                echo "" >> "$tmpfile"
                entry_added=1
            fi
            in_unreleased=0
            found_next_version=1
        fi
        
        # Look for our section within Unreleased
        if [[ $in_unreleased -eq 1 ]] && [[ "$line" == "$section" ]]; then
            section_found=1
            echo "$line" >> "$tmpfile"
            continue
        fi
        
        # If we're in our section and hit another ### or ##, add entry first
        if [[ $in_unreleased -eq 1 ]] && [[ $section_found -eq 1 ]] && [[ $entry_added -eq 0 ]]; then
            if [[ "$line" =~ ^### ]] || [[ "$line" =~ ^## ]]; then
                echo "$entry" >> "$tmpfile"
                entry_added=1
            fi
        fi
        
        # If we found our section and this is an empty line after entries, add our entry
        if [[ $in_unreleased -eq 1 ]] && [[ $section_found -eq 1 ]] && [[ $entry_added -eq 0 ]] && [[ -z "$line" ]]; then
            echo "$entry" >> "$tmpfile"
            entry_added=1
        fi
        
        echo "$line" >> "$tmpfile"
    done < "$changelog"
    
    # If we never added the entry (file ended while in Unreleased)
    if [[ $entry_added -eq 0 ]]; then
        if [[ $section_found -eq 0 ]]; then
            echo "" >> "$tmpfile"
            echo "$section" >> "$tmpfile"
        fi
        echo "$entry" >> "$tmpfile"
    fi
    
    mv "$tmpfile" "$changelog"
}

# Check if this entry already exists to avoid duplicates
if grep -qF "$ENTRY" "$CHANGELOG"; then
    exit 0
fi

add_changelog_entry "$CHANGELOG" "$SECTION" "$ENTRY" "$TODAY"

echo "Added to CHANGELOG.md: $ENTRY"
