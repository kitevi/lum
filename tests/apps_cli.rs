#![cfg(target_os = "macos")]

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn write_fixture_zip(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("openemu-fixture.zip");
    let file = std::fs::File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("OpenEmu.app/Contents/Info.plist", options)
        .unwrap();
    std::io::Write::write_all(&mut zip, b"test-bundle").unwrap();
    zip.finish().unwrap();
    path
}

fn apps_with_env(home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("LUM_APPS_DIR", home.path().join("apps"))
        .env("LUM_APPS_FOREIGN_SCAN_DIRS", home.path().join("scan"));
    cmd
}

#[test]
fn apps_install_verifies_pinned_checksum_for_same_release() {
    let home = TempDir::new().unwrap();
    let artifact = write_fixture_zip(&home);

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "install", "openemu"])
        .assert()
        .success()
        .stdout(contains("\u{2713} Installed openemu"));

    std::fs::write(&artifact, "tampered zip bytes").unwrap();
    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "install", "openemu", "--force"])
        .assert()
        .failure()
        .stderr(contains("checksum mismatch"));
}

#[test]
fn apps_status_reports_missing_app() {
    let home = TempDir::new().unwrap();

    apps_with_env(&home)
        .args(["apps", "status", "openemu"])
        .assert()
        .success()
        .stdout(contains("Managed:           no"))
        .stdout(contains("Exists:            no"))
        .stdout(contains("OpenEmu.app"));
}

#[test]
fn apps_status_unknown_app_lists_available() {
    let home = TempDir::new().unwrap();

    apps_with_env(&home)
        .args(["apps", "status", "wat"])
        .assert()
        .failure()
        .stderr(contains("unknown managed app \"wat\""))
        .stderr(contains("openemu"));
}

#[test]
fn apps_status_defaults_to_applications_lum_dir() {
    let home = TempDir::new().unwrap();

    Command::cargo_bin("lum")
        .unwrap()
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("HOME", home.path())
        .args(["apps", "status", "openemu"])
        .assert()
        .success()
        .stdout(contains("Applications/lum/OpenEmu.app"));
}

#[test]
fn apps_install_uses_local_artifact_override_and_records_managed_state() {
    let home = TempDir::new().unwrap();
    let apps_dir = home.path().join("apps"); // does not exist yet: install must create it
    let artifact = write_fixture_zip(&home);

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "install", "openemu"])
        .assert()
        .success()
        .stdout(contains("\u{2713} Installed openemu"));

    assert!(apps_dir.join("OpenEmu.app/Contents/Info.plist").exists());

    apps_with_env(&home)
        .args(["apps", "status", "openemu"])
        .assert()
        .success()
        .stdout(contains("Managed:           yes"))
        .stdout(contains("Installed version: test"));
}

#[test]
fn apps_install_refuses_unmanaged_bundle_without_force() {
    let home = TempDir::new().unwrap();
    let bundle = home.path().join("apps").join("OpenEmu.app");
    std::fs::create_dir_all(bundle.join("Contents")).unwrap();
    std::fs::write(bundle.join("Contents").join("marker"), "manual").unwrap();

    apps_with_env(&home)
        .args(["apps", "install", "openemu"])
        .assert()
        .failure()
        .stderr(contains("not managed by lum"));

    assert_eq!(
        std::fs::read_to_string(bundle.join("Contents").join("marker")).unwrap(),
        "manual"
    );
}

#[test]
fn apps_install_force_takes_over_unmanaged_bundle() {
    let home = TempDir::new().unwrap();
    let bundle = home.path().join("apps").join("OpenEmu.app");
    std::fs::create_dir_all(bundle.join("Contents")).unwrap();
    std::fs::write(bundle.join("Contents").join("Info.plist"), "manual").unwrap();
    let artifact = write_fixture_zip(&home);

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "install", "openemu", "--force"])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(bundle.join("Contents").join("Info.plist")).unwrap(),
        "test-bundle"
    );
}

#[test]
fn apps_install_refuses_when_already_managed() {
    let home = TempDir::new().unwrap();
    let artifact = write_fixture_zip(&home);

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "install", "openemu"])
        .assert()
        .success();

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "install", "openemu"])
        .assert()
        .failure()
        .stderr(contains("already installed"));
}

fn seed_managed_state(home: &TempDir) -> std::path::PathBuf {
    let bundle = home.path().join("apps").join("OpenEmu.app");
    std::fs::create_dir_all(bundle.join("Contents")).unwrap();
    std::fs::write(bundle.join("Contents").join("marker"), "old").unwrap();
    let state_dir = home.path().join("config").join("lum");
    std::fs::create_dir_all(&state_dir).unwrap();
    let state = serde_json::json!({
        "version": "1.0",
        "apps": {
            "openemu": {
                "installed": true,
                "path": bundle,
                "installed_version": "0.9",
                "installed_at": { "secs_since_epoch": 1, "nanos_since_epoch": 0 },
                "updated_at": { "secs_since_epoch": 1, "nanos_since_epoch": 0 },
                "artifact": {
                    "release_tag": "v0.9",
                    "asset_name": "OpenEmu_0.9.zip",
                    "download_url": "local"
                }
            }
        }
    });
    std::fs::write(
        state_dir.join("apps-state.json"),
        serde_json::to_string_pretty(&state).unwrap(),
    )
    .unwrap();
    bundle
}

#[test]
fn apps_update_requires_managed_install() {
    let home = TempDir::new().unwrap();

    apps_with_env(&home)
        .args(["apps", "update", "openemu"])
        .assert()
        .failure()
        .stderr(contains("not installed"))
        .stderr(contains("lum apps install openemu"));
}

#[test]
fn apps_update_replaces_older_managed_bundle() {
    let home = TempDir::new().unwrap();
    let bundle = seed_managed_state(&home);
    let artifact = write_fixture_zip(&home);

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "update", "openemu"])
        .assert()
        .success()
        .stdout(contains("\u{2713} Updated openemu 0.9 -> test"));

    assert_eq!(
        std::fs::read_to_string(bundle.join("Contents").join("Info.plist")).unwrap(),
        "test-bundle"
    );

    apps_with_env(&home)
        .args(["apps", "status", "openemu"])
        .assert()
        .success()
        .stdout(contains("Installed version: test"));
}

#[test]
fn apps_update_skips_when_up_to_date_unless_forced() {
    let home = TempDir::new().unwrap();
    let artifact = write_fixture_zip(&home);

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "install", "openemu"])
        .assert()
        .success();

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "update", "openemu"])
        .assert()
        .success()
        .stdout(contains("already up to date"));

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "update", "openemu", "--force"])
        .assert()
        .success();
}

#[test]
fn apps_sync_dry_run_installs_nothing() {
    let home = TempDir::new().unwrap();
    let artifact = write_fixture_zip(&home);

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "sync", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("\u{2022} openemu: would install test"));

    assert!(!home.path().join("apps").join("OpenEmu.app").exists());
    assert!(
        !home
            .path()
            .join("config")
            .join("lum")
            .join("apps-state.json")
            .exists()
    );
}

#[test]
fn apps_sync_installs_missing_and_reports_summary() {
    let home = TempDir::new().unwrap();
    let artifact = write_fixture_zip(&home);

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "sync"])
        .assert()
        .success()
        .stdout(contains("\u{2022} openemu: installing test... done"))
        .stdout(contains("Summary: 1 installed"));

    assert!(
        home.path()
            .join("apps")
            .join("OpenEmu.app/Contents/Info.plist")
            .exists()
    );

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "sync"])
        .assert()
        .success()
        .stdout(contains("\u{2022} openemu: up to date (test)"));
}

#[test]
fn apps_sync_skips_unmanaged_bundle_without_touching_it() {
    let home = TempDir::new().unwrap();
    let bundle = home.path().join("apps").join("OpenEmu.app");
    std::fs::create_dir_all(bundle.join("Contents")).unwrap();
    std::fs::write(bundle.join("Contents").join("marker"), "manual").unwrap();
    let artifact = write_fixture_zip(&home);

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "sync"])
        .assert()
        .success()
        .stdout(contains("--force"));

    assert_eq!(
        std::fs::read_to_string(bundle.join("Contents").join("marker")).unwrap(),
        "manual"
    );
    assert!(
        !home
            .path()
            .join("config")
            .join("lum")
            .join("apps-state.json")
            .exists()
    );
}

#[test]
fn apps_install_fails_fast_on_foreign_copy_even_with_force() {
    let home = TempDir::new().unwrap();
    let scan = home.path().join("scan");
    std::fs::create_dir_all(scan.join("OpenEmu.app")).unwrap();
    let artifact = write_fixture_zip(&home);

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "install", "openemu"])
        .assert()
        .failure()
        .stderr(contains("not managed by lum"))
        .stderr(contains("foreign copy"));

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "install", "openemu", "--force"])
        .assert()
        .failure()
        .stderr(contains("foreign copy"));
}

#[test]
fn apps_sync_fails_fast_on_foreign_copy() {
    let home = TempDir::new().unwrap();
    let scan = home.path().join("scan");
    std::fs::create_dir_all(scan.join("OpenEmu.app")).unwrap();
    let artifact = write_fixture_zip(&home);

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "sync"])
        .assert()
        .failure()
        .stderr(contains("foreign copy"));
}

#[test]
fn apps_status_and_ls_surface_foreign_copies() {
    let home = TempDir::new().unwrap();
    let scan = home.path().join("scan");
    std::fs::create_dir_all(scan.join("OpenEmu.app")).unwrap();
    let foreign = scan.join("OpenEmu.app").display().to_string();

    apps_with_env(&home)
        .args(["apps", "status", "openemu"])
        .assert()
        .success()
        .stdout(contains("Foreign copies:"))
        .stdout(contains(foreign.clone()));

    apps_with_env(&home)
        .args(["apps", "ls"])
        .assert()
        .success()
        .stdout(contains("foreign:"))
        .stdout(contains(foreign));
}

#[test]
fn apps_version_shows_stored_and_latest() {
    let home = TempDir::new().unwrap();
    let artifact = write_fixture_zip(&home);

    apps_with_env(&home)
        .env("LUM_APPS_TEST_ARTIFACT_OPENEMU", &artifact)
        .args(["apps", "version", "openemu"])
        .assert()
        .success()
        .stdout(contains("Installed: unknown"))
        .stdout(contains("Latest:    test"));
}

#[test]
fn apps_ls_lists_the_managed_catalog() {
    let home = TempDir::new().unwrap();

    apps_with_env(&home)
        .args(["apps", "ls"])
        .assert()
        .success()
        .stdout(contains("openemu"))
        .stdout(contains("OpenEmu.app"))
        .stdout(contains("not installed"));
}
