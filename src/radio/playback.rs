//! Pause/resume handle shared by a station runner's supervisor and its tray.
//!
//! The tray lives on the runner's UI thread while the supervisor blocks in
//! `ffplay`'s `wait()` on a worker thread, so neither can simply call into the
//! other. They share this handle instead: the tray flips the pause state (killing
//! the running ffplay), and the supervisor honours it between attempts. It is all
//! in-process state, so it adds no IPC (ADR-0002) and no extra process.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use shared_child::SharedChild;

type ChangeObserver = Box<dyn Fn(bool) + Send + Sync>;

#[derive(Default)]
pub(super) struct Playback {
    inner: Mutex<Inner>,
    changed: Condvar,
    on_change: Option<ChangeObserver>,
}

#[derive(Default)]
struct Inner {
    paused: bool,
    /// The ffplay being supervised, so a pause can kill it from the UI thread.
    child: Option<Arc<SharedChild>>,
    /// A pause killed `child`, so its exit is not a stream failure.
    interrupted: bool,
}

impl Inner {
    fn interrupt_child(&mut self) {
        if let Some(child) = &self.child {
            self.interrupted = true;
            // Killing a child that was already reaped is a no-op, so a stale handle is harmless.
            let _ = child.kill();
        }
    }
}

impl Playback {
    /// `on_change` runs after every pause-state change, on the thread that made it.
    pub(super) fn with_observer(on_change: impl Fn(bool) + Send + Sync + 'static) -> Self {
        Self {
            on_change: Some(Box::new(on_change)),
            ..Self::default()
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // No code path leaves `Inner` half-updated, so a poisoned lock is still consistent.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn is_paused(&self) -> bool {
        self.lock().paused
    }

    /// Pauses a playing station (killing its ffplay) or resumes a paused one.
    /// Returns the new paused state.
    pub(super) fn toggle(&self) -> bool {
        let paused = {
            let mut inner = self.lock();
            inner.paused = !inner.paused;
            if inner.paused {
                inner.interrupt_child();
            }
            inner.paused
        };
        self.changed.notify_all();
        if let Some(observer) = &self.on_change {
            observer(paused);
        }
        paused
    }

    /// Blocks while paused. Returns whether it had to wait, so the supervisor can
    /// start the next session fresh instead of inheriting an old failure streak.
    pub(super) fn wait_until_playing(&self) -> bool {
        let inner = self.lock();
        if !inner.paused {
            return false;
        }
        drop(
            self.changed
                .wait_while(inner, |inner| inner.paused)
                .unwrap_or_else(PoisonError::into_inner),
        );
        true
    }

    /// Sleeps for `duration`, waking early if playback gets paused.
    pub(super) fn sleep(&self, duration: Duration) {
        let inner = self.lock();
        drop(
            self.changed
                .wait_timeout_while(inner, duration, |inner| !inner.paused)
                .unwrap_or_else(PoisonError::into_inner),
        );
    }

    /// Registers the ffplay that was just spawned so a pause can kill it. Returns
    /// false when playback is already paused: the caller must then discard the
    /// child instead of supervising it.
    pub(super) fn attach(&self, child: Arc<SharedChild>) -> bool {
        let mut inner = self.lock();
        if inner.paused {
            return false;
        }
        inner.interrupted = false;
        inner.child = Some(child);
        true
    }

    /// Unregisters the child after it exited. Returns whether a pause killed it.
    pub(super) fn detach(&self) -> bool {
        let mut inner = self.lock();
        inner.child = None;
        std::mem::take(&mut inner.interrupted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::thread;

    /// Generous: these only bound how long a broken implementation can hang a test.
    const PATIENCE: Duration = Duration::from_secs(10);
    /// Long enough that "still blocked" is a meaningful observation.
    const SETTLE: Duration = Duration::from_millis(150);

    fn long_running_child() -> Arc<SharedChild> {
        let mut command = if cfg!(windows) {
            let mut command = Command::new("ping");
            command.args(["-n", "60", "127.0.0.1"]);
            command
        } else {
            let mut command = Command::new("sleep");
            command.arg("60");
            command
        };
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        Arc::new(SharedChild::spawn(&mut command).expect("spawn a long-running child"))
    }

    #[test]
    fn toggle_flips_and_reports_the_new_state() {
        let playback = Playback::default();

        assert!(!playback.is_paused());
        assert!(playback.toggle());
        assert!(playback.is_paused());
        assert!(!playback.toggle());
        assert!(!playback.is_paused());
    }

    #[test]
    fn observer_sees_every_change_in_order() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let playback = Playback::with_observer({
            let seen = Arc::clone(&seen);
            move |paused| seen.lock().unwrap().push(paused)
        });

        playback.toggle();
        playback.toggle();
        playback.toggle();

        assert_eq!(*seen.lock().unwrap(), [true, false, true]);
    }

    #[test]
    fn wait_until_playing_does_not_wait_while_playing() {
        assert!(!Playback::default().wait_until_playing());
    }

    #[test]
    fn wait_until_playing_blocks_until_resumed() {
        let playback = Arc::new(Playback::default());
        playback.toggle();
        let (done, finished) = mpsc::channel();
        {
            let playback = Arc::clone(&playback);
            thread::spawn(move || {
                let _ = done.send(playback.wait_until_playing());
            });
        }

        assert!(
            finished.recv_timeout(SETTLE).is_err(),
            "must keep waiting while paused"
        );
        playback.toggle();
        assert_eq!(finished.recv_timeout(PATIENCE), Ok(true));
    }

    #[test]
    fn sleep_wakes_early_when_paused() {
        let playback = Arc::new(Playback::default());
        let (done, finished) = mpsc::channel();
        {
            let playback = Arc::clone(&playback);
            thread::spawn(move || {
                playback.sleep(Duration::from_secs(300));
                let _ = done.send(());
            });
        }

        assert!(finished.recv_timeout(SETTLE).is_err(), "still sleeping");
        playback.toggle();
        assert!(
            finished.recv_timeout(PATIENCE).is_ok(),
            "a pause must cut the sleep short"
        );
    }

    #[test]
    fn sleep_returns_at_once_when_already_paused() {
        let playback = Playback::default();
        playback.toggle();

        // Would hang for five minutes if a paused handle still slept.
        playback.sleep(Duration::from_secs(300));
    }

    #[test]
    fn pausing_kills_the_attached_child_and_marks_the_exit_as_interrupted() {
        let playback = Playback::default();
        let child = long_running_child();
        assert!(playback.attach(Arc::clone(&child)));

        assert!(playback.toggle());

        let status = child.wait().expect("child exits once killed");
        assert!(!status.success());
        assert!(playback.detach(), "the pause caused this exit");
        assert!(!playback.detach(), "the mark is consumed by the first read");
    }

    #[test]
    fn an_exit_without_a_pause_is_not_marked_interrupted() {
        let playback = Playback::default();
        let child = long_running_child();
        assert!(playback.attach(Arc::clone(&child)));

        child.kill().expect("kill the child directly");
        child.wait().expect("reap it");

        assert!(!playback.detach(), "a plain failure must stay a failure");
    }

    #[test]
    fn a_pause_with_nothing_attached_does_not_taint_the_next_session() {
        let playback = Playback::default();
        playback.toggle();
        playback.toggle();

        let child = long_running_child();
        assert!(playback.attach(Arc::clone(&child)));
        child.kill().expect("kill the child directly");
        child.wait().expect("reap it");

        assert!(!playback.detach());
    }

    #[test]
    fn attach_is_refused_while_paused() {
        let playback = Playback::default();
        playback.toggle();
        let child = long_running_child();

        assert!(!playback.attach(Arc::clone(&child)));

        // The caller owns the refused child and is expected to discard it.
        child.kill().expect("kill the refused child");
        child.wait().expect("reap it");
    }
}
