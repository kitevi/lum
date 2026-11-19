use assert_cmd::Command;
use tempfile::TempDir;

fn lum_with_env(home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"));
    cmd
}

#[test]
fn tools_list_succeeds_with_empty_catalog() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["tools", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("TOOL"));
}

#[test]
fn tools_rejects_unknown_tools() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["tools", "status", "scc"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "unknown tool \"scc\" (none available)",
        ));
}

#[test]
fn tools_uninstall_rejects_unknown_tools() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["tools", "uninstall", "scc"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "unknown tool \"scc\" (none available)",
        ));
}

#[test]
fn tools_sync_with_empty_catalog_does_nothing() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["tools", "sync"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "0 installed, 0 updated, 0 up to date",
        ));
}
