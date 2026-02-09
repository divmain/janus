//! Test fixture utilities for pointing tests at pre-created fixture directories.
//!
//! This module provides helpers for working with the `tests/fixtures/` directory,
//! which contains pre-created Janus repositories for testing.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Get the path to a test fixture directory
pub fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Set JANUS_ROOT to point to a fixture's .janus directory
///
/// # Safety
/// This function modifies the process environment, which is inherently unsafe
/// in multithreaded contexts. Tests using this should be marked with `#[serial]`.
pub fn use_fixture(name: &str) {
    let path = fixture_path(name);
    // SAFETY: Tests using this should be marked #[serial] to ensure single-threaded access
    unsafe { std::env::set_var("JANUS_ROOT", path.join(".janus")) };
}

/// Clear JANUS_ROOT (return to default behavior)
///
/// # Safety
/// This function modifies the process environment, which is inherently unsafe
/// in multithreaded contexts. Tests using this should be marked with `#[serial]`.
pub fn clear_janus_root() {
    // SAFETY: Tests using this should be marked #[serial] to ensure single-threaded access
    unsafe { std::env::remove_var("JANUS_ROOT") };
}

/// RAII guard that sets JANUS_ROOT and restores it on drop.
///
/// Snapshots the current value of JANUS_ROOT before setting it to the
/// fixture path, and restores the original value (or removes it) on drop.
/// This guarantees cleanup even if a test panics.
///
/// # Example
///
/// ```ignore
/// #[test]
/// #[serial]
/// fn test_with_fixture() {
///     let _guard = FixtureGuard::new("basic_board");
///     // Test code here - JANUS_ROOT points to basic_board fixture
/// }
/// // JANUS_ROOT is automatically restored when _guard goes out of scope
/// ```
pub struct FixtureGuard {
    _name: String,
    original: Option<OsString>,
}

impl FixtureGuard {
    /// Create a new fixture guard that sets JANUS_ROOT to the fixture's .janus directory
    ///
    /// # Safety
    /// This modifies the process environment. Tests using this should be marked with `#[serial]`.
    pub fn new(name: &str) -> Self {
        let original = std::env::var_os("JANUS_ROOT");
        use_fixture(name);
        Self {
            _name: name.to_string(),
            original,
        }
    }
}

impl Drop for FixtureGuard {
    fn drop(&mut self) {
        // SAFETY: Drop runs during test teardown. Tests using FixtureGuard should be
        // marked #[serial] to ensure single-threaded access to environment variables.
        match &self.original {
            Some(val) => unsafe { std::env::set_var("JANUS_ROOT", val) },
            None => clear_janus_root(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_fixture_path() {
        let path = fixture_path("empty_repo");
        assert!(path.to_string_lossy().contains("tests"));
        assert!(path.to_string_lossy().contains("fixtures"));
        assert!(path.to_string_lossy().contains("empty_repo"));
    }

    #[test]
    #[serial]
    fn test_use_fixture_sets_janus_root() {
        clear_janus_root();

        use_fixture("basic_board");
        let root = std::env::var("JANUS_ROOT");
        assert!(root.is_ok());
        assert!(root.unwrap().contains("basic_board"));

        clear_janus_root();
    }

    #[test]
    #[serial]
    fn test_fixture_guard_clears_on_drop() {
        clear_janus_root();

        {
            let _guard = FixtureGuard::new("basic_board");
            assert!(std::env::var("JANUS_ROOT").is_ok());
        }

        assert!(std::env::var("JANUS_ROOT").is_err());
    }
}
