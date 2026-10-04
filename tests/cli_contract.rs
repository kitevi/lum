//! Process-level contracts for lum's CLI parser surface: completion scripts,
//! the usage spec endpoint, and root help/version/error behavior.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;

#[test]
fn completions_bash_emits_usage_callback_script() {
    Command::cargo_bin("lum")
        .unwrap()
        .args(["__completions", "bash"])
        .assert()
        .success()
        .stdout(predicates::str::contains("__complete_word__"));
}

#[test]
fn completions_generate_for_every_supported_shell() {
    for shell in ["bash", "elvish", "fish", "powershell", "zsh"] {
        Command::cargo_bin("lum")
            .unwrap()
            .args(["__completions", shell])
            .assert()
            .success()
            .stdout(predicates::str::contains("__complete_word__"));
    }
}

#[test]
fn completions_rejects_unsupported_shell() {
    Command::cargo_bin("lum")
        .unwrap()
        .args(["__completions", "nu"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicates::str::contains("invalid value"));
}

#[test]
fn usage_spec_endpoint_emits_kdl() {
    Command::cargo_bin("lum")
        .unwrap()
        .args(["__usage_spec__"])
        .assert()
        .success()
        .stdout(predicates::str::contains("lum"))
        .stdout(predicates::str::contains("radio"))
        .stdout(predicates::str::contains("git-id"));
}

#[test]
fn bare_lum_prints_help_to_stderr_and_exits_2() {
    Command::cargo_bin("lum")
        .unwrap()
        .assert()
        .failure()
        .code(2)
        .stdout(predicates::str::is_empty())
        .stderr(predicates::str::contains("radio"))
        .stderr(predicates::str::contains("Usage"));
}

#[test]
fn help_prints_public_commands_and_hides_internal_ones() {
    Command::cargo_bin("lum")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("radio"))
        .stdout(predicates::str::contains("git-id"))
        .stdout(predicates::str::contains("__completions").not())
        .stdout(predicates::str::contains("__radio_playlist_runner").not())
        .stdout(predicates::str::contains("__git_credential").not());
}

#[test]
fn radio_help_advertises_the_random_selector() {
    Command::cargo_bin("lum")
        .unwrap()
        .args(["radio", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("lum radio rand"));
}

#[test]
fn radio_list_shows_stations_then_the_random_selector() {
    Command::cargo_bin("lum")
        .unwrap()
        .args(["radio", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("atma  atma.fm Channel 1"))
        .stdout(predicates::str::contains(
            "rand  Play a random built-in station",
        ));
}

#[test]
fn invalid_flag_exits_2_with_stderr_diagnostic() {
    Command::cargo_bin("lum")
        .unwrap()
        .arg("--definitely-invalid")
        .assert()
        .failure()
        .code(2)
        .stderr(predicates::str::contains("unexpected argument"));
}

#[test]
fn mirror_watch_hides_cycles_flag_from_help() {
    Command::cargo_bin("lum")
        .unwrap()
        .args(["repos", "mirror", "watch", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("--cycles").not());
}
