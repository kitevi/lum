#![cfg(unix)]

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

/// Fake editor CLI: logs its arguments, lists extensions from a state file, and
/// records `ag-libs.lathe@<version>` taken from the installed vsix file name.
/// Uses shell builtins only, so it runs with an empty PATH.
const FAKE_EDITOR: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$FAKE_EDITOR_LOG"
case "$1" in
  --list-extensions)
    printf '%s\n' "Warning: 'enable-features' is not in the list of known options" >&2
    if [ -f "$FAKE_EDITOR_STATE" ]; then
      while IFS= read -r line; do printf '%s\n' "$line"; done < "$FAKE_EDITOR_STATE"
    fi
    ;;
  --install-extension)
    version=${2##*/lathe-}
    version=${version%.vsix}
    printf 'ag-libs.lathe@%s\n' "$version" >> "$FAKE_EDITOR_STATE"
    ;;
esac
"#;

struct Fixture {
    home: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let home = TempDir::new().unwrap();
        let editor = home.path().join("fake-code");
        write_executable(&editor, FAKE_EDITOR);
        std::fs::create_dir_all(home.path().join("empty-path")).unwrap();
        Self { home }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.home.path().join(name)
    }

    /// A lum command with no Java anywhere and the fake editor as its CLI.
    fn lum(&self) -> Command {
        let mut cmd = Command::cargo_bin("lum").unwrap();
        cmd.env("XDG_CONFIG_HOME", self.path("config"))
            .env("XDG_DATA_HOME", self.path("data"))
            .env("XDG_STATE_HOME", self.path("state"))
            .env("PATH", self.path("empty-path"))
            .env_remove("JAVA_HOME")
            .env_remove("LATHE_JAVA_HOME")
            .env("LUM_VSIX_EDITOR", self.path("fake-code"))
            .env("FAKE_EDITOR_LOG", self.path("editor.log"))
            .env("FAKE_EDITOR_STATE", self.path("editor.state"));
        cmd
    }

    /// A lum command that finds Java through `var`.
    fn lum_with_java(&self, var: &str) -> Command {
        let java_home = self.path("jdk");
        write_executable(&java_home.join("bin").join("java"), "#!/bin/sh\nexit 0\n");
        let mut cmd = self.lum();
        cmd.env(var, java_home);
        cmd
    }

    fn artifact(&self, version: &str) -> PathBuf {
        let path = self.path("artifacts").join(format!("lathe-{version}.vsix"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("vsix {version}")).unwrap();
        path
    }

    fn installed(&self, lines: &str) {
        std::fs::write(self.path("editor.state"), lines).unwrap();
    }

    fn editor_log(&self) -> String {
        std::fs::read_to_string(self.path("editor.log")).unwrap_or_default()
    }
}

fn write_executable(path: &Path, contents: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn install_without_java_fails_before_touching_the_editor() {
    let fixture = Fixture::new();
    let artifact = fixture.artifact("0.1.24");

    fixture
        .lum()
        .env("LUM_VSIX_TEST_ARTIFACT_LATHE", &artifact)
        .args(["vsix", "install", "lathe"])
        .assert()
        .failure()
        .stderr(contains("lathe requires Java 17+, but none was found"))
        .stderr(contains("$LATHE_JAVA_HOME, $JAVA_HOME, PATH"));

    assert_eq!(fixture.editor_log(), "");
}

#[test]
fn install_passes_vsix_to_editor_and_verifies_the_result() {
    let fixture = Fixture::new();
    let artifact = fixture.artifact("0.1.24");

    fixture
        .lum_with_java("JAVA_HOME")
        .env("LUM_VSIX_TEST_ARTIFACT_LATHE", &artifact)
        .args(["vsix", "install", "lathe"])
        .assert()
        .success()
        .stdout(contains("\u{2713} Installed lathe 0.1.24 into fake-code"))
        .stdout(contains("Note:").not());

    let log = fixture.editor_log();
    assert!(log.contains("--install-extension "), "{log}");
    assert!(log.contains("/lathe-0.1.24.vsix --force"), "{log}");
}

#[test]
fn lathe_java_home_alone_satisfies_the_java_requirement() {
    let fixture = Fixture::new();
    let artifact = fixture.artifact("0.1.24");

    fixture
        .lum_with_java("LATHE_JAVA_HOME")
        .env("LUM_VSIX_TEST_ARTIFACT_LATHE", &artifact)
        .args(["vsix", "install", "lathe"])
        .assert()
        .success();
}

#[test]
fn install_refuses_when_already_installed_unless_forced() {
    let fixture = Fixture::new();
    let artifact = fixture.artifact("0.1.24");
    fixture.installed("ag-libs.lathe@0.1.23\n");

    fixture
        .lum_with_java("JAVA_HOME")
        .env("LUM_VSIX_TEST_ARTIFACT_LATHE", &artifact)
        .args(["vsix", "install", "lathe"])
        .assert()
        .failure()
        .stderr(contains("lathe 0.1.23 is already installed"));
    assert!(!fixture.editor_log().contains("--install-extension"));

    fixture
        .lum_with_java("JAVA_HOME")
        .env("LUM_VSIX_TEST_ARTIFACT_LATHE", &artifact)
        .args(["vsix", "install", "lathe", "--force"])
        .assert()
        .success()
        .stdout(contains("Installed lathe 0.1.24"));
}

#[test]
fn update_installs_newer_release_and_skips_current_one() {
    let fixture = Fixture::new();
    fixture.installed("ag-libs.lathe@0.1.23\n");

    fixture
        .lum_with_java("JAVA_HOME")
        .env("LUM_VSIX_TEST_ARTIFACT_LATHE", fixture.artifact("0.1.24"))
        .args(["vsix", "update", "lathe"])
        .assert()
        .success()
        .stdout(contains("\u{2713} Updated lathe 0.1.23 -> 0.1.24"));

    fixture
        .lum_with_java("JAVA_HOME")
        .env("LUM_VSIX_TEST_ARTIFACT_LATHE", fixture.artifact("0.1.24"))
        .args(["vsix", "update", "lathe"])
        .assert()
        .success()
        .stdout(contains("lathe is already up to date (0.1.24)"));
}

#[test]
fn update_requires_an_existing_install() {
    let fixture = Fixture::new();

    fixture
        .lum_with_java("JAVA_HOME")
        .env("LUM_VSIX_TEST_ARTIFACT_LATHE", fixture.artifact("0.1.24"))
        .args(["vsix", "update", "lathe"])
        .assert()
        .failure()
        .stderr(contains("lathe is not installed in fake-code"));
}

#[test]
fn install_warns_about_conflicting_extensions() {
    let fixture = Fixture::new();
    fixture.installed("redhat.java@1.58.0\n");

    fixture
        .lum_with_java("JAVA_HOME")
        .env("LUM_VSIX_TEST_ARTIFACT_LATHE", fixture.artifact("0.1.24"))
        .args(["vsix", "install", "lathe"])
        .assert()
        .success()
        .stdout(contains("Note: redhat.java is also installed in fake-code"));
}

#[test]
fn sync_dry_run_reports_without_installing() {
    let fixture = Fixture::new();
    fixture.installed("ag-libs.lathe@0.1.23\n");

    fixture
        .lum_with_java("JAVA_HOME")
        .env("LUM_VSIX_TEST_ARTIFACT_LATHE", fixture.artifact("0.1.24"))
        .args(["vsix", "sync", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("lathe: would update 0.1.23 -> 0.1.24"));

    assert!(!fixture.editor_log().contains("--install-extension"));
}

#[test]
fn sync_skips_extensions_with_missing_requirements() {
    let fixture = Fixture::new();

    fixture
        .lum()
        .env("LUM_VSIX_TEST_ARTIFACT_LATHE", fixture.artifact("0.1.24"))
        .args(["vsix", "sync"])
        .assert()
        .success()
        .stdout(contains("lathe: skipped; lathe requires Java 17+"))
        .stdout(contains("0 installed, 0 updated, 0 up to date, 1 skipped"));

    assert!(!fixture.editor_log().contains("--install-extension"));
}

#[test]
fn sync_installs_missing_then_reports_up_to_date() {
    let fixture = Fixture::new();

    fixture
        .lum_with_java("JAVA_HOME")
        .env("LUM_VSIX_TEST_ARTIFACT_LATHE", fixture.artifact("0.1.24"))
        .args(["vsix", "sync"])
        .assert()
        .success()
        .stdout(contains("lathe: installing 0.1.24... done"))
        .stdout(contains("1 installed"));

    fixture
        .lum_with_java("JAVA_HOME")
        .env("LUM_VSIX_TEST_ARTIFACT_LATHE", fixture.artifact("0.1.24"))
        .args(["vsix", "sync"])
        .assert()
        .success()
        .stdout(contains("lathe: up to date (0.1.24)"));
}

#[test]
fn status_and_version_read_the_editor_list() {
    let fixture = Fixture::new();
    fixture.installed("ag-libs.lathe@0.1.23\nredhat.java@1.58.0\n");

    fixture
        .lum()
        .args(["vsix", "status", "lathe"])
        .assert()
        .success()
        .stdout(contains("ID:                ag-libs.lathe"))
        .stdout(contains("Java 17+:          missing"))
        .stdout(contains("Installed version: 0.1.23"))
        .stdout(contains("Conflicts:         redhat.java"));

    fixture
        .lum()
        .env("LUM_VSIX_TEST_ARTIFACT_LATHE", fixture.artifact("0.1.24"))
        .args(["vsix", "version", "lathe"])
        .assert()
        .success()
        .stdout(contains("Installed: 0.1.23"))
        .stdout(contains("Latest:    0.1.24"));
}

#[test]
fn ls_lists_catalog_with_installed_version() {
    let fixture = Fixture::new();
    fixture.installed("ag-libs.lathe@0.1.23\n");

    fixture
        .lum()
        .args(["vsix", "ls"])
        .assert()
        .success()
        .stdout(contains("ag-libs.lathe"))
        .stdout(contains("0.1.23"))
        .stdout(contains("Java 17+ (missing)"));
}

#[test]
fn unknown_extension_lists_available_ones() {
    let fixture = Fixture::new();

    fixture
        .lum()
        .args(["vsix", "status", "wat"])
        .assert()
        .failure()
        .stderr(contains("unknown managed extension \"wat\""))
        .stderr(contains("lathe"));
}
