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
