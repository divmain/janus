//! Shared navigation utilities for list scrolling
//!
//! This module provides common scrolling logic used across TUI components
//! like the issue browser (view) and remote TUI.

use iocraft::prelude::State;

/// Scroll down one item in a list, adjusting scroll offset if needed.
///
/// Returns the new selected index and scroll offset as a tuple.
pub fn scroll_down(
    selected_index: &mut usize,
    scroll_offset: &mut usize,
    list_count: usize,
    list_height: usize,
) {
    if list_count == 0 {
        return;
    }

    let new_idx = (*selected_index + 1).min(list_count - 1);
    *selected_index = new_idx;

    // Adjust scroll if selection moves past visible area
    if new_idx >= *scroll_offset + list_height {
        *scroll_offset = new_idx.saturating_sub(list_height - 1);
    }
}

/// Scroll up one item in a list, adjusting scroll offset if needed.
pub fn scroll_up(selected_index: &mut usize, scroll_offset: &mut usize) {
    let new_idx = selected_index.saturating_sub(1);
    *selected_index = new_idx;

    // Adjust scroll if selection moves above visible area
    if new_idx < *scroll_offset {
        *scroll_offset = new_idx;
    }
}

/// Jump to the top of a list.
pub fn scroll_to_top(selected_index: &mut usize, scroll_offset: &mut usize) {
    *selected_index = 0;
    *scroll_offset = 0;
}

/// Jump to the bottom of a list.
pub fn scroll_to_bottom(
    selected_index: &mut usize,
    scroll_offset: &mut usize,
    list_count: usize,
    list_height: usize,
) {
    if list_count > 0 {
        let new_idx = list_count - 1;
        *selected_index = new_idx;
        if new_idx >= list_height {
            *scroll_offset = new_idx.saturating_sub(list_height - 1);
        }
    }
}

/// Page down (half page).
pub fn page_down(
    selected_index: &mut usize,
    scroll_offset: &mut usize,
    list_count: usize,
    list_height: usize,
) {
    if list_count == 0 {
        return;
    }

    let jump = list_height / 2;
    let new_idx = (*selected_index + jump).min(list_count.saturating_sub(1));
    *selected_index = new_idx;

    if new_idx >= *scroll_offset + list_height {
        *scroll_offset = new_idx.saturating_sub(list_height - 1);
    }
}

/// Page up (half page).
pub fn page_up(selected_index: &mut usize, scroll_offset: &mut usize, list_height: usize) {
    let jump = list_height / 2;
    let new_idx = selected_index.saturating_sub(jump);
    *selected_index = new_idx;

    if new_idx < *scroll_offset {
        *scroll_offset = new_idx;
    }
}

// ============================================================================
// Higher-level state management wrappers
// ============================================================================

/// Apply scroll down to a State-based navigation pair.
///
/// This wrapper handles the common pattern of:
/// 1. Getting current values from State
/// 2. Applying the scroll operation
/// 3. Setting the new values back to State
pub fn apply_scroll_down(
    selected_index: &mut State<usize>,
    scroll_offset: &mut State<usize>,
    list_count: usize,
    list_height: usize,
) {
    let mut selected = selected_index.get();
    let mut scroll = scroll_offset.get();
    scroll_down(&mut selected, &mut scroll, list_count, list_height);
    selected_index.set(selected);
    scroll_offset.set(scroll);
}

/// Apply scroll up to a State-based navigation pair.
pub fn apply_scroll_up(selected_index: &mut State<usize>, scroll_offset: &mut State<usize>) {
    let mut selected = selected_index.get();
    let mut scroll = scroll_offset.get();
    scroll_up(&mut selected, &mut scroll);
    selected_index.set(selected);
    scroll_offset.set(scroll);
}

/// Apply scroll to top to a State-based navigation pair.
pub fn apply_scroll_to_top(selected_index: &mut State<usize>, scroll_offset: &mut State<usize>) {
    let mut selected = selected_index.get();
    let mut scroll = scroll_offset.get();
    scroll_to_top(&mut selected, &mut scroll);
    selected_index.set(selected);
    scroll_offset.set(scroll);
}

/// Apply scroll to bottom to a State-based navigation pair.
pub fn apply_scroll_to_bottom(
    selected_index: &mut State<usize>,
    scroll_offset: &mut State<usize>,
    list_count: usize,
    list_height: usize,
) {
    let mut selected = selected_index.get();
    let mut scroll = scroll_offset.get();
    scroll_to_bottom(&mut selected, &mut scroll, list_count, list_height);
    selected_index.set(selected);
    scroll_offset.set(scroll);
}

/// Apply page down to a State-based navigation pair.
pub fn apply_page_down(
    selected_index: &mut State<usize>,
    scroll_offset: &mut State<usize>,
    list_count: usize,
    list_height: usize,
) {
    let mut selected = selected_index.get();
    let mut scroll = scroll_offset.get();
    page_down(&mut selected, &mut scroll, list_count, list_height);
    selected_index.set(selected);
    scroll_offset.set(scroll);
}

/// Apply page up to a State-based navigation pair.
pub fn apply_page_up(
    selected_index: &mut State<usize>,
    scroll_offset: &mut State<usize>,
    list_height: usize,
) {
    let mut selected = selected_index.get();
    let mut scroll = scroll_offset.get();
    page_up(&mut selected, &mut scroll, list_height);
    selected_index.set(selected);
    scroll_offset.set(scroll);
}

/// Simple scroll down for detail content (no selection).
pub fn detail_scroll_down(scroll_offset: &mut usize, max_lines: usize, visible_lines: usize) {
    let max_scroll = max_lines.saturating_sub(visible_lines);
    *scroll_offset = (*scroll_offset + 1).min(max_scroll);
}

/// Simple scroll up for detail content (no selection).
pub fn detail_scroll_up(scroll_offset: &mut usize) {
    *scroll_offset = scroll_offset.saturating_sub(1);
}

/// Apply detail scroll down to a State-based scroll offset.
pub fn apply_detail_scroll_down(
    scroll_offset: &mut State<usize>,
    max_lines: usize,
    visible_lines: usize,
) {
    let mut scroll = scroll_offset.get();
    detail_scroll_down(&mut scroll, max_lines, visible_lines);
    scroll_offset.set(scroll);
}

/// Apply detail scroll up to a State-based scroll offset.
pub fn apply_detail_scroll_up(scroll_offset: &mut State<usize>) {
    let mut scroll = scroll_offset.get();
    detail_scroll_up(&mut scroll);
    scroll_offset.set(scroll);
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_down_at_bottom() {
        let mut selected = 9;
        let mut scroll = 0;
        scroll_down(&mut selected, &mut scroll, 10, 10); // Already at end
        assert_eq!(selected, 9, "Should stay at bottom");
    }

    #[test]
    fn test_scroll_up_at_top() {
        let mut selected = 0;
        let mut scroll = 0;
        scroll_up(&mut selected, &mut scroll);
        assert_eq!(selected, 0, "Should stay at top");
        assert_eq!(scroll, 0, "Scroll should stay at 0");
    }

    #[test]
    fn test_scroll_down_triggers_scroll() {
        let mut selected = 4;
        let mut scroll = 0;
        scroll_down(&mut selected, &mut scroll, 10, 5);
        assert_eq!(selected, 5);
        assert_eq!(scroll, 1, "Should scroll to keep visible");
    }

    #[test]
    fn test_scroll_up_triggers_scroll() {
        let mut selected = 5;
        let mut scroll = 5;
        scroll_up(&mut selected, &mut scroll);
        assert_eq!(selected, 4);
        assert_eq!(scroll, 4, "Should scroll up to keep visible");
    }

    #[test]
    fn test_page_down_empty_list() {
        let mut selected = 0;
        let mut scroll = 0;
        page_down(&mut selected, &mut scroll, 0, 10);
        assert_eq!(selected, 0, "Should stay at 0 for empty list");
    }

    #[test]
    fn test_page_up_at_top() {
        let mut selected = 2;
        let mut scroll = 0;
        page_up(&mut selected, &mut scroll, 10);
        assert_eq!(selected, 0, "Should go to top");
    }

    #[test]
    fn test_scroll_to_bottom_small_list() {
        let mut selected = 0;
        let mut scroll = 0;
        scroll_to_bottom(&mut selected, &mut scroll, 3, 10);
        assert_eq!(selected, 2);
        assert_eq!(scroll, 0, "No scroll needed for small list");
    }

    #[test]
    fn test_scroll_to_bottom_large_list() {
        let mut selected = 0;
        let mut scroll = 0;
        scroll_to_bottom(&mut selected, &mut scroll, 50, 10);
        assert_eq!(selected, 49);
        assert_eq!(scroll, 40, "Should scroll to show bottom");
    }

    #[test]
    fn test_scroll_down_increments_correctly() {
        let mut selected = 0;
        let mut scroll = 0;
        // Scroll through first few items (all visible)
        for i in 0..4 {
            scroll_down(&mut selected, &mut scroll, 10, 5);
            assert_eq!(selected, i + 1);
            assert_eq!(scroll, 0, "Should not scroll yet, still in visible area");
        }
        // Now scroll past visible area
        scroll_down(&mut selected, &mut scroll, 10, 5);
        assert_eq!(selected, 5);
        assert_eq!(scroll, 1, "Should start scrolling");
    }

    #[test]
    fn test_page_down_clamps_to_max() {
        let mut selected = 95;
        let mut scroll = 90;
        page_down(&mut selected, &mut scroll, 100, 10);
        assert_eq!(selected, 99, "Should clamp to last item");
    }

    #[test]
    fn test_scroll_to_top() {
        let mut selected = 50;
        let mut scroll = 45;
        scroll_to_top(&mut selected, &mut scroll);
        assert_eq!(selected, 0);
        assert_eq!(scroll, 0);
    }
}
