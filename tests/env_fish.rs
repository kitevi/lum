//! Execution tests for fish shell integration.
//!
//! Seam: the real `lum` binary driven from inside real `fish`.
//! These tests source `lum env init --shell fish` and assert observable
//! shell behavior (variables, exit statuses, stdout), never lum internals.
//!
//! Requires `fish` on PATH; skips gracefully when absent.
//! Unix-only: relies on `:` PATH separator and `mktemp` in the wrapper.
#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn fish_available() -> bool {
    Command::new("fish")
        .args(["--no-config", "-c", "echo ok"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn require_fish() -> bool {
    if fish_available() {
        true
    } else {
        eprintln!("skipping fish execution test: fish not on PATH");
        false
    }
}

fn lum_bin_dir() -> PathBuf {
    assert_cmd::cargo::cargo_bin("lum")
        .parent()
        .unwrap()
        .to_owned()
}

struct FishEnv {
    _home: TempDir,
    config: PathBuf,
    data: PathBuf,
    path: String,
}

fn fish_env() -> FishEnv {
    let home = TempDir::new().unwrap();
    let config = home.path().join("config");
    let data = home.path().join("data");
    let path = format!(
        "{}:{}",
        lum_bin_dir().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    FishEnv {
        _home: home,
        config,
        data,
        path,
    }
}

fn run_fish(fenv: &FishEnv, script: &str, extra_envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new("fish");
    cmd.arg("--no-config")
        .arg("-c")
        .arg(script)
        .env("XDG_CONFIG_HOME", &fenv.config)
        .env("XDG_DATA_HOME", &fenv.data)
        .env("PATH", &fenv.path);
    for (k, v) in extra_envs {
        cmd.env(k, v);
    }
    cmd.output().expect("failed to run fish")
}

#[test]
fn fish_set_applies_multiline_value_verbatim() {
    if !require_fish() {
        return;
    }
    let fenv = fish_env();
    let value = "line1\nline2'quote\\back";
    let actual_interactive = fenv.data.join("actual-interactive");
    let actual_replay = fenv.data.join("actual-replay");
    fs::create_dir_all(&fenv.data).unwrap();

    let script = format!(
        r#"command lum env init --shell fish | source
lum env set openrouter $LUM_TEST_VALUE
printf '%s' "$OPENROUTER_API_KEY" > "{actual}"
"#,
        actual = actual_interactive.display()
    );
    let out = run_fish(&fenv, &script, &[("LUM_TEST_VALUE", value)]);
    assert!(
        out.status.success(),
        "interactive set failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        fs::read(&actual_interactive).unwrap(),
        value.as_bytes(),
        "interactive apply mangled the value"
    );

    let replay = format!(
        r#"command lum env init --shell fish | source
printf '%s' "$OPENROUTER_API_KEY" > "{actual}"
"#,
        actual = actual_replay.display()
    );
    let out = run_fish(&fenv, &replay, &[]);
    assert!(out.status.success());
    assert_eq!(
        fs::read(&actual_replay).unwrap(),
        value.as_bytes(),
        "init replay mangled the value"
    );
}

#[test]
fn fish_set_and_unset_propagate_failure_status() {
    if !require_fish() {
        return;
    }
    let fenv = fish_env();
    let script = r#"command lum env init --shell fish | source
lum env set missing value
echo "set_status=$status"
lum env unset missing
echo "unset_status=$status"
"#;
    let out = run_fish(&fenv, script, &[]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("set_status=1"),
        "set failure masked (stdout={stdout:?} stderr={stderr:?})"
    );
    assert!(
        stdout.contains("unset_status=1"),
        "unset failure masked (stdout={stdout:?} stderr={stderr:?})"
    );
    assert!(
        stderr.contains("unknown environment alias"),
        "wrong failure (stderr={stderr:?})"
    );
}

#[test]
fn fish_wrapper_prints_foreign_shell_output_without_evaluating() {
    if !require_fish() {
        return;
    }
    let fenv = fish_env();
    fs::create_dir_all(&fenv.data).unwrap();
    let actual = fenv.data.join("actual");
    let script = format!(
        r#"command lum env set --shell fish openrouter sk-test > /dev/null
command lum env init --shell fish | source
lum env unset --shell posix openrouter
printf '%s' "$OPENROUTER_API_KEY" > "{actual}"
"#,
        actual = actual.display()
    );
    let out = run_fish(&fenv, &script, &[]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "foreign-shell passthrough failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("unset OPENROUTER_API_KEY"),
        "posix output not printed through (stdout={stdout:?})"
    );
    assert_eq!(
        fs::read(&actual).unwrap(),
        b"sk-test",
        "fish evaluated foreign shell code"
    );
}

#[test]
fn fish_wrapper_applies_explicit_equals_shell_form() {
    if !require_fish() {
        return;
    }
    let fenv = fish_env();
    fs::create_dir_all(&fenv.data).unwrap();
    let actual = fenv.data.join("actual");
    let script = format!(
        r#"command lum env init --shell fish | source
lum env set --shell=fish openrouter sk-test
printf '%s' "$OPENROUTER_API_KEY" > "{actual}"
"#,
        actual = actual.display()
    );
    let out = run_fish(&fenv, &script, &[]);
    assert!(
        out.status.success(),
        "equals-form set failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(fs::read(&actual).unwrap(), b"sk-test");
}

#[test]
fn fish_init_is_idempotent_on_path() {
    if !require_fish() {
        return;
    }
    let fenv = fish_env();
    let script = r#"command lum env init --shell fish | source
command lum env init --shell fish | source
set -l bindir (command lum env path)
set -l n 0
for p in $PATH
  if test "$p" = "$bindir"
    set n (math $n + 1)
  end
end
echo "count=$n"
"#;
    let out = run_fish(&fenv, script, &[]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("count=1"), "PATH duplicated: {stdout:?}");
}

#[test]
fn fish_unset_removes_variable_from_current_shell() {
    if !require_fish() {
        return;
    }
    let fenv = fish_env();
    let script = r#"command lum env set --shell fish openrouter sk-test > /dev/null
command lum env init --shell fish | source
lum env unset openrouter
if set -q OPENROUTER_API_KEY; echo "still_set"; else; echo "removed"; end
"#;
    let out = run_fish(&fenv, script, &[]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(stdout.contains("removed"), "var survived unset: {stdout:?}");
}

#[test]
fn fish_wrapper_routes_options_without_evaluating_help() {
    if !require_fish() {
        return;
    }
    let fenv = fish_env();
    for subcommand in ["set", "unset"] {
        for flag in ["--help", "-h"] {
            let script =
                format!("command lum env init --shell fish | source\nlum env {subcommand} {flag}");
            let out = run_fish(&fenv, &script, &[]);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(String::from_utf8_lossy(&out.stdout).contains("Usage:"));
        }
    }
}

#[test]
fn fish_wrapper_respects_option_terminator_and_last_shell() {
    if !require_fish() {
        return;
    }
    let fenv = fish_env();
    for value in ["--shell=posix", "--help", "-h"] {
        let script = "command lum env init --shell fish | source\nlum env set openrouter -- $LUM_TEST_VALUE\nor exit $status\nprintf '%s' \"$OPENROUTER_API_KEY\"";
        let out = run_fish(&fenv, script, &[("LUM_TEST_VALUE", value)]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, value.as_bytes());
    }
    let out = run_fish(
        &fenv,
        "command lum env init --shell fish | source\nlum env set --shell fish --shell=powershell openrouter test",
        &[],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "$env:OPENROUTER_API_KEY = 'test'\n"
    );
}
