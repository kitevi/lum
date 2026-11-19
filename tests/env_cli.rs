use assert_cmd::Command;
use tempfile::TempDir;

fn lum_with_env(home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"));
    cmd
}

#[test]
fn env_set_persists_and_init_replays_export() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "openrouter", "sk-test"])
        .assert()
        .success()
        .stdout("export OPENROUTER_API_KEY='sk-test'\n");

    lum_with_env(&home)
        .args(["env", "init"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "export OPENROUTER_API_KEY='sk-test'",
        ));
}

#[test]
fn env_set_writes_owner_only_state_file() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "openrouter", "secret-value"])
        .assert()
        .success();

    let state = home
        .path()
        .join("config")
        .join("lum")
        .join("env-state.json");
    assert!(state.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&state).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn env_rejects_unknown_aliases() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "missing", "value"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("unknown alias \"missing\""));
}

#[test]
fn env_set_opencode_emits_export_and_list_shows_it() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "opencode", "oc-key"])
        .assert()
        .success()
        .stdout("export OPENCODE_API_KEY='oc-key'\n");

    lum_with_env(&home)
        .args(["env", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "  opencode    OPENCODE_API_KEY         = ********",
        ));

    lum_with_env(&home)
        .args(["env", "init"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "export OPENCODE_API_KEY='oc-key'",
        ));
}

#[test]
fn env_set_opendesign_resolves_to_api_key_variable() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "opendesign", "od-key"])
        .assert()
        .success()
        .stdout("export OPENDESIGN_API_KEY='od-key'\n");

    lum_with_env(&home)
        .args(["env", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "  opendesign  OPENDESIGN_API_KEY       = ********",
        ));
}

#[test]
fn env_set_deploy_emits_export_and_list_shows_it() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "deploy", "dp-test"])
        .assert()
        .success()
        .stdout("export DEPLOY_API_KEY='dp-test'\n");

    lum_with_env(&home)
        .args(["env", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "  deploy      DEPLOY_API_KEY           = ********",
        ));
}

#[test]
fn env_set_keenable_emits_export_and_list_shows_it() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "keenable", "kn-test"])
        .assert()
        .success()
        .stdout("export KEENABLE_API_KEY='kn-test'\n");

    lum_with_env(&home)
        .args(["env", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "  keenable    KEENABLE_API_KEY         = ********",
        ));
}

#[test]
fn env_quotes_shell_values_safely() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "openrouter", "abc'def"])
        .assert()
        .success()
        .stdout("export OPENROUTER_API_KEY='abc'\\''def'\n");
}

#[test]
fn env_list_masks_secret_values_and_shows_forced_defaults() {
    use predicates::prelude::*;

    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "openrouter", "sk-or-v1-secret"])
        .assert()
        .success();

    lum_with_env(&home)
        .args(["env", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("OPENROUTER_API_KEY"))
        .stdout(predicates::str::contains("sk-o...cret"))
        .stdout(predicates::str::contains("npm_config_ignore_scripts"))
        .stdout(predicates::str::contains("true"))
        .stdout(predicates::str::contains("sk-or-v1-secret").not());
}

#[test]
fn env_list_orders_set_aliases_before_unset() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "coralbricks", "cb-test"])
        .assert()
        .success();

    let stdout = String::from_utf8(
        lum_with_env(&home)
            .args(["env", "list"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();

    let coralbricks = stdout
        .find("coralbricks ")
        .expect("coralbricks row printed");
    let keenable = stdout
        .find("keenable ")
        .expect("unset keenable row printed");
    assert!(
        coralbricks < keenable,
        "set alias 'coralbricks' should print before unset 'keenable':\n{stdout}"
    );
}

#[test]
fn env_init_can_emit_powershell_integration() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "init", "--shell", "powershell"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "$env:npm_config_ignore_scripts = 'true'",
        ))
        .stdout(predicates::str::contains("function global:lum"))
        .stdout(predicates::str::contains(
            "lum.exe env set --shell powershell",
        ));
}

#[test]
fn env_set_and_unset_can_emit_powershell_statements() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args([
            "env",
            "set",
            "--shell",
            "powershell",
            "openrouter",
            "abc'def",
        ])
        .assert()
        .success()
        .stdout("$env:OPENROUTER_API_KEY = 'abc''def'\n");

    lum_with_env(&home)
        .args(["env", "unset", "--shell", "powershell", "openrouter"])
        .assert()
        .success()
        .stdout("Remove-Item Env:OPENROUTER_API_KEY -ErrorAction SilentlyContinue\n");
}

#[test]
fn env_init_can_emit_fish_integration() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "--shell", "fish", "openrouter", "sk-test"])
        .assert()
        .success()
        .stdout("set -gx OPENROUTER_API_KEY 'sk-test'\n");

    lum_with_env(&home)
        .args(["env", "init", "--shell", "fish"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "set -gx OPENROUTER_API_KEY 'sk-test'",
        ))
        .stdout(predicates::str::contains(
            "set -gx npm_config_ignore_scripts 'true'",
        ))
        .stdout(predicates::str::contains("if not contains --"))
        .stdout(predicates::str::contains("function lum"))
        .stdout(predicates::str::contains(
            "command lum __completions fish | source",
        ));
}

#[test]
fn env_set_and_unset_can_emit_fish_statements() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "--shell", "fish", "openrouter", "abc'def"])
        .assert()
        .success()
        .stdout("set -gx OPENROUTER_API_KEY 'abc\\'def'\n");

    lum_with_env(&home)
        .args(["env", "unset", "--shell", "fish", "openrouter"])
        .assert()
        .success()
        .stdout("set -e OPENROUTER_API_KEY\n");
}

#[test]
fn env_set_coralbricks_emits_export_and_init_replays() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "coralbricks", "cb-test"])
        .assert()
        .success()
        .stdout("export CORALBRICKS_API_KEY='cb-test'\n");

    lum_with_env(&home)
        .args(["env", "init"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "export CORALBRICKS_API_KEY='cb-test'",
        ));
}
