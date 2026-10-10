use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use tempfile::TempDir;

/// Helper: create a git repo with one commit (clean state).
fn create_clean_repo(dir: &std::path::Path) {
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(dir)
        .output()
        .expect("git init");
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(dir)
        .output()
        .expect("git config email");
    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir)
        .output()
        .expect("git config name");
    std::process::Command::new("git")
        .args(["config", "commit.gpgsign", "false"])
        .current_dir(dir)
        .output()
        .expect("git config gpgsign");
    std::fs::write(dir.join("README"), "hello").expect("write README");
    std::process::Command::new("git")
        .args(["add", "README"])
        .current_dir(dir)
        .output()
        .expect("git add");
    std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(dir)
        .output()
        .expect("git commit");
}

/// Helper: create a git repo with uncommitted changes (dirty state).
fn create_dirty_repo(dir: &std::path::Path) {
    create_clean_repo(dir);
    std::fs::write(dir.join("README"), "changed").expect("modify README");
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(dir: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn repos_scan_in(dir: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.args(["repos", "scan"]).current_dir(dir);
    cmd
}

fn repos_scan_offline_in(dir: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.args(["repos", "scan", "--offline"]).current_dir(dir);
    cmd
}

#[test]
fn scan_reports_clean_repo() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("my-repo");
    std::fs::create_dir_all(&repo).unwrap();
    create_clean_repo(&repo);

    repos_scan_in(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("my-repo"))
        .stdout(predicates::str::contains("clean"));
}

#[test]
fn scan_notes_when_branch_has_no_upstream() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("no-upstream");
    std::fs::create_dir_all(&repo).unwrap();
    create_clean_repo(&repo);

    repos_scan_in(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("no-upstream"))
        .stdout(predicates::str::contains(
            "none; fetch skipped (no upstream)",
        ));
}

#[test]
fn scan_notes_when_repo_is_detached() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("detached");
    std::fs::create_dir_all(&repo).unwrap();
    create_clean_repo(&repo);
    git(&repo, &["checkout", "--detach"]);

    repos_scan_in(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("detached"))
        .stdout(predicates::str::contains("fetch skipped (detached HEAD)"));
}

#[test]
fn scan_reports_dirty_repo() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("dirty-repo");
    std::fs::create_dir_all(&repo).unwrap();
    create_dirty_repo(&repo);

    repos_scan_in(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("dirty-repo"))
        .stdout(predicates::str::contains("has uncommitted changes"));
}

#[test]
fn scan_fetches_before_reporting_upstream_status() {
    let tmp = TempDir::new().unwrap();
    let origin = tmp.path().join("origin.git");
    let first = tmp.path().join("first");
    let second = tmp.path().join("second");
    let scan_root = tmp.path().join("scan-root");
    let scanned = scan_root.join("scanned");

    git(tmp.path(), &["init", "--bare", origin.to_str().unwrap()]);
    git(
        tmp.path(),
        &["clone", origin.to_str().unwrap(), first.to_str().unwrap()],
    );
    git(&first, &["config", "user.email", "test@example.com"]);
    git(&first, &["config", "user.name", "Test"]);
    git(&first, &["checkout", "-b", "main"]);
    std::fs::write(first.join("README"), "one").unwrap();
    git(&first, &["add", "README"]);
    git(&first, &["commit", "-m", "init"]);
    git(&first, &["push", "-u", "origin", "main"]);
    git(&origin, &["symbolic-ref", "HEAD", "refs/heads/main"]);

    std::fs::create_dir_all(&scan_root).unwrap();
    git(
        &scan_root,
        &["clone", origin.to_str().unwrap(), scanned.to_str().unwrap()],
    );

    git(
        tmp.path(),
        &["clone", origin.to_str().unwrap(), second.to_str().unwrap()],
    );
    git(&second, &["config", "user.email", "test@example.com"]);
    git(&second, &["config", "user.name", "Test"]);
    std::fs::write(second.join("README"), "two").unwrap();
    git(&second, &["commit", "-am", "advance origin"]);
    git(&second, &["push"]);

    repos_scan_in(&scan_root)
        .assert()
        .success()
        .stdout(predicates::str::contains("scanned"))
        .stdout(predicates::str::contains("behind origin/main; [behind 1]"));
}

#[test]
fn scan_offline_uses_cached_refs_without_fetching() {
    let tmp = TempDir::new().unwrap();
    let origin = tmp.path().join("origin.git");
    let first = tmp.path().join("first");
    let second = tmp.path().join("second");
    let scan_root = tmp.path().join("scan-root");
    let scanned = scan_root.join("scanned");

    git(tmp.path(), &["init", "--bare", origin.to_str().unwrap()]);
    git(
        tmp.path(),
        &["clone", origin.to_str().unwrap(), first.to_str().unwrap()],
    );
    git(&first, &["config", "user.email", "test@example.com"]);
    git(&first, &["config", "user.name", "Test"]);
    git(&first, &["checkout", "-b", "main"]);
    std::fs::write(first.join("README"), "one").unwrap();
    git(&first, &["add", "README"]);
    git(&first, &["commit", "-m", "init"]);
    git(&first, &["push", "-u", "origin", "main"]);
    git(&origin, &["symbolic-ref", "HEAD", "refs/heads/main"]);

    std::fs::create_dir_all(&scan_root).unwrap();
    git(
        &scan_root,
        &["clone", origin.to_str().unwrap(), scanned.to_str().unwrap()],
    );

    git(
        tmp.path(),
        &["clone", origin.to_str().unwrap(), second.to_str().unwrap()],
    );
    git(&second, &["config", "user.email", "test@example.com"]);
    git(&second, &["config", "user.name", "Test"]);
    std::fs::write(second.join("README"), "two").unwrap();
    git(&second, &["commit", "-am", "advance origin"]);
    git(&second, &["push"]);

    repos_scan_offline_in(&scan_root)
        .assert()
        .success()
        .stdout(predicates::str::contains("synced with origin/main"))
        .stdout(predicates::str::contains("behind origin/main").not())
        .stderr(predicates::str::contains(
            "info: offline mode; using cached remote refs only",
        ));
}

#[test]
fn scan_reports_fetch_failure_without_failing_command() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("bad-remote");
    std::fs::create_dir_all(&repo).unwrap();
    create_clean_repo(&repo);
    let branch = git_stdout(&repo, &["branch", "--show-current"]);
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "file:///definitely/missing/lum-test.git",
        ],
    );
    git(
        &repo,
        &["config", &format!("branch.{branch}.remote"), "origin"],
    );
    git(
        &repo,
        &[
            "config",
            &format!("branch.{branch}.merge"),
            &format!("refs/heads/{branch}"),
        ],
    );

    repos_scan_in(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "fetch failed; status may be stale",
        ))
        .stderr(predicates::str::contains("warning: git fetch failed"));
}

#[test]
fn scan_finds_nested_repos() {
    let tmp = TempDir::new().unwrap();
    let outer = tmp.path().join("outer");
    let inner = outer.join("inner");
    std::fs::create_dir_all(&inner).unwrap();
    create_clean_repo(&outer);
    create_clean_repo(&inner);

    repos_scan_in(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("outer"))
        .stdout(predicates::str::contains("inner"));
}

#[test]
fn scan_skips_hidden_dirs_by_default() {
    let tmp = TempDir::new().unwrap();
    let visible = tmp.path().join("visible-repo");
    let hidden = tmp.path().join(".hidden-repo");
    std::fs::create_dir_all(&visible).unwrap();
    std::fs::create_dir_all(&hidden).unwrap();
    create_clean_repo(&visible);
    create_clean_repo(&hidden);

    repos_scan_in(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("visible-repo"))
        .stdout(predicates::str::contains("hidden-repo").not());
}

#[test]
fn scan_hidden_flag_includes_hidden_dirs() {
    let tmp = TempDir::new().unwrap();
    let visible = tmp.path().join("visible-repo");
    let hidden = tmp.path().join(".hidden-repo");
    std::fs::create_dir_all(&visible).unwrap();
    std::fs::create_dir_all(&hidden).unwrap();
    create_clean_repo(&visible);
    create_clean_repo(&hidden);

    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.args(["repos", "scan", "--hidden"])
        .current_dir(tmp.path());
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("visible-repo"))
        .stdout(predicates::str::contains("hidden-repo"));
}

#[test]
fn scan_with_explicit_path() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("my-repo");
    std::fs::create_dir_all(&repo).unwrap();
    create_clean_repo(&repo);

    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.args(["repos", "scan", &tmp.path().to_string_lossy()]);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("my-repo"));
}

#[test]
fn scan_accepts_jobs_limit_flag() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("my-repo");
    std::fs::create_dir_all(&repo).unwrap();
    create_clean_repo(&repo);

    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.args(["repos", "scan", "-j", "1", &tmp.path().to_string_lossy()]);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("my-repo"));
}

#[test]
fn scan_does_not_apply_ignore_files_or_enter_git_metadata() {
    let tmp = TempDir::new().unwrap();
    create_clean_repo(tmp.path());
    std::fs::write(tmp.path().join(".gitignore"), "git-excluded-repo/\n").unwrap();
    std::fs::write(tmp.path().join(".ignore"), "ignore-excluded-repo/\n").unwrap();
    for name in ["git-excluded-repo", "ignore-excluded-repo"] {
        let repo = tmp.path().join(name);
        std::fs::create_dir(&repo).unwrap();
        create_clean_repo(&repo);
    }
    std::fs::create_dir_all(tmp.path().join(".git/metadata-repo/.git")).unwrap();

    let mut cmd = repos_scan_offline_in(tmp.path());
    cmd.arg("--hidden");
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("git-excluded-repo"))
        .stdout(predicates::str::contains("ignore-excluded-repo"))
        .stdout(predicates::str::contains("metadata-repo").not());
}

#[cfg(unix)]
#[test]
fn scan_skips_symlinked_repositories() {
    let tmp = TempDir::new().unwrap();
    let external = TempDir::new().unwrap();
    create_clean_repo(external.path());
    std::os::unix::fs::symlink(external.path(), tmp.path().join("linked-repo")).unwrap();

    repos_scan_offline_in(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("linked-repo").not());
}

#[test]
fn scan_accepts_explicit_hidden_root() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join(".hidden-repo");
    std::fs::create_dir(&repo).unwrap();
    create_clean_repo(&repo);

    let mut cmd = repos_scan_offline_in(tmp.path());
    cmd.arg(&repo);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains(".hidden-repo"));
}

#[test]
fn scan_reports_worktree_with_git_file() {
    let tmp = TempDir::new().unwrap();
    let main = tmp.path().join("main-repo");
    let linked = tmp.path().join("linked-worktree");
    std::fs::create_dir(&main).unwrap();
    create_clean_repo(&main);
    git(
        &main,
        &["worktree", "add", "-b", "linked", linked.to_str().unwrap()],
    );
    assert!(linked.join(".git").is_file());

    repos_scan_offline_in(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("main-repo"))
        .stdout(predicates::str::contains("linked-worktree"));
}

#[test]
fn scan_warns_about_missing_root() {
    let tmp = TempDir::new().unwrap();
    let mut cmd = repos_scan_offline_in(tmp.path());
    cmd.arg(tmp.path().join("missing"));
    cmd.assert()
        .success()
        .stdout(predicates::str::is_empty())
        .stderr(predicates::str::contains("warning:"));
}

#[test]
fn scan_reports_repositories_in_path_order() {
    let tmp = TempDir::new().unwrap();
    for name in ["zeta", "alpha", "middle"] {
        let repo = tmp.path().join(name);
        std::fs::create_dir(&repo).unwrap();
        create_clean_repo(&repo);
    }
    let output = repos_scan_offline_in(tmp.path())
        .args(["-j", "2"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let names: Vec<_> = text
        .lines()
        .map(|line| {
            std::path::Path::new(line.split("  ").next().unwrap())
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(names, ["alpha", "middle", "zeta"]);
}

#[test]
fn scan_reports_missing_git_before_scanning() {
    let tmp = TempDir::new().unwrap();
    let empty_path = tmp.path().join("empty-path");
    std::fs::create_dir_all(&empty_path).unwrap();

    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("PATH", empty_path)
        .args(["repos", "scan", tmp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "git executable not found on PATH",
        ));
}

fn lum_with_xdg(home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("XDG_STATE_HOME", home.path().join("state"))
        .env("XDG_DOCUMENTS_DIR", home.path().join("Documents"))
        .env("LUM_NO_NOTIFY", "1");
    cmd
}

#[test]
fn mirror_sync_reports_missing_git_before_loading_work() {
    let tmp = TempDir::new().unwrap();
    let empty_path = tmp.path().join("empty-path");
    std::fs::create_dir_all(&empty_path).unwrap();

    let mut cmd = lum_with_xdg(&tmp);
    cmd.env("PATH", empty_path)
        .args(["repos", "mirror", "sync"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "git executable not found on PATH",
        ));
}

// --- mirror tests ---

#[test]
fn mirror_config_path_prints_path() {
    let home = TempDir::new().unwrap();
    lum_with_xdg(&home)
        .args(["repos", "mirror", "config-path"])
        .assert()
        .success()
        .stdout(predicates::str::contains("repos.json"));
}

#[test]
fn mirror_dir_prints_documents_codemirror() {
    let home = TempDir::new().unwrap();
    lum_with_xdg(&home)
        .args(["repos", "mirror", "dir"])
        .assert()
        .success()
        .stdout(predicates::str::contains("CodeMirror"));
}

#[test]
fn mirror_init_creates_config() {
    let home = TempDir::new().unwrap();
    lum_with_xdg(&home)
        .args(["repos", "mirror", "init"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Created"));

    // Config file should now exist
    let config_path = home.path().join("config").join("lum").join("repos.json");
    assert!(config_path.exists());
}

#[test]
fn mirror_init_does_not_overwrite() {
    let home = TempDir::new().unwrap();
    lum_with_xdg(&home)
        .args(["repos", "mirror", "init"])
        .assert()
        .success();

    lum_with_xdg(&home)
        .args(["repos", "mirror", "init"])
        .assert()
        .success()
        .stdout(predicates::str::contains("already exists"));
}

#[test]
fn mirror_list_shows_configured_repos() {
    let home = TempDir::new().unwrap();
    lum_with_xdg(&home)
        .args(["repos", "mirror", "init"])
        .assert()
        .success();

    lum_with_xdg(&home)
        .args(["repos", "mirror", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("example-repo-main-sample"));
}

#[test]
fn mirror_list_reports_no_config() {
    let home = TempDir::new().unwrap();
    lum_with_xdg(&home)
        .args(["repos", "mirror", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("not found"));
}

#[test]
fn mirror_sync_reports_json_parse_details() {
    let home = TempDir::new().unwrap();
    let config_dir = home.path().join("config").join("lum");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("repos.json"),
        r#"{"repos":[{"url":"https://github.com/example/one.git"}]"#,
    )
    .unwrap();

    lum_with_xdg(&home)
        .args(["repos", "mirror", "sync"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("parsing"))
        .stderr(predicates::str::contains("repos.json"))
        .stderr(predicates::str::contains("EOF"));
}

fn write_mirror_config(home: &TempDir, repos: &[(&str, &str)]) {
    let config_dir = home.path().join("config").join("lum");
    std::fs::create_dir_all(&config_dir).unwrap();
    let entries: Vec<_> = repos
        .iter()
        .map(|(url, branch)| serde_json::json!({ "url": url, "branch": branch }))
        .collect();
    let config = serde_json::json!({ "repos": entries });
    std::fs::write(
        config_dir.join("repos.json"),
        serde_json::to_string(&config).unwrap(),
    )
    .unwrap();
}

#[cfg(unix)]
fn fake_git_dir(home: &TempDir) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let bin = home.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let git = bin.join("git");
    std::fs::write(
        &git,
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo git version 2.0.0
  exit 0
fi
if [ "$1" = "clone" ]; then
  dst=""
  for arg in "$@"; do dst="$arg"; done
  sleep 1
  mkdir -p "$dst/.git"
  exit 0
fi
if [ "$1" = "-C" ] && [ "$3" = "rev-parse" ]; then
  sleep 1
  if [ "$4" = "--abbrev-ref" ]; then
    echo main
  else
    echo abc
  fi
  exit 0
fi
echo "unexpected git invocation: $@" >&2
exit 1
"#,
    )
    .unwrap();
    let mut perms = std::fs::metadata(&git).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&git, perms).unwrap();
    bin
}

#[cfg(unix)]
#[test]
fn mirror_sync_reuses_worker_before_slow_job_finishes() {
    let home = TempDir::new().unwrap();
    write_mirror_config(
        &home,
        &[
            ("https://example.com/slow.git", "main"),
            ("https://example.com/fast.git", "main"),
            ("https://example.com/next.git", "main"),
        ],
    );
    let state = home.path().join("scheduler");
    std::fs::create_dir(&state).unwrap();
    let fake_bin = fake_git_dir(&home);
    std::fs::write(
        fake_bin.join("git"),
        r#"#!/bin/sh
set -eu
[ "$1" = clone ] || exit 1
dst=""
for arg in "$@"; do dst="$arg"; done
case "$*" in
  *"/slow.git"*)
    attempts=0
    while [ ! -f "$LUM_TEST_SCHEDULER_DIR/next-started" ]; do
      attempts=$((attempts + 1))
      if [ "$attempts" -ge 200 ]; then
        echo "an available worker was blocked by the slow job" >&2
        exit 1
      fi
      sleep 0.025
    done
    ;;
  *"/next.git"*) touch "$LUM_TEST_SCHEDULER_DIR/next-started" ;;
esac
mkdir -p "$dst/.git"
"#,
    )
    .unwrap();
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    lum_with_xdg(&home)
        .env("PATH", path)
        .env("LUM_TEST_SCHEDULER_DIR", &state)
        .args(["repos", "mirror", "sync", "-j", "2"])
        .assert()
        .success();
    assert!(state.join("next-started").is_file());
}

#[cfg(unix)]
#[test]
fn mirror_sync_continues_after_individual_errors() {
    let home = TempDir::new().unwrap();
    write_mirror_config(
        &home,
        &[
            ("https://example.com/bad.git", "main"),
            ("https://example.com/one.git", "main"),
            ("https://example.com/two.git", "main"),
        ],
    );
    let fake_bin = fake_git_dir(&home);
    std::fs::write(
        fake_bin.join("git"),
        r#"#!/bin/sh
case "$*" in *"/bad.git"*) echo "fixture clone failure" >&2; exit 1 ;; esac
dst=""
for arg in "$@"; do dst="$arg"; done
mkdir -p "$dst/.git"
"#,
    )
    .unwrap();
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    lum_with_xdg(&home)
        .env("PATH", path)
        .args(["repos", "mirror", "sync", "-j", "1"])
        .assert()
        .code(1)
        .stderr(predicates::str::contains("some mirror operations failed"));
    let mirror = home.path().join("Documents/CodeMirror");
    assert!(mirror.join("one-main/.git").is_dir());
    assert!(mirror.join("two-main/.git").is_dir());
}

#[cfg(unix)]
#[test]
fn mirror_status_errors_take_precedence_over_missing_repositories() {
    let home = TempDir::new().unwrap();
    write_mirror_config(
        &home,
        &[
            ("https://example.com/bad.git", "main"),
            ("https://example.com/missing.git", "main"),
        ],
    );
    std::fs::create_dir_all(home.path().join("Documents/CodeMirror/bad-main/.git")).unwrap();
    let fake_bin = fake_git_dir(&home);
    std::fs::write(
        fake_bin.join("git"),
        "#!/bin/sh\necho 'fixture status failure' >&2\nexit 1\n",
    )
    .unwrap();
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    lum_with_xdg(&home)
        .env("PATH", path)
        .args(["repos", "mirror", "status", "--offline", "-j", "2"])
        .assert()
        .code(1)
        .stdout(predicates::str::contains("missing-main: missing"))
        .stderr(predicates::str::contains("error reading local HEAD"));
}

#[cfg(unix)]
#[test]
fn mirror_sync_runs_multiple_clone_jobs_concurrently() {
    let home = TempDir::new().unwrap();
    write_mirror_config(
        &home,
        &[
            ("https://github.com/example/one.git", "main"),
            ("https://github.com/example/two.git", "main"),
        ],
    );
    let fake_bin = fake_git_dir(&home);
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let started = std::time::Instant::now();
    lum_with_xdg(&home)
        .env("PATH", path)
        .args(["repos", "mirror", "sync", "-j", "2"])
        .assert()
        .success();

    assert!(
        started.elapsed() < std::time::Duration::from_millis(1800),
        "two one-second clone jobs should overlap when -j 2 is used"
    );
}

#[cfg(unix)]
#[test]
fn mirror_status_runs_multiple_checks_concurrently() {
    let home = TempDir::new().unwrap();
    write_mirror_config(
        &home,
        &[
            ("https://github.com/example/one.git", "main"),
            ("https://github.com/example/two.git", "main"),
        ],
    );
    let mirror = home.path().join("Documents").join("CodeMirror");
    std::fs::create_dir_all(mirror.join("one-main/.git")).unwrap();
    std::fs::create_dir_all(mirror.join("two-main/.git")).unwrap();
    let fake_bin = fake_git_dir(&home);
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let started = std::time::Instant::now();
    lum_with_xdg(&home)
        .env("PATH", path)
        .args(["repos", "mirror", "status", "--offline", "-j", "2"])
        .assert()
        .success();

    assert!(
        started.elapsed() < std::time::Duration::from_millis(2800),
        "two two-second status checks should overlap when -j 2 is used"
    );
}

// --- mirror watch tests ---

#[test]
fn mirror_watch_without_tag_with_config_points_to_list() {
    let home = TempDir::new().unwrap();
    let config_dir = home.path().join("config").join("lum");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("repos.json"), r#"{"repos": []}"#).unwrap();
    lum_with_xdg(&home)
        .args(["repos", "mirror", "watch"])
        .assert()
        .success()
        .stdout(predicates::str::contains("mirror list"));
}

#[test]
fn mirror_watch_without_tag_no_config_points_to_init() {
    let home = TempDir::new().unwrap();
    lum_with_xdg(&home)
        .args(["repos", "mirror", "watch"])
        .assert()
        .success()
        .stdout(predicates::str::contains("mirror init"));
}

#[test]
fn mirror_watch_with_tag_no_matching_repos() {
    let home = TempDir::new().unwrap();
    let config_dir = home.path().join("config").join("lum");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("repos.json"),
        r#"{"repos": [{"url": "https://github.com/org/repo.git", "branch": "main", "tags": ["other"]}]}"#,
    )
    .unwrap();

    lum_with_xdg(&home)
        .args(["repos", "mirror", "watch", "nonexistent"])
        .assert()
        .success()
        .stdout(predicates::str::contains("No repos found"));
}

#[test]
fn mirror_watch_detects_sha_change() {
    // Set up a local bare repo as the remote
    let remote_dir = TempDir::new().unwrap();
    let bare_path = remote_dir.path().join("remote.git");
    let clone_path = remote_dir.path().join("clone");

    // Create bare remote with one commit
    std::process::Command::new("git")
        .args(["init", "--bare"])
        .arg(&bare_path)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["clone", &bare_path.to_string_lossy()])
        .arg(&clone_path)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args([
            "-C",
            &clone_path.to_string_lossy(),
            "config",
            "user.email",
            "test@test.com",
        ])
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args([
            "-C",
            &clone_path.to_string_lossy(),
            "config",
            "user.name",
            "Test",
        ])
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args([
            "-C",
            &clone_path.to_string_lossy(),
            "checkout",
            "-b",
            "main",
        ])
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args([
            "-C",
            &clone_path.to_string_lossy(),
            "commit",
            "--allow-empty",
            "-m",
            "initial",
        ])
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args([
            "-C",
            &clone_path.to_string_lossy(),
            "push",
            "-u",
            "origin",
            "main",
        ])
        .output()
        .unwrap();

    let url = format!("file://{}", bare_path.to_string_lossy());

    // Set up lum config pointing to this remote
    let home = TempDir::new().unwrap();
    let config_dir = home.path().join("config").join("lum");
    std::fs::create_dir_all(&config_dir).unwrap();
    let config_json = format!(
        r#"{{"repos": [{{"url": "{}", "branch": "main", "tags": ["rebase"]}}]}}"#,
        url.replace('\"', "\\\"")
    );
    std::fs::write(config_dir.join("repos.json"), &config_json).unwrap();

    // First cycle: records baseline, no notification
    lum_with_xdg(&home)
        .args(["repos", "mirror", "watch", "rebase", "--cycles", "1"])
        .assert()
        .success()
        .stdout(
            predicates::str::contains("Watching repositories with the rebase tag")
                .and(predicates::str::contains("remote/main"))
                .and(predicates::str::contains("HEAD changed").not()),
        );

    // Push a new commit to move the remote HEAD
    std::process::Command::new("git")
        .args([
            "-C",
            &clone_path.to_string_lossy(),
            "commit",
            "--allow-empty",
            "-m",
            "second",
        ])
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["-C", &clone_path.to_string_lossy(), "push"])
        .output()
        .unwrap();

    // Second cycle: should detect the change and report it
    lum_with_xdg(&home)
        .args(["repos", "mirror", "watch", "rebase", "--cycles", "1"])
        .assert()
        .success()
        .stdout(predicates::str::contains("HEAD changed"));
}
