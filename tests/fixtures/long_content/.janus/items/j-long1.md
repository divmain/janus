---
id: j-long1
uuid: 00000000-0000-0000-0000-000000000101
status: in_progress
deps: []
links: []
created: 2024-01-01T00:00:00Z
type: feature
priority: 1
---
# Ticket with very long body content

This ticket has extensive body content to test that the TUI layout
properly handles overflow and keeps the footer visible at all times.

## Problem Description

When ticket content is very long and exceeds the available vertical space
in the detail pane, the footer status bar can be pushed off the bottom
of the terminal window. This is problematic because users need to always
see the keyboard shortcuts available to them.

## Technical Details

The TUI uses a flexbox layout with the following structure:
- Header (fixed height)
- Content area (flex-grow: 1.0)
- Footer (fixed height)

The issue occurs when the content area's children produce more content
than fits in the available space. Without proper constraints, this can
cause the overall layout to exceed the terminal height.

## Solution Approach

1. Add `flex_shrink: 0.0` to fixed elements (header, footer)
2. Add `overflow: Overflow::Hidden` to content areas
3. Ensure inner components respect their allocated space

## Implementation Notes

Line 1 of implementation notes
Line 2 of implementation notes
Line 3 of implementation notes
Line 4 of implementation notes
Line 5 of implementation notes
Line 6 of implementation notes
Line 7 of implementation notes
Line 8 of implementation notes
Line 9 of implementation notes
Line 10 of implementation notes
Line 11 of implementation notes
Line 12 of implementation notes
Line 13 of implementation notes
Line 14 of implementation notes
Line 15 of implementation notes
Line 16 of implementation notes
Line 17 of implementation notes
Line 18 of implementation notes
Line 19 of implementation notes
Line 20 of implementation notes
Line 21 of implementation notes
Line 22 of implementation notes
Line 23 of implementation notes
Line 24 of implementation notes
Line 25 of implementation notes
Line 26 of implementation notes
Line 27 of implementation notes
Line 28 of implementation notes
Line 29 of implementation notes
Line 30 of implementation notes
Line 31 of implementation notes
Line 32 of implementation notes
Line 33 of implementation notes
Line 34 of implementation notes
Line 35 of implementation notes
Line 36 of implementation notes
Line 37 of implementation notes
Line 38 of implementation notes
Line 39 of implementation notes
Line 40 of implementation notes
Line 41 of implementation notes
Line 42 of implementation notes
Line 43 of implementation notes
Line 44 of implementation notes
Line 45 of implementation notes
Line 46 of implementation notes
Line 47 of implementation notes
Line 48 of implementation notes
Line 49 of implementation notes
Line 50 of implementation notes

## Acceptance Criteria

- Footer always visible regardless of content length
- Header always visible regardless of content length
- Content area properly clips overflow
- Scrolling within detail pane works correctly
