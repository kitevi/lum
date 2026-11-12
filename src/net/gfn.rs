//! `lum net game gfn`: GeForce NOW from Flatpak in game mode, with Dash to
//! Panel off until GeForce NOW is gone. Linux with GNOME only.

use std::{
    process::{Command, ExitStatus, Stdio},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, ensure};

use super::{
    GameRecord, Platform, desktop, root,
    wrap::{self, Trap},
};

/// The word after `lum net game` that picks this launcher instead of a command.
pub(super) const NAME: &str = "gfn";
const APP: &str = "com.nvidia.geforcenow";

pub(super) fn play(platform: &dyn Platform, requested: Option<&str>) -> Result<()> {
    ensure!(
        cfg!(target_os = "linux"),
        "'lum net game gfn' is only supported on Linux"
    );
    desktop::ensure_gnome()?;
    which::which("flatpak").context("flatpak not found; GeForce NOW runs from Flatpak")?;
    ensure!(
        installed()?,
        "GeForce NOW is not installed from Flatpak ({APP})"
    );
    ensure!(!running()?, "GeForce NOW is already running; quit it first");

    let trap = Trap::install()?;
    let started = super::begin(platform, requested)?;
    let mut record = match &started {
        Some(record) => record.clone(),
        // Game mode was on already: the desktop changes join its record.
        None => super::load_record()?.context("the game mode record disappeared")?,
    };
    let played = hide_and_play(&mut record, &trap);

    // Put everything back even when hiding or playing failed.
    let restored = if started.is_some() {
        say!("Switching game mode off.");
        super::switch_off(platform, &record)
            .context("game mode is still on; run 'lum net normal' to switch back")
    } else {
        let shown = super::show_extensions(&mut record);
        super::save_record(&record)?;
        shown
    };
    if let Err(error) = restored {
        if let Err(played) = &played {
            say_err!("{played:#}");
        }
        return Err(error);
    }
    if let Some(status) = played?
        && !status.success()
        && !trap.fired()
    {
        say_err!("note: GeForce NOW exited with {status}");
    }
    Ok(())
}

/// Switches the game extensions off, recording each one first: GNOME saves
/// the change, so a lum that gets killed must still be able to undo it. Then
/// runs GeForce NOW until Flatpak lists no instance of it.
fn hide_and_play(record: &mut GameRecord, trap: &Trap) -> Result<Option<ExitStatus>> {
    for uuid in desktop::enabled_game_extensions()? {
        if !record.hidden_extensions.contains(&uuid) {
            record.hidden_extensions.push(uuid.clone());
            super::save_record(record)?;
        }
        desktop::disable(&uuid)?;
        say!("{} is off.", desktop::name(&uuid));
    }

    say!("Starting GeForce NOW.");
    let _sudo = root::keep_alive();
    let status = wrap::run(&["flatpak", "run", APP].map(str::to_owned), trap)?;
    // `flatpak run` can return while GeForce NOW still runs, for example if
    // it detaches, so the game is over only when its last instance is.
    while !trap.fired() && running().unwrap_or(false) {
        thread::sleep(Duration::from_secs(1));
    }
    Ok(status)
}

fn installed() -> Result<bool> {
    let status = Command::new("flatpak")
        .args(["info", APP])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to run flatpak")?;
    Ok(status.success())
}

fn running() -> Result<bool> {
    let output = Command::new("flatpak")
        .args(["ps", "--columns=application"])
        .output()
        .context("failed to run flatpak")?;
    ensure!(
        output.status.success(),
        "flatpak ps failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(super::has_line(
        &String::from_utf8_lossy(&output.stdout),
        APP,
    ))
}
