use std::io::Write;

use assert_cmd::Command;
use tempfile::TempDir;
use zip::write::SimpleFileOptions;

fn lum_with_env(home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"));
    cmd
}

fn test_font_dir(home: &TempDir) -> std::path::PathBuf {
    if cfg!(target_os = "macos") {
        home.path().join("Library").join("Fonts")
    } else {
        home.path().join("data").join("fonts")
    }
}

/// Create a minimal zip containing fake TTF files for testing.
fn create_test_font_zip(dir: &std::path::Path) -> std::path::PathBuf {
    let zip_path = dir.join("test-font.zip");
    let file = std::fs::File::create(&zip_path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default();

    writer.start_file("FakeFont-Regular.ttf", options).unwrap();
    writer.write_all(b"fake-ttf-content").unwrap();

    writer
        .start_file("subdir/FakeFont-Bold.ttf", options)
        .unwrap();
    writer.write_all(b"fake-ttf-bold").unwrap();

    // DMCA upstream includes compatibility aliases that must not be installed,
    // otherwise they shadow real Windows Arial/Tahoma in fontconfig.
    writer.start_file("Arial.ttf", options).unwrap();
    writer.write_all(b"fake-dmca-Arial-alias").unwrap();

    writer.start_file("Tahoma.ttf", options).unwrap();
    writer.write_all(b"fake-dmca-Tahoma-alias").unwrap();

    // Include a non-TTF file that should be skipped
    writer.start_file("readme.txt", options).unwrap();
    writer.write_all(b"not a font").unwrap();

    writer.finish().unwrap();
    zip_path
}

#[test]
fn font_list_shows_the_managed_catalog() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["fonts", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("dmca-sans-serif"))
        .stdout(predicates::str::contains("not installed"));
}

#[test]
fn font_list_shows_installed_for_present_font_dir() {
    let home = TempDir::new().unwrap();
    let font_dir = test_font_dir(&home).join("dmca-sans-serif");
    std::fs::create_dir_all(&font_dir).unwrap();

    lum_with_env(&home)
        .args(["fonts", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("installed"));
}

#[test]
fn font_status_shows_the_path_and_whether_it_is_installed() {
    let home = TempDir::new().unwrap();
    let font_dir = test_font_dir(&home).join("dmca-sans-serif");

    lum_with_env(&home)
        .args(["fonts", "status", "dmca-sans-serif"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Font:              dmca-sans-serif",
        ))
        .stdout(predicates::str::contains(format!(
            "Path:              {}",
            font_dir.display()
        )))
        .stdout(predicates::str::contains("Exists:            no"));

    std::fs::create_dir_all(&font_dir).unwrap();
    lum_with_env(&home)
        .args(["fonts", "status", "dmca-sans-serif"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Exists:            yes"));
}

#[test]
fn font_status_rejects_unknown_font() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["fonts", "status", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("unknown font \"nonexistent\""));
}

#[test]
fn font_install_rejects_unknown_font() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["fonts", "install", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("unknown font \"nonexistent\""))
        .stderr(predicates::str::contains("dmca-sans-serif"));
}

#[test]
fn font_uninstall_rejects_unknown_font() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["fonts", "uninstall", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("unknown font \"nonexistent\""));
}

#[test]
fn font_uninstall_bails_if_not_installed() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["fonts", "uninstall", "dmca-sans-serif"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not installed"));
}

#[test]
fn font_install_downloads_and_extracts_ttf_files() {
    let home = TempDir::new().unwrap();
    let zip_path = create_test_font_zip(home.path());

    lum_with_env(&home)
        .env("LUM_FONT_TEST_ARTIFACT_DMCA_SANS_SERIF", &zip_path)
        .args(["fonts", "install", "dmca-sans-serif"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Installed dmca-sans-serif"));

    // Verify TTF files were extracted (not the .txt)
    let font_dir = test_font_dir(&home).join("dmca-sans-serif");
    assert!(font_dir.join("FakeFont-Regular.ttf").exists());
    assert!(font_dir.join("FakeFont-Bold.ttf").exists());
    assert!(!font_dir.join("Arial.ttf").exists());
    assert!(!font_dir.join("Tahoma.ttf").exists());
    assert!(!font_dir.join("readme.txt").exists());
    assert!(!font_dir.join("subdir").exists());

    // Verify list now shows "installed"
    lum_with_env(&home)
        .args(["fonts", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("installed"));
}

#[test]
fn font_install_bails_if_already_installed_without_force() {
    let home = TempDir::new().unwrap();
    let zip_path = create_test_font_zip(home.path());

    // Install first time
    lum_with_env(&home)
        .env("LUM_FONT_TEST_ARTIFACT_DMCA_SANS_SERIF", &zip_path)
        .args(["fonts", "install", "dmca-sans-serif"])
        .assert()
        .success();

    // Second install without --force should fail
    lum_with_env(&home)
        .env("LUM_FONT_TEST_ARTIFACT_DMCA_SANS_SERIF", &zip_path)
        .args(["fonts", "install", "dmca-sans-serif"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("already installed"))
        .stderr(predicates::str::contains("--force"));
}

#[test]
fn font_install_force_reinstalls() {
    let home = TempDir::new().unwrap();
    let zip_path = create_test_font_zip(home.path());

    // Install first time
    lum_with_env(&home)
        .env("LUM_FONT_TEST_ARTIFACT_DMCA_SANS_SERIF", &zip_path)
        .args(["fonts", "install", "dmca-sans-serif"])
        .assert()
        .success();

    // Force reinstall should succeed
    lum_with_env(&home)
        .env("LUM_FONT_TEST_ARTIFACT_DMCA_SANS_SERIF", &zip_path)
        .args(["fonts", "install", "dmca-sans-serif", "--force"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Installed dmca-sans-serif"));
}

#[test]
fn font_install_force_preserves_existing_install_when_new_archive_is_bad() {
    let home = TempDir::new().unwrap();
    let zip_path = create_test_font_zip(home.path());
    let bad_zip_path = home.path().join("bad-font.zip");
    std::fs::write(&bad_zip_path, "not a zip").unwrap();

    lum_with_env(&home)
        .env("LUM_FONT_TEST_ARTIFACT_DMCA_SANS_SERIF", &zip_path)
        .args(["fonts", "install", "dmca-sans-serif"])
        .assert()
        .success();

    lum_with_env(&home)
        .env("LUM_FONT_TEST_ARTIFACT_DMCA_SANS_SERIF", &bad_zip_path)
        .args(["fonts", "install", "dmca-sans-serif", "--force"])
        .assert()
        .failure();

    let font_dir = test_font_dir(&home).join("dmca-sans-serif");
    assert!(font_dir.join("FakeFont-Regular.ttf").exists());
}

#[test]
fn font_install_failure_does_not_leave_installed_marker() {
    let home = TempDir::new().unwrap();
    let bad_zip_path = home.path().join("bad-font.zip");
    std::fs::write(&bad_zip_path, "not a zip").unwrap();

    lum_with_env(&home)
        .env("LUM_FONT_TEST_ARTIFACT_DMCA_SANS_SERIF", &bad_zip_path)
        .args(["fonts", "install", "dmca-sans-serif"])
        .assert()
        .failure();

    let font_dir = test_font_dir(&home).join("dmca-sans-serif");
    assert!(!font_dir.exists());
}

#[test]
fn font_uninstall_removes_font_directory() {
    let home = TempDir::new().unwrap();
    let zip_path = create_test_font_zip(home.path());

    // Install first
    lum_with_env(&home)
        .env("LUM_FONT_TEST_ARTIFACT_DMCA_SANS_SERIF", &zip_path)
        .args(["fonts", "install", "dmca-sans-serif"])
        .assert()
        .success();

    // Uninstall
    lum_with_env(&home)
        .args(["fonts", "uninstall", "dmca-sans-serif"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Uninstalled dmca-sans-serif"));

    // Font directory should be gone
    let font_dir = test_font_dir(&home).join("dmca-sans-serif");
    assert!(!font_dir.exists());

    // list should show "available" again
    lum_with_env(&home)
        .args(["fonts", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("not installed"));
}
