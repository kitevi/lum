//! Running a command that lum has to clean up after when it exits: the game
//! that `lum net game` wraps.

use std::{
    process::{Command, ExitStatus},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use shared_child::SharedChild;

/// How long a command gets to exit after SIGTERM before it is killed.
const GRACE: Duration = Duration::from_secs(10);

/// Catches Ctrl+C, SIGTERM and SIGHUP instead of letting them kill lum, so
/// the cleanup after a wrapped command still runs.
#[derive(Default)]
pub(super) struct Trap(Arc<AtomicBool>);

impl Trap {
    /// A trap that never fires: signals keep their usual effect.
    pub(super) fn none() -> Self {
        Self::default()
    }

    /// From now on, the three signals only make `fired` return true.
    #[cfg(unix)]
    pub(super) fn install() -> Result<Self> {
        use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};

        let trap = Self::default();
        for signal in [SIGINT, SIGTERM, SIGHUP] {
            signal_hook::flag::register(signal, Arc::clone(&trap.0))
                .context("failed to install a signal handler")?;
        }
        Ok(trap)
    }

    #[cfg(not(unix))]
    pub(super) fn install() -> Result<Self> {
        Ok(Self::default())
    }

    pub(super) fn fired(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Runs `command` until it exits and returns its status, or `None` if the
/// trap fired before it started. A signal the trap catches is passed on as
/// SIGTERM, so the command ends and the cleanup can run. (Ctrl+C in the
/// terminal already reaches the command directly.) A command that is still
/// running `GRACE` later is killed, or the cleanup would wait on it for ever.
pub(super) fn run(command: &[String], trap: &Trap) -> Result<Option<ExitStatus>> {
    run_with_grace(command, trap, GRACE)
}

fn run_with_grace(command: &[String], trap: &Trap, grace: Duration) -> Result<Option<ExitStatus>> {
    if trap.fired() {
        return Ok(None);
    }
    let (program, args) = command.split_first().context("no command to run")?;
    let child = SharedChild::spawn(Command::new(program).args(args))
        .with_context(|| format!("failed to start {program}"))?;
    // When SIGTERM was passed on, and whether the kill has followed it.
    let mut terminated = None;
    let mut killed = false;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        match terminated {
            None if trap.fired() => {
                terminate(&child);
                terminated = Some(Instant::now());
            }
            Some(since) if !killed && since.elapsed() >= grace => {
                say_err!("{program} is still running; killing it.");
                let _ = child.kill();
                killed = true;
            }
            _ => {}
        }
        thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(unix)]
fn terminate(child: &SharedChild) {
    use shared_child::unix::SharedChildExt;

    let _ = child.send_signal(signal_hook::consts::SIGTERM);
}

#[cfg(not(unix))]
fn terminate(child: &SharedChild) {
    let _ = child.kill();
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::process::ExitStatusExt;

    use super::*;

    fn command(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn returns_the_exit_status() {
        let trap = Trap::none();
        assert!(run(&command(&["true"]), &trap).unwrap().unwrap().success());
        assert!(!run(&command(&["false"]), &trap).unwrap().unwrap().success());
    }

    #[test]
    fn does_not_start_once_the_trap_has_fired() {
        let trap = Trap::none();
        trap.0.store(true, Ordering::SeqCst);
        assert_eq!(run(&command(&["true"]), &trap).unwrap(), None);
    }

    #[test]
    fn a_fired_trap_ends_the_command() {
        let trap = Trap::none();
        let flag = Arc::clone(&trap.0);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            flag.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();
        let status = run(&command(&["sleep", "30"]), &trap).unwrap().unwrap();
        assert!(!status.success());
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_command_that_ignores_sigterm_is_killed() {
        let dir = tempfile::tempdir().unwrap();
        let ready = dir.path().join("ready");
        // `$0` is the file that says SIGTERM is being ignored from here on.
        let script = r#"trap "" TERM; touch "$0"; exec sleep 30"#;
        let command = command(&["sh", "-c", script, ready.to_str().unwrap()]);
        let trap = Trap::none();
        let flag = Arc::clone(&trap.0);
        thread::spawn(move || {
            while !ready.exists() {
                thread::sleep(Duration::from_millis(10));
            }
            flag.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();
        let status = run_with_grace(&command, &trap, Duration::from_millis(300))
            .unwrap()
            .unwrap();
        assert_eq!(status.signal(), Some(signal_hook::consts::SIGKILL));
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_missing_program_is_an_error() {
        let error = run(&command(&["lum-no-such-program"]), &Trap::none()).unwrap_err();
        assert!(format!("{error:#}").contains("failed to start lum-no-such-program"));
    }
}
