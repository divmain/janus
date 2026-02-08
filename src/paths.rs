use std::path::PathBuf;

/// Returns the root Janus directory path.
///
/// Resolution order:
/// 1. `JANUS_ROOT` environment variable (if set)
/// 2. Current working directory + `.janus`
pub fn janus_root() -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_janus_root_default() {
        // Clear JANUS_ROOT to test default behavior
        // SAFETY: We use #[serial] to ensure single-threaded access
        unsafe { std::env::remove_var("JANUS_ROOT") };
        let root = janus_root();
        assert_eq!(root, PathBuf::from(".janus"));
    }

    #[test]
    #[serial]
    fn test_janus_root_with_env_var() {
        // SAFETY: We use #[serial] to ensure single-threaded access
        unsafe { std::env::set_var("JANUS_ROOT", "/custom/path/.janus") };
        let root = janus_root();
        assert_eq!(root, PathBuf::from("/custom/path/.janus"));
        unsafe { std::env::remove_var("JANUS_ROOT") };
    }

    #[test]
    #[serial]
    fn test_tickets_items_dir_default() {
        // SAFETY: We use #[serial] to ensure single-threaded access
        unsafe { std::env::remove_var("JANUS_ROOT") };
        let dir = tickets_items_dir();
        assert_eq!(dir, PathBuf::from(".janus/items"));
    }

    #[test]
    #[serial]
    fn test_tickets_items_dir_with_env_var() {
        // SAFETY: We use #[serial] to ensure single-threaded access
        unsafe { std::env::set_var("JANUS_ROOT", "/custom/path/.janus") };
        let dir = tickets_items_dir();
        assert_eq!(dir, PathBuf::from("/custom/path/.janus/items"));
        unsafe { std::env::remove_var("JANUS_ROOT") };
    }

    #[test]
    #[serial]
    fn test_plans_dir_default() {
        // SAFETY: We use #[serial] to ensure single-threaded access
        unsafe { std::env::remove_var("JANUS_ROOT") };
        let dir = plans_dir();
        assert_eq!(dir, PathBuf::from(".janus/plans"));
    }

    #[test]
    #[serial]
    fn test_plans_dir_with_env_var() {
        // SAFETY: We use #[serial] to ensure single-threaded access
        unsafe { std::env::set_var("JANUS_ROOT", "/custom/path/.janus") };
        let dir = plans_dir();
        assert_eq!(dir, PathBuf::from("/custom/path/.janus/plans"));
        unsafe { std::env::remove_var("JANUS_ROOT") };
    }
}
