//! Process-level contracts for `lum display` that change no display.

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn display(state: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("XDG_STATE_HOME", state.path());
    cmd.arg("display");
    cmd
}

#[test]
fn display_help_offers_on_off_srgb_and_status() {
    let state = TempDir::new().unwrap();
    display(&state)
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Usage: lum display <SUBCOMMAND>"))
        .stdout(contains("  on "))
        .stdout(contains("  off "))
        .stdout(contains("  srgb "))
        .stdout(contains("  status "));
}

#[test]
fn display_needs_a_command() {
    let state = TempDir::new().unwrap();
    display(&state).assert().code(2);
}

#[test]
fn display_rejects_an_unknown_command() {
    let state = TempDir::new().unwrap();
    display(&state)
        .arg("maybe")
        .assert()
        .code(2)
        .stderr(contains("unrecognized subcommand 'maybe'"));
}

#[cfg(not(target_os = "macos"))]
#[test]
fn display_refuses_to_run_off_macos() {
    let state = TempDir::new().unwrap();
    display(&state)
        .arg("status")
        .assert()
        .failure()
        .stderr(contains("lum display is only supported on macOS"));
}

#[cfg(not(target_os = "macos"))]
#[test]
fn display_srgb_refuses_to_run_off_macos() {
    let state = TempDir::new().unwrap();
    display(&state)
        .arg("srgb")
        .assert()
        .failure()
        .stderr(contains("lum display is only supported on macOS"));
}
