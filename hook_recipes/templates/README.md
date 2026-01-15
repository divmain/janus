# Templates Recipe

Enforces ticket structure based on ticket type. Ensures that tickets contain required sections before they are saved.

## Setup

Install the recipe:
```bash
janus hook install templates
```

## Requirements by Ticket Type

| Type | Required Section |
|------|------------------|
| bug | `## Steps to Reproduce` |
| feature | `## Acceptance Criteria` |
| epic | `## Acceptance Criteria` |
| task | None |
| chore | None |

## When Validation Runs

Validation only runs:
- On `ticket_created` events
- When creating a new ticket file

Existing tickets are not re-validated on updates, allowing you to edit tickets freely after creation.

## Examples

### Valid Bug Ticket

```markdown
---
id: j-a1b2
status: new
type: bug
priority: 1
---
# Login button not responding

The login button on the homepage doesn't work on mobile devices.

## Steps to Reproduce

1. Open the app on a mobile device
2. Navigate to the login page
3. Tap the "Login" button
4. Nothing happens (expected: login form should submit)

## Environment

- iOS 17.2
- Safari browser
```

### Valid Feature Ticket

```markdown
---
id: j-c3d4
status: new
type: feature
priority: 2
---
# Add dark mode support

Users should be able to toggle between light and dark themes.

## Acceptance Criteria

- [ ] Add theme toggle in settings
- [ ] Persist theme preference
- [ ] Support system preference detection
- [ ] All pages render correctly in both modes

## Design Notes

Follow the existing color palette defined in the design system.
```

## Customizing Requirements

To modify the required sections, edit `.janus/hooks/validate-template.sh`:

```bash
# Example: Require "## Technical Notes" for all features
feature)
    if ! echo "$CONTENT" | grep -q '^## Acceptance Criteria'; then
        echo "Error: Feature tickets must have an '## Acceptance Criteria' section." >&2
        exit 1
    fi
    if ! echo "$CONTENT" | grep -q '^## Technical Notes'; then
        echo "Error: Feature tickets must have a '## Technical Notes' section." >&2
        exit 1
    fi
    ;;
```

### Adding New Type Requirements

Add a new case to the validation script:

```bash
# Example: Require "## Security Considerations" for security type
security)
    if ! echo "$CONTENT" | grep -q '^## Security Considerations'; then
        echo "Error: Security tickets must have a '## Security Considerations' section." >&2
        exit 1
    fi
    ;;
```

## Disabling Validation Temporarily

To bypass validation for a single ticket, you can edit the file directly instead of using `janus create`:

```bash
# Create ticket without validation
cat > .janus/items/j-xxxx.md << 'EOF'
---
id: j-xxxx
status: new
type: bug
---
# Quick bug note
EOF
```

Or temporarily rename the hook script:

```bash
mv .janus/hooks/validate-template.sh .janus/hooks/validate-template.sh.disabled
janus create bug "Quick fix"
mv .janus/hooks/validate-template.sh.disabled .janus/hooks/validate-template.sh
```
