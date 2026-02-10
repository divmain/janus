//! Simple file I/O utilities with hook support

use crate::error::{JanusError, Result};
use crate::hooks::{
    HookContext, HookEvent, run_post_hooks, run_post_hooks_async, run_pre_hooks,
    run_pre_hooks_async,
};
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;
use tokio::fs as tokio_fs;

/// RAII guard that holds an advisory file lock.
///
/// The lock is automatically released when the guard is dropped (the underlying
/// file handle is closed). On non-Unix platforms, or if locking fails, this is
/// a no-op — the guard still exists but holds no lock. This provides graceful
/// degradation: concurrent safety on Unix, and no change in behavior elsewhere.
#[allow(dead_code)]
pub struct FileLockGuard {
    // Holding the File keeps the flock active until drop.
    _file: Option<std::fs::File>,
}

/// Acquire an advisory exclusive file lock on the given path.
///
/// Uses `flock(2)` with `LOCK_EX` on Unix to serialize concurrent
/// read-modify-write operations on the same file. The lock is best-effort:
/// if the file cannot be opened or locking fails, a no-op guard is returned
/// and the caller proceeds without a lock (graceful degradation).
///
/// The returned [`FileLockGuard`] holds the lock until it is dropped.
pub fn lock_file_exclusive(path: &Path) -> FileLockGuard {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;

        // Open or create a lock file alongside the target.
        // We use the target file itself so the lock is per-ticket-file.
        let file = match std::fs::OpenOptions::new().read(true).open(path) {
            Ok(f) => f,
            Err(_) => return FileLockGuard { _file: None },
        };

        let fd = file.as_raw_fd();
        // LOCK_EX = exclusive lock, blocks until acquired.
        // Safety: fd is a valid file descriptor owned by `file`.
        let ret = unsafe { libc::flock(fd, libc::LOCK_EX) };
        if ret != 0 {
            // Locking failed — proceed without lock (graceful degradation).
            return FileLockGuard { _file: None };
        }

        FileLockGuard { _file: Some(file) }
    }

    #[cfg(not(unix))]
    {
        let _ = path;
        FileLockGuard { _file: None }
    }
}

/// Read file content with error handling
pub fn read_file(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| JanusError::StorageError {
        operation: "read",
        item_type: "file",
        path: path.to_path_buf(),
        source: e,
    })
}

/// Write file content with error handling
pub fn write_file(path: &Path, content: &str) -> Result<()> {
    write_file_atomic(path, content)
}

/// Write file atomically using temp file and rename.
///
/// This ensures that the original file is never in a partially written state.
/// The write is atomic: either the new content is fully written, or the
/// original file remains unchanged. Uses `tempfile::NamedTempFile` to generate
/// a unique temp filename, avoiding collisions from concurrent writes.
pub fn write_file_atomic(path: &Path, content: &str) -> Result<()> {
    ensure_parent_dir(path)?;

    let parent = path.parent().unwrap_or(Path::new("."));

    // Create a uniquely-named temp file in the same directory as the target
    let mut temp_file = NamedTempFile::new_in(parent).map_err(|e| JanusError::StorageError {
        operation: "create temp file for",
        item_type: "file",
        path: path.to_path_buf(),
        source: e,
    })?;

    // Write content to the temp file
    temp_file
        .write_all(content.as_bytes())
        .map_err(|e| JanusError::StorageError {
            operation: "write",
            item_type: "file",
            path: temp_file.path().to_path_buf(),
            source: e,
        })?;

    // Atomically persist (rename) the temp file to the target path
    temp_file
        .persist(path)
        .map_err(|e| JanusError::StorageError {
            operation: "rename",
            item_type: "file",
            path: path.to_path_buf(),
            source: e.into(),
        })?;

    Ok(())
}

/// Ensure parent directory exists
pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent).map_err(|e| JanusError::StorageError {
            operation: "create",
            item_type: "directory",
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    Ok(())
}

/// Delete a file with error handling
pub fn delete_file(path: &Path) -> Result<()> {
    std::fs::remove_file(path).map_err(|e| JanusError::StorageError {
        operation: "delete",
        item_type: "file",
        path: path.to_path_buf(),
        source: e,
    })
}

/// Execute an operation with standard write hooks.
///
/// Runs PreWrite hook before the operation, then PostWrite hook,
/// and optionally an additional post-hook event after successful completion.
pub fn with_write_hooks<F>(
    context: HookContext,
    operation: F,
    post_hook_event: Option<HookEvent>,
) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    run_pre_hooks(HookEvent::PreWrite, &context)?;
    operation()?;
    run_post_hooks(HookEvent::PostWrite, &context);
    if let Some(event) = post_hook_event {
        run_post_hooks(event, &context);
    }
    Ok(())
}

/// Execute an operation with standard write hooks (async version).
///
/// Runs PreWrite hook before the operation, then PostWrite hook,
/// and optionally an additional post-hook event after successful completion.
pub async fn with_write_hooks_async<F, Fut>(
    context: HookContext,
    operation: F,
    post_hook_event: Option<HookEvent>,
) -> Result<()>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    run_pre_hooks_async(HookEvent::PreWrite, &context).await?;
    operation().await?;
    run_post_hooks_async(HookEvent::PostWrite, &context).await;
    if let Some(event) = post_hook_event {
        run_post_hooks_async(event, &context).await;
    }
    Ok(())
}

/// Read file content with error handling (async version)
pub async fn read_file_async(path: &Path) -> Result<String> {
    tokio_fs::read_to_string(path)
        .await
        .map_err(|e| JanusError::StorageError {
            operation: "read",
            item_type: "file",
            path: path.to_path_buf(),
            source: e,
        })
}

/// Write file content with error handling (async version)
pub async fn write_file_async(path: &Path, content: &str) -> Result<()> {
    write_file_async_atomic(path, content).await
}

/// Write file atomically using temp file and rename (async version).
///
/// This ensures that the original file is never in a partially written state.
/// The write is atomic: either the new content is fully written, or the
/// original file remains unchanged. Uses `tempfile::NamedTempFile` to generate
/// a unique temp filename, avoiding collisions from concurrent writes.
pub async fn write_file_async_atomic(path: &Path, content: &str) -> Result<()> {
    ensure_parent_dir_async(path).await?;

    let parent = path.parent().unwrap_or(Path::new("."));

    // Create a uniquely-named temp file in the same directory as the target
    let mut temp_file = NamedTempFile::new_in(parent).map_err(|e| JanusError::StorageError {
        operation: "create temp file for",
        item_type: "file",
        path: path.to_path_buf(),
        source: e,
    })?;

    // Write content to the temp file
    temp_file
        .write_all(content.as_bytes())
        .map_err(|e| JanusError::StorageError {
            operation: "write",
            item_type: "file",
            path: temp_file.path().to_path_buf(),
            source: e,
        })?;

    // Atomically persist (rename) the temp file to the target path
    temp_file
        .persist(path)
        .map_err(|e| JanusError::StorageError {
            operation: "rename",
            item_type: "file",
            path: path.to_path_buf(),
            source: e.into(),
        })?;

    Ok(())
}

/// Ensure parent directory exists (async version)
pub async fn ensure_parent_dir_async(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        tokio_fs::create_dir_all(parent)
            .await
            .map_err(|e| JanusError::StorageError {
                operation: "create",
                item_type: "directory",
                path: parent.to_path_buf(),
                source: e,
            })?;
    }
    Ok(())
}

/// Delete a file with error handling (async version)
pub async fn delete_file_async(path: &Path) -> Result<()> {
    tokio_fs::remove_file(path)
        .await
        .map_err(|e| JanusError::StorageError {
            operation: "delete",
            item_type: "file",
            path: path.to_path_buf(),
            source: e,
        })
}
