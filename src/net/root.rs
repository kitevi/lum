//! The few commands game mode runs as root, through sudo.

use std::{
    io::Write as _,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use anyhow::{Context, Result, ensure};

/// Asks for the sudo password up front, once, instead of in the middle of a
/// switch. Later commands reuse sudo's cached credentials.
pub(super) fn authenticate() -> Result<()> {
    which::which("sudo").context("sudo not found; lum net needs it to change network settings")?;
    let status = Command::new("sudo")
        .arg("-v")
        .status()
        .context("failed to run sudo")?;
    ensure!(status.success(), "sudo authentication failed");
    Ok(())
}

/// Runs `args` as root and returns its stdout and stderr together.
pub(super) fn run(args: &[&str]) -> Result<String> {
    run_with_input(args, None)
}

/// Runs `args` as root with `input` on stdin.
pub(super) fn run_with_input(args: &[&str], input: Option<&str>) -> Result<String> {
    tracing::info!(command = ?args, "net: running as root");
    let mut child = Command::new("sudo")
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to run sudo {}", args.join(" ")))?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin.write_all(input.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(
        output.status.success(),
        "'{}' failed: {}",
        args.join(" "),
        text.trim()
    );
    Ok(text)
}

/// For cleanup steps whose target may already be gone: failures are logged.
pub(super) fn run_quiet(args: &[&str]) {
    if let Err(error) = run(args) {
        tracing::info!(error = %error, "net: cleanup step had nothing to do");
    }
}

/// Keeps sudo's cached credentials fresh while a game runs, so switching back
/// hours later does not wait on a password prompt. Stops when the returned
/// sender is dropped.
pub(super) fn keep_alive() -> mpsc::Sender<()> {
    let (stop, stopped) = mpsc::channel::<()>();
    thread::spawn(move || {
        while let Err(mpsc::RecvTimeoutError::Timeout) =
            stopped.recv_timeout(Duration::from_secs(60))
        {
            let _ = Command::new("sudo")
                .args(["-n", "-v"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    });
    stop
}
