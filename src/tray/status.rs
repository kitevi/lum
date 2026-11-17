//! The status tray: while `lum display off` is in force (macOS) or `lum net game` is on,
//! a tray icon with a line for each (ADR-0022, ADR-0033). The command that switches starts
//! it in a process of its own, and it leaves once neither is in force. While a station is
//! loaded, the station's tray shows the lines instead and this one hides.

use std::fs::{File, OpenOptions, TryLockError};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use super::{Shown, Status};
use crate::paths;

/// How often the trays read what the lines say: the modes of every monitor, and whether game
/// mode is still on, which on macOS asks `ifconfig`. Their watch runs every 2 seconds, so a
/// station starting or stopping shows quickly, but reading that often is wasted work for
/// state that changes a few times a day.
const READ_EVERY: Duration = Duration::from_secs(10);

/// A value read at most every [`READ_EVERY`].
#[derive(Default)]
struct Cached<T> {
    read: Option<(Instant, T)>,
}

impl<T: Clone> Cached<T> {
    fn get(&mut self, read: impl FnOnce() -> T) -> T {
        match &self.read {
            Some((at, value)) if at.elapsed() < READ_EVERY => value.clone(),
            _ => {
                let value = read();
                self.read = Some((Instant::now(), value.clone()));
                value
            }
        }
    }

    /// Reads afresh next time.
    fn forget(&mut self) {
        self.read = None;
    }
}

/// What the status lines are made of.
#[derive(Default)]
struct Sources {
    display: Cached<Option<Vec<String>>>,
    game: Cached<Option<String>>,
}

impl Sources {
    /// The display lines, with None when no monitor can be read, and the game mode line, with
    /// None when game mode is off.
    fn read(&mut self) -> (Option<Vec<String>>, Option<String>) {
        let display = self.display.get(crate::display::off_lines);
        // Looking for the record is cheap; what the system says about it is not, on macOS.
        let game = if crate::net::game_recorded() {
            self.game.get(crate::net::tray_line)
        } else {
            self.game.forget();
            None
        };
        (display, game)
    }

    fn forget(&mut self) {
        self.display.forget();
        self.game.forget();
    }
}

/// The status tray holds this file locked while it runs.
fn lock_file() -> Result<File> {
    let path = paths::state_dir("status-tray.lock")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("could not create {}", dir.display()))?;
    }
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .with_context(|| format!("could not open {}", path.display()))
}

/// Whether the status tray is running.
pub(crate) fn up() -> bool {
    lock_file().is_ok_and(|file| matches!(file.try_lock_shared(), Err(TryLockError::WouldBlock)))
}

/// Starts the status tray unless it is running already. `$LUM_NO_STATUS_TRAY`, a test hook,
/// keeps tests of the commands that call this from putting an icon on the desk.
pub(crate) fn show() {
    if std::env::var_os("LUM_NO_STATUS_TRAY").is_some() || up() {
        return;
    }
    let started = std::env::current_exe().and_then(|lum| {
        let mut tray = Command::new(lum);
        tray.arg("__status_tray")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // A process group of its own, so that Ctrl+C in the terminal that ran `lum net game`
        // does not take the icon down while game mode is still on.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut tray, 0);
        tray.spawn()
    });
    if let Err(error) = started {
        tracing::warn!(error = %error, "could not start the status tray");
    }
}

/// The status lines for a station's tray: the status tray's while it is up, which is while
/// something is in force, and none otherwise.
pub(crate) fn watch() -> impl FnMut() -> Status {
    let mut sources = Sources::default();
    move || {
        if !up() {
            // Read afresh once a status tray starts.
            sources.forget();
            return Status::default();
        }
        let (display, game) = sources.read();
        Status {
            display: display.unwrap_or_default(),
            game,
        }
    }
}

/// Runs the status tray until HiDPI is back on everywhere and game mode is off.
pub(crate) fn run() -> Result<()> {
    let lock = lock_file()?;
    match lock.try_lock() {
        Ok(()) => {}
        // Another status tray is running.
        Err(TryLockError::WouldBlock) => return Ok(()),
        Err(TryLockError::Error(error)) => {
            return Err(error).context("could not lock the status tray");
        }
    }
    let mut sources = Sources::default();
    let mut idle = false;
    let hosted = super::host_status(|| {
        let (display, game) = sources.read();
        if game.is_none() && display.as_ref().is_some_and(Vec::is_empty) {
            idle = true;
            return None;
        }
        let status = Status {
            display: display.unwrap_or_default(),
            game,
        };
        Some(Shown {
            visible: !status.is_empty() && !crate::radio::station_loaded(),
            status,
        })
    });
    drop(lock);
    // A `lum display off` or `lum net game` that ran while this tray was leaving saw it still
    // up and started none, but it had switched first, so it reads as in force here.
    if idle {
        sources.forget();
        let (display, game) = sources.read();
        if game.is_some() || display.is_some_and(|lines| !lines.is_empty()) {
            show();
        }
    }
    hosted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cached_value_is_read_once_until_it_is_forgotten() {
        let mut cached = Cached::default();
        let mut reads = 0;
        for _ in 0..3 {
            cached.get(|| {
                reads += 1;
                reads
            });
        }
        assert_eq!(reads, 1);
        cached.forget();
        let again = cached.get(|| {
            reads += 1;
            reads
        });
        assert_eq!(again, 2);
    }
}
