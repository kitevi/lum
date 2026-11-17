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
fn display_help_offers_on_and_off() {
    let state = TempDir::new().unwrap();
    display(&state)
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Usage: lum display [STATE]"))
        .stdout(contains("[possible values: on, off]"));
}

#[test]
fn display_rejects_a_state_other_than_on_or_off() {
    let state = TempDir::new().unwrap();
    display(&state)
        .arg("maybe")
        .assert()
        .failure()
        .stderr(contains("invalid value 'maybe'"));
}

#[cfg(not(target_os = "macos"))]
#[test]
fn display_refuses_to_run_off_macos() {
    let state = TempDir::new().unwrap();
    display(&state)
        .assert()
        .failure()
        .stderr(contains("lum display is only supported on macOS"));
}
