use std::io::stdout;
use std::path::Path;

use crossterm::ExecutableCommand;
use crossterm::cursor;
use crossterm::terminal::{
    self, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use crate::error::{JanusError, Result};
use crate::tui::components::toast::Toast;
use crate::utils::open_in_editor;

pub struct ExternalEditor;

impl ExternalEditor {
    /// Open a ticket file in the user's $EDITOR, suspending the TUI.
    ///
    /// This temporarily exits the alternate screen and raw mode so the
    /// editor can take full control of the terminal. After the editor
    /// exits, terminal state is restored for iocraft to resume.
    pub fn open_ticket_file(path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(JanusError::FileNotFound(format!(
                "Ticket file not found: {}",
                path.to_string_lossy()
            )));
        }

        // Suspend TUI terminal state
        Self::suspend_terminal()
            .map_err(|e| JanusError::TuiError(format!("Failed to suspend terminal: {e}")))?;

        // Launch editor (blocks until editor closes)
        let editor_result = open_in_editor(path);

        // Always restore terminal state, even if editor failed.
        // Use best-effort recovery: attempt each restore step independently
        // so a failure in one doesn't prevent the others from running.
        let restore_result = Self::restore_terminal_best_effort();

        // Return the first error encountered (editor error takes priority)
        editor_result?;
        if let Some(err) = restore_result {
            return Err(JanusError::TuiError(format!(
                "Failed to fully restore terminal: {err}"
            )));
        }
        Ok(())
    }

    /// Convert an error from `open_ticket_file` into an appropriate Toast notification.
    ///
    /// Maps error types to user-friendly messages with appropriate severity levels:
    /// - `EditorFailed(127)` (command not found) → error toast suggesting $EDITOR
    /// - `EditorFailed(126)` (permission denied) → error toast suggesting $EDITOR
    /// - `EditorFailed(other)` (non-zero exit) → warning toast with exit code
    /// - `FileNotFound` → error toast with file path
    /// - `Io` (editor not found / spawn failure) → error toast suggesting $EDITOR
    /// - Terminal errors → error toast
    /// - All others → generic error toast
    pub fn error_to_toast(err: &JanusError) -> Toast {
        match err {
            // Shell exit code 127 = command not found, 126 = permission denied
            JanusError::EditorFailed(127 | 126) => Toast::error(
                "Editor failed to launch. Set $EDITOR environment variable.".to_string(),
            ),
            JanusError::EditorFailed(code) => {
                Toast::warning(format!("Editor exited with error (code {code})"))
            }
            JanusError::FileNotFound(msg) => Toast::error(msg.clone()),
            JanusError::Io(io_err)
                if io_err.kind() == std::io::ErrorKind::NotFound
                    || io_err.kind() == std::io::ErrorKind::PermissionDenied =>
            {
                Toast::error(
                    "Editor failed to launch. Set $EDITOR environment variable.".to_string(),
                )
            }
            JanusError::TuiError(msg) => Toast::error(msg.clone()),
            other => Toast::error(format!("{other}")),
        }
    }

    fn suspend_terminal() -> std::io::Result<()> {
        let mut out = stdout();
        out.execute(LeaveAlternateScreen)?;
        disable_raw_mode()?;
        out.execute(cursor::Show)?;
        Ok(())
    }

    /// Attempt to restore terminal state with best-effort recovery.
    ///
    /// Each restore step is attempted independently so that a failure in one
    /// (e.g., hiding cursor) doesn't prevent the others (e.g., re-entering
    /// alternate screen, enabling raw mode) from running. This minimizes the
    /// chance of leaving the terminal in a broken state.
    ///
    /// Returns `None` on full success, or `Some(error_description)` if any
    /// step failed (but all steps were still attempted).
    fn restore_terminal_best_effort() -> Option<String> {
        let mut errors: Vec<String> = Vec::new();
        let mut out = stdout();

        if let Err(e) = out.execute(cursor::Hide) {
            errors.push(format!("hide cursor: {e}"));
        }
        if let Err(e) = out.execute(EnterAlternateScreen) {
            errors.push(format!("enter alternate screen: {e}"));
        }
        if let Err(e) = enable_raw_mode() {
            errors.push(format!("enable raw mode: {e}"));
        }
        if let Err(e) = out.execute(terminal::Clear(terminal::ClearType::All)) {
            errors.push(format!("clear screen: {e}"));
        }

        if errors.is_empty() {
            None
        } else {
            Some(errors.join("; "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::components::toast::ToastLevel;
    use std::path::PathBuf;

    // ========================================================================
    // open_ticket_file: error paths
    // ========================================================================

    #[test]
    fn test_open_ticket_file_nonexistent_path_returns_error() {
        let path = PathBuf::from("/tmp/janus_nonexistent_ticket_file_12345.md");
        let result = ExternalEditor::open_ticket_file(&path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, JanusError::FileNotFound(_)),
            "expected FileNotFound, got: {err:?}"
        );
    }

    #[test]
    fn test_open_ticket_file_nonexistent_path_includes_path_in_message() {
        let path = PathBuf::from("/tmp/janus_nonexistent_ticket_file_12345.md");
        let result = ExternalEditor::open_ticket_file(&path);
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("janus_nonexistent_ticket_file_12345.md"),
            "error message should include the file path, got: {msg}"
        );
    }

    // ========================================================================
    // error_to_toast: mapping tests
    // ========================================================================

    #[test]
    fn test_error_to_toast_editor_not_found_code_127() {
        let err = JanusError::EditorFailed(127);
        let toast = ExternalEditor::error_to_toast(&err);
        assert_eq!(toast.level, ToastLevel::Error);
        assert!(
            toast.message.contains("$EDITOR"),
            "toast should suggest setting $EDITOR, got: {}",
            toast.message
        );
    }

    #[test]
    fn test_error_to_toast_editor_permission_denied_code_126() {
        let err = JanusError::EditorFailed(126);
        let toast = ExternalEditor::error_to_toast(&err);
        assert_eq!(toast.level, ToastLevel::Error);
        assert!(
            toast.message.contains("$EDITOR"),
            "toast should suggest setting $EDITOR, got: {}",
            toast.message
        );
    }

    #[test]
    fn test_error_to_toast_editor_nonzero_exit() {
        let err = JanusError::EditorFailed(1);
        let toast = ExternalEditor::error_to_toast(&err);
        assert_eq!(toast.level, ToastLevel::Warning);
        assert!(
            toast.message.contains("code 1"),
            "toast should include exit code, got: {}",
            toast.message
        );
    }

    #[test]
    fn test_error_to_toast_editor_nonzero_exit_other_code() {
        let err = JanusError::EditorFailed(42);
        let toast = ExternalEditor::error_to_toast(&err);
        assert_eq!(toast.level, ToastLevel::Warning);
        assert!(
            toast.message.contains("code 42"),
            "toast should include exit code, got: {}",
            toast.message
        );
    }

    #[test]
    fn test_error_to_toast_file_not_found() {
        let msg = "Ticket file not found: /path/to/ticket.md".to_string();
        let err = JanusError::FileNotFound(msg.clone());
        let toast = ExternalEditor::error_to_toast(&err);
        assert_eq!(toast.level, ToastLevel::Error);
        assert_eq!(toast.message, msg);
    }

    #[test]
    fn test_error_to_toast_io_not_found() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "editor binary not found");
        let err = JanusError::Io(io_err);
        let toast = ExternalEditor::error_to_toast(&err);
        assert_eq!(toast.level, ToastLevel::Error);
        assert!(
            toast.message.contains("$EDITOR"),
            "toast should suggest setting $EDITOR, got: {}",
            toast.message
        );
    }

    #[test]
    fn test_error_to_toast_io_permission_denied() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err = JanusError::Io(io_err);
        let toast = ExternalEditor::error_to_toast(&err);
        assert_eq!(toast.level, ToastLevel::Error);
        assert!(
            toast.message.contains("$EDITOR"),
            "toast should suggest setting $EDITOR, got: {}",
            toast.message
        );
    }

    #[test]
    fn test_error_to_toast_tui_error() {
        let msg = "Failed to suspend terminal: some error".to_string();
        let err = JanusError::TuiError(msg.clone());
        let toast = ExternalEditor::error_to_toast(&err);
        assert_eq!(toast.level, ToastLevel::Error);
        assert_eq!(toast.message, msg);
    }
}
