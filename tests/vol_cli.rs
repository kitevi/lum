use assert_cmd::Command;
use predicates::str::contains;

fn lum() -> Command {
    Command::cargo_bin("lum").unwrap()
}

#[test]
fn vol_shows_sets_and_resets() {
    lum()
        .args(["vol", "--help"])
        .assert()
        .success()
        .stdout(contains("system volume"))
        .stdout(contains("  reset "))
        .stdout(contains("  set "))
        .stdout(contains("  status "));
}

#[test]
fn vol_takes_no_bare_level() {
    lum()
        .args(["vol", "40"])
        .assert()
        .code(2)
        .stderr(contains("unrecognized subcommand '40'"));
}

#[test]
fn vol_set_rejects_non_numeric_volume() {
    lum()
        .args(["vol", "set", "abc"])
        .assert()
        .failure()
        .stderr(contains("invalid value"));
}

#[test]
fn vol_set_rejects_volume_above_100() {
    lum()
        .args(["vol", "set", "200"])
        .assert()
        .code(2)
        .stderr(contains("expected a whole number from 0 to 100"));
}
