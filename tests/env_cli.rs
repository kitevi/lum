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
fn env_rejects_unknown_aliases() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "missing", "value"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "unknown environment alias: missing",
        ));
}

#[test]
fn env_set_opencode_emits_export_and_aliases_lists_it() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "opencode", "oc-key"])
        .assert()
        .success()
        .stdout("export OPENCODE_API_KEY='oc-key'\n");

    lum_with_env(&home)
        .args(["env", "aliases"])
        .assert()
        .success()
        .stdout(predicates::str::contains("opencode   → OPENCODE_API_KEY"));

    lum_with_env(&home)
        .args(["env", "init"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "export OPENCODE_API_KEY='oc-key'",
        ));
}

#[test]
fn env_set_deepseek_emits_export_and_aliases_lists_it() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "deepseek", "sk-deepseek"])
        .assert()
        .success()
        .stdout("export DEEPSEEK_API_KEY='sk-deepseek'\n");

    lum_with_env(&home)
        .args(["env", "aliases"])
        .assert()
        .success()
        .stdout(predicates::str::contains("deepseek   → DEEPSEEK_API_KEY"));

    lum_with_env(&home)
        .args(["env", "init"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "export DEEPSEEK_API_KEY='sk-deepseek'",
        ));
}

#[test]
fn env_set_zro_emits_export_and_aliases_lists_it() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "zro", "zro-key"])
        .assert()
        .success()
        .stdout("export ZRO_API_KEY='zro-key'\n");

    lum_with_env(&home)
        .args(["env", "aliases"])
        .assert()
        .success()
        .stdout(predicates::str::contains("zro        → ZRO_API_KEY"));

    lum_with_env(&home)
        .args(["env", "unset", "zro"])
        .assert()
        .success()
        .stdout("unset ZRO_API_KEY\n");
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
        .args(["env", "set", "zro", "zro-key"])
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

    let zro = stdout.find("zro ").expect("zro row printed");
    let deepseek = stdout.find("deepseek").expect("unset deepseek row printed");
    assert!(
        zro < deepseek,
        "set alias 'zro' should print before unset 'deepseek':\n{stdout}"
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
fn env_set_hypercharm_emits_export_and_init_replays() {
    let home = TempDir::new().unwrap();

    lum_with_env(&home)
        .args(["env", "set", "hypercharm", "hc-test"])
        .assert()
        .success()
        .stdout("export HYPERCHARM_API_KEY='hc-test'\n");

    lum_with_env(&home)
        .args(["env", "init"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "export HYPERCHARM_API_KEY='hc-test'",
        ));
}
