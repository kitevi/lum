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
fn tools_update_reports_invalid_version_unless_forced() {
    let home = TempDir::new().unwrap();
    let artifact = home.path().join("scc-source");
    std::fs::write(&artifact, "fake scc").unwrap();
    lum_with_env(&home)
        .env("LUM_TOOLS_TEST_ARTIFACT_SCC", &artifact)
        .args(["tools", "install", "scc"])
        .assert()
        .success();
    let state_path = home.path().join("config/lum/tools-state.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state_path).unwrap()).unwrap();
    state["tools"]["scc"]["installed_version"] = "unsupported".into();
    std::fs::write(&state_path, serde_json::to_vec_pretty(&state).unwrap()).unwrap();
    let before = std::fs::read(&state_path).unwrap();

    lum_with_env(&home)
        .env("LUM_TOOLS_TEST_ARTIFACT_SCC", &artifact)
        .args(["tools", "update", "scc"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid version"));
    assert_eq!(std::fs::read(&state_path).unwrap(), before);

    lum_with_env(&home)
        .env("LUM_TOOLS_TEST_ARTIFACT_SCC", &artifact)
        .args(["tools", "update", "scc", "--force"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Updated scc"));
}

#[test]
fn tools_sync_leaves_identical_local_artifacts_unchanged() {
    let home = TempDir::new().unwrap();
    let artifact = home.path().join("source");
    std::fs::write(&artifact, "fake tool").unwrap();
    for tool in ["scc", "universal-ctags"] {
        lum_with_env(&home)
            .env("LUM_TOOLS_TEST_ARTIFACT_SCC", &artifact)
            .env("LUM_TOOLS_TEST_ARTIFACT_UNIVERSAL_CTAGS", &artifact)
            .args(["tools", "install", tool])
            .assert()
            .success();
    }
    let state_path = home.path().join("config/lum/tools-state.json");
    let before = std::fs::read(&state_path).unwrap();

    lum_with_env(&home)
        .env("LUM_TOOLS_TEST_ARTIFACT_SCC", &artifact)
        .env("LUM_TOOLS_TEST_ARTIFACT_UNIVERSAL_CTAGS", &artifact)
        .args(["tools", "sync"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "0 installed, 0 updated, 2 up to date",
        ));
    assert_eq!(std::fs::read(state_path).unwrap(), before);
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
