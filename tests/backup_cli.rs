use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn backup_is_a_subcommand_with_bio_and_roms_targets() {
    Command::cargo_bin("lum")
        .unwrap()
        .args(["backup", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Backup and restore"))
        .stdout(predicates::str::contains("bio"))
        .stdout(predicates::str::contains("roms"))
        .stdout(predicates::str::contains("openemu").not());
}

#[test]
fn backup_bio_reports_missing_source_directory() {
    let home = tempfile::TempDir::new().unwrap();

    Command::cargo_bin("lum")
        .unwrap()
        .env("HOME", home.path())
        .args(["backup", "bio"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("bio directory not found"));
}

#[test]
fn backup_roms_reports_missing_source_directory() {
    let home = tempfile::TempDir::new().unwrap();

    Command::cargo_bin("lum")
        .unwrap()
        .env("HOME", home.path())
        .args(["backup", "roms"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("roms directory not found"));
}
