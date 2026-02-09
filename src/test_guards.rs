//! RAII guards for process-global state in tests.
//!
//! Tests that mutate process-global state (current working directory,
//! environment variables) must guarantee restoration even if the test panics.
//! These guards snapshot the state in `new()` and restore it in `Drop`.
//!
//! All tests using these guards should still be marked `#[serial]` since the
//! underlying state is truly process-global and cannot be isolated between
//! concurrent tests.

use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

/// RAII guard that restores the current working directory on drop.
///
/// Snapshots `std::env::current_dir()` on construction and restores it
/// when the guard is dropped, even if the test panics.
///
/// # Example
///
/// ```ignore
/// #[test]
/// #[serial]
/// fn test_something() {
///     let _guard = CwdGuard::new().unwrap();
///     std::env::set_current_dir("/tmp").unwrap();
///     // CWD is automatically restored when _guard goes out of scope
/// }
/// ```
pub struct CwdGuard {
    original: PathBuf,
}

impl CwdGuard {
    /// Create a new guard that snapshots the current working directory.
    pub fn new() -> std::io::Result<Self> {
        let original = env::current_dir()?;
        Ok(Self { original })
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = env::set_current_dir(&self.original);
    }
}

/// RAII guard that restores an environment variable on drop.
///
/// Snapshots the environment variable's value (or absence) on construction
/// and restores it when the guard is dropped, even if the test panics.
///
/// # Example
///
/// ```ignore
/// #[test]
/// #[serial]
/// fn test_something() {
///     let _guard = EnvGuard::new("JANUS_ROOT");
///     unsafe { std::env::set_var("JANUS_ROOT", "/custom/path") };
///     // JANUS_ROOT is automatically restored when _guard goes out of scope
/// }
/// ```
pub struct EnvGuard {
    key: String,
    original: Option<OsString>,
}

impl EnvGuard {
    /// Create a new guard that snapshots the current value of `key`.
    pub fn new(key: &str) -> Self {
        let original = env::var_os(key);
        Self {
            key: key.to_string(),
            original,
        }
    }

    /// Create a new guard and immediately set the variable to `value`.
    ///
    /// This is a convenience for the common pattern of snapshotting and
    /// then setting the variable in a single call.
    ///
    /// # Safety
    /// This function calls `std::env::set_var` which is unsafe in Rust 2024
    /// edition due to potential data races in multi-threaded programs.
    /// Tests using this should be marked `#[serial]`.
    pub unsafe fn set(key: &str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let guard = Self::new(key);
        unsafe { env::set_var(key, value) };
        guard
    }

    /// Create a new guard and immediately remove the variable.
    ///
    /// # Safety
    /// This function calls `std::env::remove_var` which is unsafe in Rust 2024
    /// edition due to potential data races in multi-threaded programs.
    /// Tests using this should be marked `#[serial]`.
    pub unsafe fn remove(key: &str) -> Self {
        let guard = Self::new(key);
        unsafe { env::remove_var(key) };
        guard
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: Drop runs during test teardown. Tests using EnvGuard should be
        // marked #[serial] to ensure single-threaded access to environment variables.
        match &self.original {
            Some(val) => unsafe { env::set_var(&self.key, val) },
            None => unsafe { env::remove_var(&self.key) },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_cwd_guard_restores_on_drop() {
        let original = env::current_dir().unwrap();
        {
            let _guard = CwdGuard::new().unwrap();
            let tmp = tempfile::TempDir::new().unwrap();
            env::set_current_dir(tmp.path()).unwrap();
            assert_ne!(env::current_dir().unwrap(), original);
        }
        assert_eq!(env::current_dir().unwrap(), original);
    }

    #[test]
    #[serial]
    fn test_env_guard_restores_existing_var() {
        let key = "JANUS_TEST_GUARD_EXISTING";
        unsafe { env::set_var(key, "original_value") };
        {
            let _guard = unsafe { EnvGuard::set(key, "modified_value") };
            assert_eq!(env::var(key).unwrap(), "modified_value");
        }
        assert_eq!(env::var(key).unwrap(), "original_value");
        unsafe { env::remove_var(key) };
    }

    #[test]
    #[serial]
    fn test_env_guard_restores_absent_var() {
        let key = "JANUS_TEST_GUARD_ABSENT";
        unsafe { env::remove_var(key) };
        {
            let _guard = unsafe { EnvGuard::set(key, "temporary") };
            assert_eq!(env::var(key).unwrap(), "temporary");
        }
        assert!(env::var(key).is_err());
    }

    #[test]
    #[serial]
    fn test_env_guard_remove() {
        let key = "JANUS_TEST_GUARD_REMOVE";
        unsafe { env::set_var(key, "should_be_restored") };
        {
            let _guard = unsafe { EnvGuard::remove(key) };
            assert!(env::var(key).is_err());
        }
        assert_eq!(env::var(key).unwrap(), "should_be_restored");
        unsafe { env::remove_var(key) };
    }
}
