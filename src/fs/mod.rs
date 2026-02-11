//! Simple file I/O utilities with hook support
//!
//! # Concurrency Model
//!
//! All writes use atomic replace (write to a temp file, then rename onto the
//! target). This guarantees readers never see a partially-written file.
//! However, concurrent writers follow **last-writer-wins** semantics: if two
//! processes perform overlapping read-modify-write cycles on the same file,
//! one update may silently overwrite the other. No advisory locking is
//! performed because atomic-replace swaps the file's inode, which makes
//! `flock(2)`-style locks ineffective.

use crate::error::{JanusError, Result};
use crate::hooks::{
    HookContext, HookEvent, run_post_hooks, run_post_hooks_async, run_pre_hooks,
    run_pre_hooks_async,
};
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;
use tokio::fs as tokio_fs;

/// Read file content with error handling
pub fn read_file(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| JanusError::StorageError {
        operation: "read",
        item_type: "file",
        path: path.to_path_buf(),
        source: e,
    })
}

/// Write file content with error handling.
///
/// Uses atomic replace (temp file + rename) so readers never see partial
/// writes. Concurrent writers use **last-writer-wins** semantics — no
/// advisory locking is performed.
pub fn write_file(path: &Path, content: &str) -> Result<()> {
    write_file_atomic(path, content)
}

/// Write file atomically using temp file and rename.
///
/// This ensures that the original file is never in a partially written state.
/// The write is atomic: either the new content is fully written, or the
/// original file remains unchanged. Uses `tempfile::NamedTempFile` to generate
/// a unique temp filename, avoiding collisions from concurrent writes.
///
/// **Concurrency note**: no advisory locking is performed. Concurrent
/// read-modify-write cycles follow last-writer-wins semantics.
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

/// Write file content with error handling (async version).
///
/// Uses atomic replace (temp file + rename) so readers never see partial
/// writes. Concurrent writers use **last-writer-wins** semantics.
pub async fn write_file_async(path: &Path, content: &str) -> Result<()> {
    write_file_async_atomic(path, content).await
}

/// Write file atomically using temp file and rename (async version).
///
/// This ensures that the original file is never in a partially written state.
/// The write is atomic: either the new content is fully written, or the
/// original file remains unchanged. Uses `tempfile::NamedTempFile` to generate
/// a unique temp filename, avoiding collisions from concurrent writes.
///
/// The synchronous I/O operations are wrapped in `spawn_blocking` to avoid
/// blocking the async runtime.
///
/// **Concurrency note**: no advisory locking is performed. Concurrent
/// read-modify-write cycles follow last-writer-wins semantics.
pub async fn write_file_async_atomic(path: &Path, content: &str) -> Result<()> {
    let path = path.to_path_buf();
    let content = content.to_string();
    let path_for_error = path.clone();

    tokio::task::spawn_blocking(move || write_file_atomic(&path, &content))
        .await
        .map_err(|e| JanusError::StorageError {
            operation: "write",
            item_type: "file",
            path: path_for_error,
            source: std::io::Error::other(e),
        })?
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
