use serial_test::serial;
use std::process::Command;

mod common;

// ============================================================================
// Completions command tests
// ============================================================================

#[test]
#[serial]
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
#[serial]
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
#[serial]
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
#[serial]
fn test_completions_invalid_shell() {
    let output = Command::new(common::janus_binary())
        .args(["completions", "invalid"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
}
