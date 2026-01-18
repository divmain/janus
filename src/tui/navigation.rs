//! Shared navigation utilities for list scrolling
//!
//! This module provides common scrolling logic used across TUI components
//! like the issue browser (view) and remote TUI.

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
