//! Test fixture utilities for pointing tests at pre-created fixture directories.
//!
//! This module provides helpers for working with the `tests/fixtures/` directory,
//! which contains pre-created Janus repositories for testing.
//!
//! **Important**: These helpers return paths and values that should be passed to
//! subprocess commands via `Command::env("JANUS_ROOT", ...)`. Never use
//! `std::env::set_var` in integration tests — it mutates process-global state
//! and races with parallel tests.

use std::path::{Path, PathBuf};

/// Get the path to a test fixture directory.
pub fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Get the JANUS_ROOT value for a fixture (its `.janus` subdirectory).
///
/// Use this with `Command::env("JANUS_ROOT", fixture_janus_root("name"))` to
/// point a subprocess at a fixture without mutating the test process environment.
pub fn fixture_janus_root(name: &str) -> PathBuf {
    fixture_path(name).join(".janus")
}
