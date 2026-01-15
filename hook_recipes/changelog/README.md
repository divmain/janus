# Changelog Hook Recipe

Automatically updates `CHANGELOG.md` when tickets are marked as complete.

## What It Does

When a ticket's status changes to `complete`, this hook:

1. Extracts the ticket ID and title
2. Determines the changelog category based on ticket type
3. Appends an entry to `CHANGELOG.md` in the project root
4. Groups entries under the appropriate section in `## [Unreleased]`

## Installation

```bash
janus hook install changelog
```

## CHANGELOG.md Format

This recipe follows the [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format:

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- [j-a1b2] New user authentication feature

### Fixed
- [j-c3d4] Login timeout bug

### Changed
- [j-e5f6] Refactored database connection handling

## [1.0.0] - 2024-01-15

### Added
- Initial release
```

## Entry Categorization

Entries are automatically categorized based on ticket type:

| Ticket Type | Changelog Section |
|-------------|-------------------|
| `bug`       | `### Fixed`       |
| `feature`   | `### Added`       |
| `task`      | `### Changed`     |
| `chore`     | `### Changed`     |

## Entry Format

Each entry follows the format:

```
- [TICKET-ID] Ticket title
```

The ticket title is extracted from the first `# Heading` in the ticket file.

## Releasing a Version

When you're ready to release, manually edit `CHANGELOG.md`:

1. Change `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD`
2. Add a new `## [Unreleased]` section above it
3. Optionally add a comparison link at the bottom

Example:

```markdown
## [Unreleased]

## [1.1.0] - 2024-02-01

### Added
- [j-a1b2] New feature that was completed

### Fixed
- [j-c3d4] Bug that was fixed
```

## Duplicate Prevention

The hook checks if an entry already exists before adding it, preventing duplicates if a ticket is completed multiple times (e.g., reopened and completed again).

## Environment Variables Used

- `JANUS_FIELD_NAME` - The field that was updated
- `JANUS_NEW_VALUE` - The new value of the field
- `JANUS_TICKET_ID` - The ticket ID
- `JANUS_TICKET_PATH` - Path to the ticket file

## Customization

To customize the changelog format or categorization, edit `files/hooks/update-changelog.sh` after installation:

```bash
# Edit the installed hook
$EDITOR .janus/hooks/update-changelog.sh
```
