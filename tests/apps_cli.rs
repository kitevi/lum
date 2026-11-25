#![cfg(target_os = "linux")]

use std::{
    fs,
    io::Cursor,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use assert_cmd::Command;
use backhand::{FilesystemCompressor, FilesystemWriter, NodeHeader, compression::Compressor};
use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

/// What an upstream AppImage holds, as far as lum cares.
struct Fixture {
    file: &'static str,
    program: &'static str,
    desktop: &'static str,
    entry: &'static str,
    icon: &'static str,
}

const MGBA: Fixture = Fixture {
    file: "mGBA-test-appimage-x64.appimage",
    program: "mgba-qt",
    desktop: "io.mgba.mGBA.desktop",
    entry: "[Desktop Entry]\nType=Application\nName=mGBA\nIcon=io.mgba.mGBA\nTryExec=mgba-qt\nExec=mgba-qt %f\nMimeType=application/x-gba-rom;\n",
    icon: "io.mgba.mGBA.png",
};

const SNES9X: Fixture = Fixture {
    file: "Snes9x-test-x86_64.AppImage",
    program: "snes9x-gtk",
    desktop: "snes9x-gtk.desktop",
    entry: "[Desktop Entry]\nName=Snes9x\nType=Application\nMimeType=application/vnd.nintendo.snes.rom;application/x-snes-rom;\nExec=snes9x-gtk %F\nIcon=snes9x\n",
    icon: "snes9x.png",
};

/// A stand-in for an upstream AppImage: an ELF runtime followed by a gzip
/// SquashFS image laid out like the real ones, with AppRun and the desktop
/// entry as symlinks.
fn write_appimage(home: &TempDir, fixture: &Fixture) -> PathBuf {
    let dir = NodeHeader::new(0o755, 0, 0, 0);
    let file = NodeHeader::new(0o644, 0, 0, 0);
    let program = format!("usr/bin/{}", fixture.program);
    let desktop = format!("usr/share/applications/{}", fixture.desktop);
    let mut image = FilesystemWriter::default();
    image.set_compressor(FilesystemCompressor::new(Compressor::Gzip, None).unwrap());
    image.push_dir_all("usr/bin", dir).unwrap();
    image.push_dir_all("usr/share/applications", dir).unwrap();
    image
        .push_file(Cursor::new(b"#!/bin/sh\n".to_vec()), &program, dir)
        .unwrap();
    image
        .push_file(
            Cursor::new(fixture.entry.as_bytes().to_vec()),
            &desktop,
            file,
        )
        .unwrap();
    image
        .push_file(Cursor::new(b"png".to_vec()), fixture.icon, file)
        .unwrap();
    image.push_symlink(&program, "AppRun", dir).unwrap();
    image.push_symlink(&desktop, fixture.desktop, dir).unwrap();
    let mut squashfs = Cursor::new(Vec::new());
    image.write(&mut squashfs).unwrap();

    // A 64-bit little-endian ELF header with no sections ends at byte 64, where
    // the image starts.
    let mut appimage = vec![0u8; 64];
    appimage[..6].copy_from_slice(b"\x7fELF\x02\x01");
    appimage[0x28..0x30].copy_from_slice(&64u64.to_le_bytes());
    appimage.extend(squashfs.into_inner());
    let path = home.path().join(fixture.file);
    fs::write(&path, appimage).unwrap();
    path
}

fn write_fixture_appimage(home: &TempDir) -> PathBuf {
    write_appimage(home, &MGBA)
}

fn lum(home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("XDG_STATE_HOME", home.path().join("state"))
        .env("LUM_APPS_DIR", home.path().join("apps"))
        // Apps running on the machine that runs the tests must not count.
        .env("LUM_APPS_TEST_PROCESS_MGBA", "lum-test-absent")
        .env("LUM_APPS_TEST_PROCESS_SNES9X", "lum-test-absent")
        .env_remove("LUM_APPS_TEST_ARTIFACT_MGBA")
        .env_remove("LUM_APPS_TEST_ARTIFACT_SNES9X");
    cmd
}

/// lum whose `status` takes the latest version from a fixture name instead of GitHub.
fn lum_offline(home: &TempDir) -> Command {
    let mut cmd = lum(home);
    cmd.env("LUM_APPS_TEST_ARTIFACT_MGBA", "mgba.AppImage")
        .env("LUM_APPS_TEST_ARTIFACT_SNES9X", "snes9x.AppImage");
    cmd
}

fn lum_with_fixture(home: &TempDir, appimage: &Path) -> Command {
    let mut cmd = lum(home);
    cmd.env("LUM_APPS_TEST_ARTIFACT_MGBA", appimage);
    cmd
}

/// lum with an AppImage for every app, for commands that walk the catalog.
/// Each fixture is written once per test, so its checksum stays put.
fn lum_with_catalog(home: &TempDir) -> Command {
    let mut cmd = lum(home);
    for (variable, fixture) in [
        ("LUM_APPS_TEST_ARTIFACT_MGBA", &MGBA),
        ("LUM_APPS_TEST_ARTIFACT_SNES9X", &SNES9X),
    ] {
        let path = home.path().join(fixture.file);
        if !path.exists() {
            write_appimage(home, fixture);
        }
        cmd.env(variable, path);
    }
    cmd
}

fn app_dir(home: &TempDir) -> PathBuf {
    home.path().join("apps").join("mgba")
}

fn config_file(home: &TempDir) -> PathBuf {
    home.path().join("config").join("mgba").join("config.ini")
}

fn desktop_entry(home: &TempDir) -> PathBuf {
    home.path()
        .join("data")
        .join("applications")
        .join("lum-mgba.desktop")
}

fn state_file(home: &TempDir) -> PathBuf {
    home.path()
        .join("config")
        .join("lum")
        .join("apps-state.json")
}

fn write_config(home: &TempDir, contents: &str) {
    let config = config_file(home);
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(config, contents).unwrap();
}

fn seed_managed_install(home: &TempDir, version: &str) -> PathBuf {
    let app = app_dir(home);
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("marker"), "old").unwrap();
    let state = serde_json::json!({
        "version": "1.0",
        "apps": {
            "mgba": {
                "installed": true,
                "path": app,
                "installed_version": version,
                "installed_at": { "secs_since_epoch": 1, "nanos_since_epoch": 0 },
                "updated_at": { "secs_since_epoch": 1, "nanos_since_epoch": 0 },
                "artifact": {
                    "release_tag": version,
                    "asset_name": format!("mGBA-{version}-appimage-x64.appimage"),
                    "download_url": "local"
                }
            }
        }
    });
    let state_file = state_file(home);
    fs::create_dir_all(state_file.parent().unwrap()).unwrap();
    fs::write(state_file, serde_json::to_string_pretty(&state).unwrap()).unwrap();
    app
}

#[test]
fn list_shows_the_catalog_with_settings_state() {
    let home = TempDir::new().unwrap();

    lum(&home)
        .args(["apps", "list"])
        .assert()
        .success()
        .stdout(contains("mgba"))
        .stdout(contains("not installed"))
        .stdout(contains("1 to apply"))
        .stdout(contains("Game Boy Advance emulator"))
        .stdout(contains("snes9x"))
        .stdout(contains("Super Nintendo emulator"));
}

#[test]
fn macos_only_apps_do_not_show_up() {
    let home = TempDir::new().unwrap();

    lum(&home)
        .args(["apps", "list"])
        .assert()
        .success()
        .stdout(contains("octomac").not());
    lum(&home)
        .args(["apps", "install", "octomac"])
        .assert()
        .failure()
        .stderr(contains("unknown app \"octomac\""));
}

#[test]
fn status_reports_a_missing_app_and_its_settings() {
    let home = TempDir::new().unwrap();

    lum_offline(&home)
        .args(["apps", "status", "mgba"])
        .assert()
        .success()
        .stdout(contains("Managed:           no"))
        .stdout(contains("Exists:            no"))
        .stdout(contains(format!(
            "Launcher:          {}",
            desktop_entry(&home).display()
        )))
        .stdout(contains(format!(
            "Settings file:     {}",
            config_file(&home).display()
        )))
        .stdout(contains("Settings:          1 to apply"))
        .stdout(contains("[ports.qt] updateAutoCheck: (unset) -> 0"));
}

#[test]
fn status_rejects_unknown_apps_and_lists_available() {
    let home = TempDir::new().unwrap();

    lum_offline(&home)
        .args(["apps", "status", "wat"])
        .assert()
        .failure()
        .stderr(contains("unknown app \"wat\""))
        .stderr(contains("mgba"));
}

#[test]
fn apps_install_into_the_xdg_data_folder_by_default() {
    let home = TempDir::new().unwrap();

    lum_offline(&home)
        .env_remove("LUM_APPS_DIR")
        .args(["apps", "status", "mgba"])
        .assert()
        .success()
        .stdout(contains(format!(
            "Path:              {}",
            home.path().join("data/lum/apps/mgba").display()
        )));
}

#[test]
fn install_unpacks_the_appimage_and_adds_a_desktop_entry() {
    let home = TempDir::new().unwrap();
    let appimage = write_fixture_appimage(&home);

    lum_with_fixture(&home, &appimage)
        .args(["apps", "install", "mgba"])
        .assert()
        .success()
        .stdout(contains("\u{2713} Installed mgba test"))
        .stdout(contains("\u{2713} Set 1 setting in"));

    let app = app_dir(&home);
    let program = app.join("usr/bin/mgba-qt");
    assert_eq!(fs::read_to_string(&program).unwrap(), "#!/bin/sh\n");
    assert_eq!(
        fs::metadata(&program).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert_eq!(
        fs::read_link(app.join("AppRun")).unwrap(),
        Path::new("usr/bin/mgba-qt")
    );

    let entry = fs::read_to_string(desktop_entry(&home)).unwrap();
    assert!(
        entry.contains(&format!("\nExec={} %f\n", app.join("AppRun").display())),
        "{entry}"
    );
    assert!(
        entry.contains(&format!(
            "\nIcon={}\n",
            app.join("io.mgba.mGBA.png").display()
        )),
        "{entry}"
    );
    assert!(
        entry.contains("\nMimeType=application/x-gba-rom;\n"),
        "{entry}"
    );
    assert!(!entry.contains("TryExec"), "{entry}");

    // Nothing but the app is left in the install root.
    let names: Vec<_> = fs::read_dir(home.path().join("apps"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["mgba"]);

    lum_offline(&home)
        .args(["apps", "status", "mgba"])
        .assert()
        .success()
        .stdout(contains("Managed:           yes"))
        .stdout(contains("Installed version: test"))
        .stdout(contains("Settings:          in sync"));
}

#[test]
fn install_sets_owned_settings_and_keeps_the_rest() {
    let home = TempDir::new().unwrap();
    write_config(
        &home,
        "[ports.qt]\nupdateAutoCheck=1\nfastForwardRatio=3\nbios=/games/gba_bios.bin\n\n[gba.input.QT_K]\nkeyA=88\n",
    );
    let appimage = write_fixture_appimage(&home);

    lum_with_fixture(&home, &appimage)
        .args(["apps", "install", "mgba"])
        .assert()
        .success();

    let config = fs::read_to_string(config_file(&home)).unwrap();
    assert!(config.contains("updateAutoCheck=0\n"), "{config}");
    assert!(config.contains("fastForwardRatio=3\n"), "{config}");
    assert!(config.contains("bios=/games/gba_bios.bin\n"), "{config}");
    assert!(config.contains("[gba.input.QT_K]\nkeyA=88\n"), "{config}");
}

#[test]
fn install_refuses_while_mgba_runs() {
    let home = TempDir::new().unwrap();
    let appimage = write_fixture_appimage(&home);
    // This test process stands in for a running mGBA. Its name in the process
    // table is cut to 15 characters, the way mGBA started through the AppRun
    // symlink is listed as "AppRun": only its executable's file name matches.
    let exe = std::env::current_exe().unwrap();
    let running = exe.file_name().unwrap();
    assert!(running.len() > 15);

    lum_with_fixture(&home, &appimage)
        .env("LUM_APPS_TEST_PROCESS_MGBA", running)
        .args(["apps", "install", "mgba"])
        .assert()
        .failure()
        .stderr(contains("mgba is running; quit it and try again"));

    assert!(!app_dir(&home).exists());
    assert!(!config_file(&home).exists());
}

#[test]
fn install_verifies_the_pinned_checksum_for_the_same_release() {
    let home = TempDir::new().unwrap();
    let appimage = write_fixture_appimage(&home);

    lum_with_fixture(&home, &appimage)
        .args(["apps", "install", "mgba"])
        .assert()
        .success();

    fs::write(&appimage, "tampered").unwrap();
    lum_with_fixture(&home, &appimage)
        .args(["apps", "install", "mgba", "--force"])
        .assert()
        .failure()
        .stderr(contains("checksum mismatch"));
    assert!(app_dir(&home).join("usr/bin/mgba-qt").exists());
}

#[test]
fn install_refuses_an_unmanaged_folder_without_force() {
    let home = TempDir::new().unwrap();
    let app = app_dir(&home);
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("marker"), "manual").unwrap();

    lum(&home)
        .args(["apps", "install", "mgba"])
        .assert()
        .failure()
        .stderr(contains("not managed by lum"));

    assert_eq!(fs::read_to_string(app.join("marker")).unwrap(), "manual");
}

#[test]
fn install_force_takes_over_an_unmanaged_folder() {
    let home = TempDir::new().unwrap();
    let app = app_dir(&home);
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("marker"), "manual").unwrap();
    let appimage = write_fixture_appimage(&home);

    lum_with_fixture(&home, &appimage)
        .args(["apps", "install", "mgba", "--force"])
        .assert()
        .success();

    assert!(!app.join("marker").exists());
    assert!(app.join("usr/bin/mgba-qt").exists());
}

#[test]
fn install_refuses_when_already_installed() {
    let home = TempDir::new().unwrap();
    let appimage = write_fixture_appimage(&home);

    lum_with_fixture(&home, &appimage)
        .args(["apps", "install", "mgba"])
        .assert()
        .success();

    lum_with_fixture(&home, &appimage)
        .args(["apps", "install", "mgba"])
        .assert()
        .failure()
        .stderr(contains("already installed"));
}

#[test]
fn update_requires_an_install() {
    let home = TempDir::new().unwrap();

    lum(&home)
        .args(["apps", "update", "mgba"])
        .assert()
        .failure()
        .stderr(contains("not installed"))
        .stderr(contains("lum apps install mgba"));
}

#[test]
fn update_replaces_an_older_install() {
    let home = TempDir::new().unwrap();
    let app = seed_managed_install(&home, "0.9");
    let appimage = write_fixture_appimage(&home);

    lum_with_fixture(&home, &appimage)
        .args(["apps", "update", "mgba"])
        .assert()
        .success()
        .stdout(contains("\u{2713} Updated mgba 0.9 -> test"));

    assert!(!app.join("marker").exists());
    assert!(app.join("usr/bin/mgba-qt").exists());
    lum_offline(&home)
        .args(["apps", "status", "mgba"])
        .assert()
        .success()
        .stdout(contains("Installed version: test"));
}

#[test]
fn update_reapplies_settings_when_up_to_date() {
    let home = TempDir::new().unwrap();
    let appimage = write_fixture_appimage(&home);
    lum_with_fixture(&home, &appimage)
        .args(["apps", "install", "mgba"])
        .assert()
        .success();
    write_config(&home, "[ports.qt]\nupdateAutoCheck=1\n");

    lum_with_fixture(&home, &appimage)
        .args(["apps", "update", "mgba"])
        .assert()
        .success()
        .stdout(contains("mgba is already up to date (test)"))
        .stdout(contains("\u{2713} Set 1 setting in"));

    let config = fs::read_to_string(config_file(&home)).unwrap();
    assert!(config.contains("updateAutoCheck=0\n"), "{config}");

    lum_with_fixture(&home, &appimage)
        .args(["apps", "update", "mgba", "--force"])
        .assert()
        .success()
        .stdout(contains("\u{2713} Updated mgba test -> test"));
}

#[test]
fn sync_dry_run_changes_nothing() {
    let home = TempDir::new().unwrap();

    lum_with_catalog(&home)
        .args(["apps", "sync", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("\u{2022} mgba: would install test"))
        .stdout(contains("\u{2022} mgba: would set 1 setting in"))
        .stdout(contains("[ports.qt] updateAutoCheck: (unset) -> 0"))
        .stdout(contains("\u{2022} snes9x: would install test"));

    assert!(!app_dir(&home).exists());
    assert!(!home.path().join("apps").join("snes9x").exists());
    assert!(!state_file(&home).exists());
    assert!(!config_file(&home).exists());
    assert!(!desktop_entry(&home).exists());
}

#[test]
fn sync_installs_then_repairs_drifted_settings() {
    let home = TempDir::new().unwrap();

    lum_with_catalog(&home)
        .args(["apps", "sync"])
        .assert()
        .success()
        .stdout(contains("\u{2022} mgba: installing test... done"))
        .stdout(contains("\u{2022} mgba: set 1 setting in"))
        .stdout(contains("\u{2022} snes9x: installing test... done"))
        .stdout(contains(
            "Summary: 2 installed, 0 updated, 0 up to date, 1 configured, 0 skipped",
        ));

    write_config(&home, "[ports.qt]\nupdateAutoCheck=1\nfastForwardRatio=3\n");
    lum_with_catalog(&home)
        .args(["apps", "sync"])
        .assert()
        .success()
        .stdout(contains("\u{2022} mgba: up to date (test)"))
        .stdout(contains("\u{2022} snes9x: up to date (test)"))
        .stdout(contains(
            "Summary: 0 installed, 0 updated, 2 up to date, 1 configured, 0 skipped",
        ));
    let config = fs::read_to_string(config_file(&home)).unwrap();
    assert!(config.contains("updateAutoCheck=0\n"), "{config}");
    assert!(config.contains("fastForwardRatio=3\n"), "{config}");

    lum_with_catalog(&home)
        .args(["apps", "sync"])
        .assert()
        .success()
        .stdout(contains(
            "Summary: 0 installed, 0 updated, 2 up to date, 0 configured, 0 skipped",
        ));
}

#[test]
fn sync_skips_an_unmanaged_folder_without_touching_it() {
    let home = TempDir::new().unwrap();
    let app = app_dir(&home);
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("marker"), "manual").unwrap();

    lum_with_catalog(&home)
        .args(["apps", "sync"])
        .assert()
        .success()
        .stdout(contains("--force"))
        .stdout(contains(
            "Summary: 1 installed, 0 updated, 0 up to date, 0 configured, 1 skipped",
        ));

    assert_eq!(fs::read_to_string(app.join("marker")).unwrap(), "manual");
}

#[test]
fn uninstall_removes_the_app_and_launcher_but_keeps_settings() {
    let home = TempDir::new().unwrap();
    let appimage = write_fixture_appimage(&home);
    lum_with_fixture(&home, &appimage)
        .args(["apps", "install", "mgba"])
        .assert()
        .success();

    lum(&home)
        .args(["apps", "uninstall", "mgba"])
        .assert()
        .success()
        .stdout(contains("\u{2713} Uninstalled mgba from"))
        .stdout(contains(format!(
            "Its settings stay in {}",
            config_file(&home).display()
        )));

    assert!(!app_dir(&home).exists());
    assert!(!desktop_entry(&home).exists());
    assert!(config_file(&home).exists());
    lum_offline(&home)
        .args(["apps", "status", "mgba"])
        .assert()
        .success()
        .stdout(contains("Managed:           no"));
}

#[test]
fn uninstall_requires_an_install() {
    let home = TempDir::new().unwrap();

    lum(&home)
        .args(["apps", "uninstall", "mgba"])
        .assert()
        .failure()
        .stderr(contains("mgba is not installed by lum"));
}

#[test]
fn status_shows_installed_and_latest_versions() {
    let home = TempDir::new().unwrap();

    lum_offline(&home)
        .args(["apps", "status", "mgba"])
        .assert()
        .success()
        .stdout(contains("Installed version: unknown"))
        .stdout(contains("Latest version:    test"));
}

#[test]
fn snes9x_installs_its_appimage_with_its_own_desktop_entry() {
    let home = TempDir::new().unwrap();
    let appimage = write_appimage(&home, &SNES9X);

    lum(&home)
        .env("LUM_APPS_TEST_ARTIFACT_SNES9X", &appimage)
        .args(["apps", "install", "snes9x"])
        .assert()
        .success()
        .stdout(contains("\u{2713} Installed snes9x test"))
        .stdout(contains("Note: Snes9x needs glibmm 2.4 from the system"))
        // On Linux snes9x saves beside each ROM already; lum sets nothing.
        .stdout(contains("Set ").not());

    let app = home.path().join("apps").join("snes9x");
    assert!(app.join("usr/bin/snes9x-gtk").is_file());
    let entry = fs::read_to_string(
        home.path()
            .join("data")
            .join("applications")
            .join("lum-snes9x.desktop"),
    )
    .unwrap();
    assert!(
        entry.contains(&format!("\nExec={} %F\n", app.join("AppRun").display())),
        "{entry}"
    );
    assert!(
        entry.contains(&format!("\nIcon={}\n", app.join("snes9x.png").display())),
        "{entry}"
    );
    assert!(
        entry.contains("application/vnd.nintendo.snes.rom;"),
        "{entry}"
    );

    lum_offline(&home)
        .args(["apps", "status", "snes9x"])
        .assert()
        .success()
        .stdout(contains("Managed:           yes"))
        .stdout(contains("Settings").not());
}

#[test]
fn snes9x_refuses_while_it_runs() {
    let home = TempDir::new().unwrap();
    let appimage = write_appimage(&home, &SNES9X);
    let exe = std::env::current_exe().unwrap();

    lum(&home)
        .env("LUM_APPS_TEST_ARTIFACT_SNES9X", &appimage)
        .env("LUM_APPS_TEST_PROCESS_SNES9X", exe.file_name().unwrap())
        .args(["apps", "install", "snes9x"])
        .assert()
        .failure()
        .stderr(contains("snes9x is running; quit it and try again"));
}
