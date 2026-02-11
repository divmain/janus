//! Snapshot testing utilities using insta.
//!
//! This module provides macros and helpers for TUI snapshot testing
//! with automatic normalization of dynamic values like timestamps.
//!
//! # Usage
//!
//! Due to how Rust handles macros, the `assert_tui_snapshot!` macro is exported
//! at the test crate level. Use it like:
//!
//! ```ignore
//! // In your test file:
//! mod common;
//!
//! #[test]
//! fn test_tui_output() {
//!     let output = render_component();
//!     crate::assert_tui_snapshot!(output);
//! }
//! ```

#![allow(dead_code)]

/// Returns insta filter settings for normalizing dynamic values in TUI output.
///
/// Use this with `insta::with_settings!` for custom snapshot assertions:
///
/// ```ignore
/// insta::with_settings!({
///     filters => tui_snapshot_filters(),
/// }, {
///     insta::assert_snapshot!(output);
/// });
/// ```
pub fn tui_snapshot_filters() -> Vec<(&'static str, &'static str)> {
    vec![
        // Normalize ISO 8601 timestamps
        (r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z?", "[TIMESTAMP]"),
        // Normalize elapsed time in milliseconds
        (r"\d+ms", "[TIME]"),
        // Normalize elapsed time in seconds with decimals
        (r"\d+\.\d+s", "[TIME]"),
    ]
}

/// Asserts a snapshot with TUI-specific normalizations applied.
///
/// This is a function-based alternative to the macro that works better
/// with Rust's module system.
pub fn assert_tui_snapshot_impl(name: &str, output: &str) {
    let filters = tui_snapshot_filters();
    insta::with_settings!({
        filters => filters,
    }, {
        insta::assert_snapshot!(name, output);
    });
}

// Note: Self-tests for this module have been intentionally removed.
// This module is included via #[path] into every test binary, so any tests
// here would be duplicated 10+ times across all test binaries.
