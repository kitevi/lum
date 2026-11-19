//! A command a target runs before it is archived, when the machine has it.

use anyhow::{Context, Result, bail};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where `program` is on PATH, if the environment has it at all. Being there
/// is the whole opt-in; a machine without it backs up exactly as before.
pub(crate) fn find(program: &str) -> Option<PathBuf> {
    which::which(program).ok()
}

/// Run the command at `path`, holding its output back until it fails: a step
/// that worked needs no reprint, and one that broke must say why.
pub(crate) fn run(path: &Path) -> Result<()> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("pre-backup command");
    let output = Command::new(path)
        .output()
        .with_context(|| format!("failed to run {name} at {}", path.display()))?;
    if output.status.success() {
        return Ok(());
    }

    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(&output.stdout);
    let _ = stdout.flush();
    let mut stderr = std::io::stderr();
    let _ = stderr.write_all(&output.stderr);
    let _ = stderr.flush();
    bail!(
        "{name} failed ({}), so nothing was uploaded; fix it, or take it off PATH to back up without it",
        output.status
    )
}

#[cfg(test)]
mod tests {
    use super::{find, run};

    #[test]
    fn find_reports_nothing_when_the_environment_lacks_the_command() {
        assert!(find("lum-pre-command-that-does-not-exist").is_none());
    }

    #[cfg(unix)]
    fn script(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn run_accepts_a_command_that_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(dir.path(), "brdump", "echo 'Brewfile created'\n");
        run(&path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn run_fails_with_the_command_name_when_it_exits_nonzero() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(
            dir.path(),
            "brdump",
            "echo 'Error: Homebrew is not installed' >&2\nexit 3\n",
        );
        let error = run(&path).unwrap_err().to_string();
        assert!(error.contains("brdump failed"), "{error}");
        assert!(error.contains("back up without it"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn run_reports_a_command_that_cannot_be_started() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("brdump");
        std::fs::write(&path, "not a script").unwrap();
        let error = run(&path).unwrap_err().to_string();
        assert!(error.contains("failed to run brdump"), "{error}");
    }
}
