//! Process-level contracts for `lum net` that need neither root nor a network.

#![cfg(any(target_os = "linux", target_os = "macos"))]

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn net(state: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("XDG_STATE_HOME", state.path());
    cmd.arg("net");
    cmd
}

#[test]
fn net_help_lists_the_modes() {
    let state = TempDir::new().unwrap();
    net(&state)
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("game"))
        .stdout(contains("normal"))
        .stdout(contains("status"))
        .stdout(contains("test"));
}

#[test]
fn net_game_help_documents_the_forwarded_command() {
    let state = TempDir::new().unwrap();
    net(&state)
        .args(["game", "--help"])
        .assert()
        .success()
        .stdout(contains("COMMAND"))
        .stdout(contains("--iface"));
}

#[test]
fn net_normal_without_game_mode_changes_nothing() {
    let state = TempDir::new().unwrap();
    net(&state)
        .arg("normal")
        .assert()
        .success()
        .stdout(contains("Game mode is not on."));
}

#[test]
fn net_test_rejects_out_of_range_durations() {
    let state = TempDir::new().unwrap();
    net(&state)
        .args(["test", "--seconds", "500"])
        .assert()
        .failure()
        .stderr(contains("--seconds must be between 5 and 120"));
}
