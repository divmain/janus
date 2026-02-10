use std::io::{self, BufRead};
use std::path::Path;
use std::process::Command;

use crate::error::{JanusError, Result};

/// Open a file in the user's preferred editor ($EDITOR, defaulting to vi)
///
/// Executes the editor through a shell to support EDITOR values with arguments
/// (e.g., "subl -w", "code --wait").
///
/// # Security Note
///
/// This function intentionally uses shell execution (`sh -c`) to interpret the
/// `$EDITOR` environment variable, which allows arbitrary command execution.
/// This is by design and follows Unix conventions used by git, mercurial, and
/// other CLI tools.
///
/// The `$EDITOR` variable is user-controlled configuration, not untrusted input.
/// If an attacker can modify a user's environment variables (e.g., via a
/// compromised `.bashrc`), they already have code execution in every shell
/// session—the editor invocation adds no additional attack surface.
///
/// The file path argument is safely passed using shell positional parameters
/// (`$1`) to prevent path-based injection.
pub fn open_in_editor(path: &Path) -> Result<()> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("--")
        .arg(path)
        .status()?;

    if !status.success() {
        return Err(JanusError::EditorFailed(status.code().unwrap_or(-1)));
    }

    Ok(())
}

/// Read all input from stdin (for piped input)
pub fn read_stdin() -> io::Result<String> {
    let stdin = io::stdin();
    let mut lines = Vec::new();
    for line in stdin.lock().lines() {
        lines.push(line?);
    }
    Ok(lines.join("\n").trim().to_string())
}

/// Check if stdin is a TTY (interactive)
pub fn is_stdin_tty() -> bool {
    atty_check()
}

#[cfg(unix)]
fn atty_check() -> bool {
    use std::os::unix::io::AsRawFd;
    unsafe { libc::isatty(std::io::stdin().as_raw_fd()) != 0 }
}

#[cfg(not(unix))]
fn atty_check() -> bool {
    // On non-Unix, assume it's a TTY
    true
}
