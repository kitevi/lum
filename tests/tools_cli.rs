use assert_cmd::Command;
use tempfile::TempDir;

fn lum_with_env(home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"));
    cmd
}

#[test]
fn tools_ls_lists_the_managed_catalog() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["tools", "ls"])
        .assert()
        .success()
        .stdout(predicates::str::contains("scc"))
        .stdout(predicates::str::contains("universal-ctags"));
}

#[test]
fn tools_status_reports_missing_tool() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["tools", "status", "scc"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Tool:              scc"))
        .stdout(predicates::str::contains("Managed:           no"))
        .stdout(predicates::str::contains("Exists:            no"));
}

#[test]
fn tools_rejects_unknown_tools_with_available_names() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["tools", "status", "missing"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "unknown managed tool \"missing\"",
        ))
        .stderr(predicates::str::contains("scc"));
}

#[test]
fn tools_status_reports_unmanaged_existing_binary() {
    let home = TempDir::new().unwrap();
    let bin = home.path().join("data").join("lum").join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("scc"), "manual").unwrap();

    lum_with_env(&home)
        .args(["tools", "status", "scc"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Managed:           no"))
        .stdout(predicates::str::contains("Exists:            yes"));
}

#[test]
fn tools_install_uses_local_artifact_override_and_records_managed_state() {
    let home = TempDir::new().unwrap();
    let artifact = home.path().join("scc-source");
    std::fs::write(&artifact, "fake scc").unwrap();

    lum_with_env(&home)
        .env("LUM_TOOLS_TEST_ARTIFACT_SCC", &artifact)
        .args(["tools", "install", "scc"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Installed scc"));

    lum_with_env(&home)
        .args(["tools", "status", "scc"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Managed:           yes"))
        .stdout(predicates::str::contains("Exists:            yes"));
}

#[test]
fn tools_install_verifies_pinned_checksum_for_same_release() {
    let home = TempDir::new().unwrap();
    let artifact = home.path().join("scc-source");
    std::fs::write(&artifact, "fake scc").unwrap();

    lum_with_env(&home)
        .env("LUM_TOOLS_TEST_ARTIFACT_SCC", &artifact)
        .args(["tools", "install", "scc"])
        .assert()
        .success();

    std::fs::write(&artifact, "fake scc (tampered)").unwrap();
    lum_with_env(&home)
        .env("LUM_TOOLS_TEST_ARTIFACT_SCC", &artifact)
        .args(["tools", "install", "scc", "--force"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("checksum mismatch"));

    let fresh = TempDir::new().unwrap();
    lum_with_env(&fresh)
        .env("LUM_TOOLS_TEST_ARTIFACT_SCC", &artifact)
        .args(["tools", "install", "scc"])
        .assert()
        .success();
}

#[test]
fn tools_install_protects_unmanaged_files_without_force() {
    let home = TempDir::new().unwrap();
    let bin = home.path().join("data").join("lum").join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("scc"), "manual").unwrap();
    let artifact = home.path().join("scc-source");
    std::fs::write(&artifact, "fake scc").unwrap();

    lum_with_env(&home)
        .env("LUM_TOOLS_TEST_ARTIFACT_SCC", &artifact)
        .args(["tools", "install", "scc"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not managed by lum"));
}

#[test]
fn tools_install_force_takes_over_unmanaged_files() {
    let home = TempDir::new().unwrap();
    let bin = home.path().join("data").join("lum").join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("scc"), "manual").unwrap();
    let artifact = home.path().join("scc-source");
    std::fs::write(&artifact, "fake scc").unwrap();

    lum_with_env(&home)
        .env("LUM_TOOLS_TEST_ARTIFACT_SCC", &artifact)
        .args(["tools", "install", "scc", "--force"])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(bin.join("scc")).unwrap(),
        "fake scc"
    );
}
