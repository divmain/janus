//! Simple file I/O utilities with hook support

use crate::error::{JanusError, Result};
use crate::hooks::{
    HookContext, HookEvent, run_post_hooks, run_post_hooks_async, run_pre_hooks,
    run_pre_hooks_async,
};
use std::path::Path;
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

/// Write file content with error handling
pub fn write_file(path: &Path, content: &str) -> Result<()> {
    ensure_parent_dir(path)?;
    std::fs::write(path, content).map_err(|e| JanusError::StorageError {
        operation: "write",
        item_type: "file",
        path: path.to_path_buf(),
        source: e,
    })
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
    ensure_parent_dir_async(path).await?;
    tokio_fs::write(path, content)
        .await
        .map_err(|e| JanusError::StorageError {
            operation: "write",
            item_type: "file",
            path: path.to_path_buf(),
            source: e,
        })
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
