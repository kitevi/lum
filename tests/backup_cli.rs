use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn backup_is_a_subcommand_with_only_a_bio_target() {
    Command::cargo_bin("lum")
        .unwrap()
        .args(["backup", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Backup and restore"))
        .stdout(predicates::str::contains("bio"))
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
