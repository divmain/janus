# Plan Format Specification

This document describes the format for plan documents that can be imported
into Janus using `janus plan import`.

## Basic Structure

```markdown
# Plan Title (required)

Introductory paragraph(s) providing a description of the overall plan.

## Design

Comprehensive description of the desired end-state when the multi-phase plan
is complete. This section should contain multiple sections breaking down the
design, key technical decisions, architecture, reasoning behind the design,
and the final acceptance criteria for the entire plan.

## Acceptance Criteria (optional)

- First criterion
- Second criterion

## Implementation

### Phase 1: Phase Name

Multi-paragraph description of what should be accomplished in Phase 1.

#### The Title of the First Task in Phase One

The first task's description, implementation notes, or code examples. Required.
Must be comprehensive -- bullet points are acceptable, as are multiple paragraphs.
Must include code samples if required for clarity. Must include acceptance
criteria for the task.

#### The Title of the Second Task in Phase One

The second task's description. All task descriptions must be comprehensive.

### Phase 2: Another Phase Name

#### The Title of the First Task in Phase Two

Task description.
```

## Required Sections

The following sections are **required**:

1. **`# Plan Title`** (H1) - The plan title, must be first heading
2. **`## Design`** (H2) - Design details, architecture, and reasoning
3. **`## Implementation`** (H2) - Contains all phase definitions

## Optional Sections

- **`## Acceptance Criteria`** (H2) - If present, creates a verification ticket

## Element Reference

| Element             | Format                      | Notes                                       |
|---------------------|-----------------------------|---------------------------------------------|
| Plan title          | `# Title` (H1)              | Required, must be first heading             |
| Description         | Paragraphs after H1         | Optional, before first H2                   |
| Design              | `## Design`                 | Required, contains design details           |
| Acceptance criteria | `## Acceptance Criteria`    | Optional, creates verification ticket       |
| Implementation      | `## Implementation`         | Required, contains all phases               |
| Phase               | `### Phase N: Name`         | Under Implementation; also: Stage N, etc.   |
| Task                | `#### Task Title`           | Under a phase, becomes ticket title         |
| Completed task      | `#### Title [x]`            | Created with status: complete               |
| Task body           | Content after H4            | Becomes ticket description                  |

## Phase Numbering

Phase numbers can be:
- Numeric: `### Phase 1:`, `### Phase 2:`
- Alphanumeric: `### Phase 1a:`, `### Phase 2b:`
- Keywords: Phase, Stage, Part, Step (followed by number and optional name)

## Task Content

Content between an H4 task header and the next H4/H3 becomes the ticket body:

```markdown
#### Add Caching Support

Implement caching in the TTS service to avoid redundant synthesis.

Key changes:
- Add cache data structure
- Modify speak() method

**Acceptance Criteria:**
- Cache hits return in <5ms
- Cache invalidation works correctly

#### Next Task
```

The above creates a ticket titled "Add Caching Support" with the description
containing all the prose, bullet points, and acceptance criteria.

## Examples

See `janus plan import --dry-run <file>` to preview what would be created.
