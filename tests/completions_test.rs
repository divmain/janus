use std::process::Command;

mod common;

// ============================================================================
// Completions command tests
// ============================================================================

#[test]
fn test_completions_bash() {
    let output = Command::new(common::janus_binary())
        .args(["completions", "bash"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("_janus"));
}

#[test]
fn test_completions_zsh() {
    let output = Command::new(common::janus_binary())
        .args(["completions", "zsh"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("#compdef janus"));
}

#[test]
fn test_completions_fish() {
    let output = Command::new(common::janus_binary())
        .args(["completions", "fish"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("complete -c janus"));
}

#[test]
fn test_completions_invalid_shell() {
    let output = Command::new(common::janus_binary())
        .args(["completions", "invalid"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
}
