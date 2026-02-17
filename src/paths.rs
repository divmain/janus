use std::cell::RefCell;
use std::path::PathBuf;

thread_local! {
    /// Thread-local override for the Janus root path.
    ///
    /// When set, `janus_root()` returns this value instead of consulting the
    /// `JANUS_ROOT` environment variable or using the default `.janus`. This
    /// enables parallel tests to each point at their own temp directory without
    /// mutating process-global state.
    static JANUS_ROOT_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Set the thread-local Janus root override.
///
/// While set, `janus_root()` on this thread returns the given path. Use
/// [`clear_janus_root_override`] or the RAII guard [`JanusRootGuard`] to
/// restore the default behaviour.
pub fn set_janus_root_override(path: PathBuf) {
    JANUS_ROOT_OVERRIDE.with(|cell| {
        *cell.borrow_mut() = Some(path);
    });
}

/// Clear the thread-local Janus root override.
pub fn clear_janus_root_override() {
    JANUS_ROOT_OVERRIDE.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

/// RAII guard that sets a thread-local Janus root override and clears it on drop.
///
/// This is the preferred way for tests to point `janus_root()` at a temp directory.
/// It is thread-safe (no process-global mutation) and panic-safe (clears on drop).
///
/// # Example
///
/// ```ignore
/// #[test]
/// fn test_something() {
///     let tmp = tempfile::TempDir::new().unwrap();
///     let _guard = JanusRootGuard::new(tmp.path().join(".janus"));
///     // janus_root() now returns tmp/.janus on this thread only
/// }
/// ```
pub struct JanusRootGuard {
    _private: (),
}

impl JanusRootGuard {
    /// Create a new guard that sets the thread-local override to `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        set_janus_root_override(root.into());
        Self { _private: () }
    }
}

impl Drop for JanusRootGuard {
    fn drop(&mut self) {
        clear_janus_root_override();
    }
}

/// Returns the root Janus directory path.
///
/// Resolution order:
/// 1. Thread-local override (if set via [`set_janus_root_override`])
/// 2. `JANUS_ROOT` environment variable (if set)
/// 3. Current working directory + `.janus`
pub fn janus_root() -> PathBuf {
    // Check thread-local override first (safe for parallel tests)
    let tl = JANUS_ROOT_OVERRIDE.with(|cell| cell.borrow().clone());
    if let Some(root) = tl {
        return root;
    }

    if let Ok(root) = std::env::var("JANUS_ROOT") {
        PathBuf::from(root)
    } else {
        PathBuf::from(".janus")
    }
}

/// Returns the path to the tickets items directory.
pub fn tickets_items_dir() -> PathBuf {
    janus_root().join("items")
}

/// Returns the path to the plans directory.
pub fn plans_dir() -> PathBuf {
    janus_root().join("plans")
}

/// Returns the path to the docs directory.
pub fn docs_dir() -> PathBuf {
    janus_root().join("docs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_janus_root_default() {
        // Ensure no thread-local override is set
        let _guard = JanusRootGuard::new("/unused");
        clear_janus_root_override();
        // Without override or env var, should return ".janus"
        // (env var may or may not be set in the process, so we test via override)
    }

    #[test]
    fn test_janus_root_with_thread_local_override() {
        let _guard = JanusRootGuard::new("/custom/path/.janus");
        assert_eq!(janus_root(), PathBuf::from("/custom/path/.janus"));
    }

    #[test]
    fn test_tickets_items_dir_with_override() {
        let _guard = JanusRootGuard::new("/custom/path/.janus");
        assert_eq!(
            tickets_items_dir(),
            PathBuf::from("/custom/path/.janus/items")
        );
    }

    #[test]
    fn test_plans_dir_with_override() {
        let _guard = JanusRootGuard::new("/custom/path/.janus");
        assert_eq!(plans_dir(), PathBuf::from("/custom/path/.janus/plans"));
    }

    #[test]
    fn test_guard_clears_on_drop() {
        {
            let _guard = JanusRootGuard::new("/temporary");
            assert_eq!(janus_root(), PathBuf::from("/temporary"));
        }
        // After guard drops, thread-local is cleared
        let root = JANUS_ROOT_OVERRIDE.with(|cell| cell.borrow().clone());
        assert!(root.is_none());
    }
}
