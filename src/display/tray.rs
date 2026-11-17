//! The display tray: while `lum display off` is in force, a tray icon with a line
//! for each monitor that has HiDPI off (ADR-0022). `off` starts it in a process
//! of its own, and it leaves once HiDPI is back on everywhere. While a station
//! is loaded, the station's tray shows the lines instead and this one hides.

use std::fs::{File, OpenOptions, TryLockError};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::paths;
use crate::tray::{self, Shown};

/// How often the trays read the monitors' modes. Their watch runs every 2 seconds, so a
/// station starting or stopping shows quickly, but reading every mode of every monitor
/// that often is wasted work for a state that changes a few times a day.
const MODES_EVERY: Duration = Duration::from_secs(10);

/// The display lines, read at most every [`MODES_EVERY`].
#[derive(Default)]
pub(super) struct OffLines {
    read: Option<(Instant, Option<Vec<String>>)>,
}

impl OffLines {
    /// See [`super::macos::off_lines`].
    pub(super) fn get(&mut self) -> Option<Vec<String>> {
        if self
            .read
            .as_ref()
            .is_none_or(|(at, _)| at.elapsed() >= MODES_EVERY)
        {
            self.read = Some((Instant::now(), super::macos::off_lines()));
        }
        self.read.as_ref().and_then(|(_, lines)| lines.clone())
    }

    /// Reads afresh next time.
    pub(super) fn forget(&mut self) {
        self.read = None;
    }
}

/// The display tray holds this file locked while it runs.
fn lock_file() -> Result<File> {
    let path = paths::state_dir("display-tray.lock")?;
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

/// Whether the display tray is running.
pub(super) fn up() -> bool {
    lock_file().is_ok_and(|file| matches!(file.try_lock_shared(), Err(TryLockError::WouldBlock)))
}

/// Starts the display tray unless it is running already.
pub(super) fn show() {
    if up() {
        return;
    }
    let started = std::env::current_exe().and_then(|lum| {
        Command::new(lum)
            .arg("__display_tray")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    });
    if let Err(error) = started {
        tracing::warn!(error = %error, "could not start the display tray");
    }
}

/// Runs the display tray until HiDPI is back on everywhere.
pub(super) fn run() -> Result<()> {
    let lock = lock_file()?;
    match lock.try_lock() {
        Ok(()) => {}
        // Another display tray is running.
        Err(TryLockError::WouldBlock) => return Ok(()),
        Err(TryLockError::Error(error)) => {
            return Err(error).context("could not lock the display tray");
        }
    }
    let mut lines = OffLines::default();
    let mut back_on = false;
    let hosted = tray::host_display(|| {
        let lines = lines.get();
        if lines.as_ref().is_some_and(Vec::is_empty) {
            back_on = true;
            return None;
        }
        let display = lines.unwrap_or_default();
        Some(Shown {
            visible: !display.is_empty() && !crate::radio::station_loaded(),
            display,
        })
    });
    drop(lock);
    // A `lum display off` that ran while this tray was leaving saw it still up and
    // started none, but it had switched its monitors first, so they read as off here.
    if back_on && super::macos::off_lines().is_some_and(|lines| !lines.is_empty()) {
        show();
    }
    hosted
}
