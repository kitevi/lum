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
fn net_help_lists_test() {
    let state = TempDir::new().unwrap();
    net(&state)
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("test"));
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
