//! Test guard utilities.
//!
//! For overriding the Janus root directory in tests, use
//! [`crate::paths::JanusRootGuard`] which provides a thread-local override
//! safe for parallel test execution.
//!
//! The previous `CwdGuard` and `EnvGuard` types were removed because they
//! mutated process-global state (`set_current_dir`, `set_var`) which caused
//! flaky test failures when tests ran in parallel.
