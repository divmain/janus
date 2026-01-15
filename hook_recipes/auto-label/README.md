# Auto-Label Hook Recipe

Automatically adds labels to tickets based on content patterns. When a ticket is saved, this hook scans the ticket content and applies relevant labels based on configurable pattern rules.

## What It Does

1. Triggers after a ticket is written (`post_write` hook)
2. Reads the ticket content (title and body)
3. Matches content against patterns defined in `label-rules.yaml`
4. Adds matching labels to the ticket's `labels` frontmatter field
5. Skips labels that already exist (no duplicates)

## Installation

Copy the recipe files to your Janus repository:

```bash
# Copy the hook script
cp files/hooks/auto-label.sh .janus/hooks/post_write/
chmod +x .janus/hooks/post_write/auto-label.sh

# Copy the rules file to your repo root (alongside .janus/)
cp files/label-rules.yaml ./label-rules.yaml
```

## Configuration

### Label Rules (`label-rules.yaml`)

Define pattern-to-label mappings in `label-rules.yaml`:

```yaml
rules:
  - label: security
    patterns:
      - "auth"
      - "password"
      - "encryption"

  - label: database
    patterns:
      - "SQL"
      - "migration"
      - "schema"
```

Each rule consists of:
- **label**: The label to apply when a pattern matches
- **patterns**: List of substrings to search for

### Adding Custom Rules

Add new rules to `label-rules.yaml`:

```yaml
rules:
  # ... existing rules ...

  - label: testing
    patterns:
      - "test"
      - "spec"
      - "mock"
      - "fixture"

  - label: documentation
    patterns:
      - "README"
      - "docs"
      - "comment"
      - "docstring"
```

## Pattern Matching Behavior

- **Case-insensitive**: "API", "api", and "Api" all match the pattern "api"
- **Substring matching**: Pattern "auth" matches "authentication", "oauth", "unauthorized"
- **First match wins**: Once a pattern matches for a label, that label is added and remaining patterns for that label are skipped
- **No duplicates**: Labels already present on the ticket are not added again

## Dependencies

The hook works best with `yq` installed for reliable YAML manipulation:

```bash
# macOS
brew install yq

# Linux
sudo snap install yq

# Or via pip
pip install yq
```

**Fallback**: If `yq` is not available, the hook falls back to `sed`/`awk` for basic YAML manipulation. The fallback works for most cases but may have edge cases with complex YAML structures.

## Examples

### Example 1: Security Ticket

**Ticket content:**
```markdown
---
id: j-a1b2
status: new
type: bug
---
# Fix authentication bypass vulnerability

Users can bypass password check by...
```

**After auto-label runs:**
```markdown
---
id: j-a1b2
status: new
type: bug
labels:
  - security
---
# Fix authentication bypass vulnerability

Users can bypass password check by...
```

The hook matched patterns "auth", "password", and "vulnerability" and added the `security` label.

### Example 2: Multiple Labels

**Ticket content:**
```markdown
---
id: j-c3d4
status: new
type: feature
---
# Add caching layer for API endpoints

Implement Redis cache to optimize slow database queries...
```

**After auto-label runs:**
```markdown
---
id: j-c3d4
status: new
type: feature
labels:
  - api
  - performance
  - database
---
# Add caching layer for API endpoints

Implement Redis cache to optimize slow database queries...
```

The hook matched:
- "API", "endpoint" → `api` label
- "cache", "optimize", "slow" → `performance` label  
- "database" → `database` label

### Example 3: Existing Labels Preserved

**Ticket content (already has labels):**
```markdown
---
id: j-e5f6
status: in_progress
type: task
labels:
  - urgent
  - security
---
# Update encryption algorithm

Switch from SHA1 to SHA256...
```

**After auto-label runs:**
```markdown
---
id: j-e5f6
status: in_progress
type: task
labels:
  - urgent
  - security
---
# Update encryption algorithm

Switch from SHA1 to SHA256...
```

The `security` label already exists, so it's not duplicated. The `urgent` label (manually added) is preserved.

## Troubleshooting

### Labels not being added

1. Check that the hook is executable: `chmod +x .janus/hooks/post_write/auto-label.sh`
2. Verify `label-rules.yaml` exists in your repository root
3. Run the hook manually to see output:
   ```bash
   JANUS_FILE_PATH=.janus/items/j-xxxx.md \
   JANUS_ROOT=.janus \
   JANUS_ITEM_TYPE=ticket \
   .janus/hooks/post_write/auto-label.sh
   ```

### YAML formatting issues

If you see malformed YAML after the hook runs:
1. Install `yq` for more reliable YAML handling
2. Check for unusual characters in your ticket content

## Limitations

- Only processes tickets (not plans)
- Patterns are simple substring matches (not regex)
- The fallback sed/awk implementation may not handle all YAML edge cases
