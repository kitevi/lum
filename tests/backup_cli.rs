use assert_cmd::Command;
use predicates::prelude::*;

fn lum() -> Command {
    Command::cargo_bin("lum").unwrap()
}

#[test]
fn backup_creates_and_restores() {
    lum()
        .args(["backup", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Back up and restore"))
        .stdout(predicates::str::contains("  create "))
        .stdout(predicates::str::contains("  restore "));
}

#[test]
fn backup_targets_are_bio_and_roms() {
    lum()
        .args(["backup", "create", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("bio"))
        .stdout(predicates::str::contains("roms"))
        .stdout(predicates::str::contains("openemu").not());
}

#[test]
fn backup_rejects_an_unknown_target() {
    lum()
        .args(["backup", "create", "openemu"])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("invalid value"));
}

#[test]
fn backup_restore_needs_a_code() {
    lum().args(["backup", "restore", "roms"]).assert().code(2);
}

#[test]
fn backup_create_bio_reports_missing_source_directory() {
    let home = tempfile::TempDir::new().unwrap();

    lum()
        .env("HOME", home.path())
        .args(["backup", "create", "bio"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("bio directory not found"));
}

#[test]
fn backup_create_roms_reports_missing_source_directory() {
    let home = tempfile::TempDir::new().unwrap();

    lum()
        .env("HOME", home.path())
        .args(["backup", "create", "roms"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("roms directory not found"));
}
