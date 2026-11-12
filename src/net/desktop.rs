//! The GNOME desktop around a game: extensions in its way go off while it
//! runs and come back on afterwards, through the `gnome-extensions` CLI.

use std::process::Command;

use anyhow::{Context, Result, ensure};

/// Extensions switched off while a game runs, with their names.
const GAME_EXTENSIONS: &[(&str, &str)] = &[("dash-to-panel@jderose9.github.com", "Dash to Panel")];

/// Fails unless this is a GNOME session with the `gnome-extensions` CLI.
pub(super) fn ensure_gnome() -> Result<()> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    ensure!(
        is_gnome(&desktop),
        "this is not a GNOME session (XDG_CURRENT_DESKTOP is '{desktop}')"
    );
    which::which("gnome-extensions")
        .context("gnome-extensions not found; it comes with GNOME Shell")?;
    Ok(())
}

/// `XDG_CURRENT_DESKTOP` is a colon-separated list, such as `ubuntu:GNOME`.
fn is_gnome(desktop: &str) -> bool {
    desktop
        .split(':')
        .any(|name| name.eq_ignore_ascii_case("GNOME"))
}

/// The game extensions that are on right now.
pub(super) fn enabled_game_extensions() -> Result<Vec<String>> {
    let enabled = run(&["list", "--enabled"])?;
    Ok(GAME_EXTENSIONS
        .iter()
        .filter(|(uuid, _)| super::has_line(&enabled, uuid))
        .map(|(uuid, _)| (*uuid).to_owned())
        .collect())
}

pub(super) fn disable(uuid: &str) -> Result<()> {
    run(&["disable", uuid])?;
    Ok(())
}

pub(super) fn enable(uuid: &str) -> Result<()> {
    run(&["enable", uuid])
        .with_context(|| format!("switch it back on with: gnome-extensions enable {uuid}"))?;
    Ok(())
}

/// The extension's name, or its UUID for one lum does not know.
pub(super) fn name(uuid: &str) -> &str {
    GAME_EXTENSIONS
        .iter()
        .find(|(known, _)| *known == uuid)
        .map_or(uuid, |(_, name)| name)
}

fn run(args: &[&str]) -> Result<String> {
    let output = Command::new("gnome-extensions")
        .args(args)
        .output()
        .context("failed to run gnome-extensions")?;
    ensure!(
        output.status.success(),
        "gnome-extensions {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_gnome_sessions() {
        assert!(is_gnome("GNOME"));
        assert!(is_gnome("ubuntu:GNOME"));
        assert!(is_gnome("gnome"));
        assert!(!is_gnome("KDE"));
        assert!(!is_gnome("GNOME-Flashback-ish"));
        assert!(!is_gnome(""));
    }

    #[test]
    fn names_known_extensions() {
        assert_eq!(name("dash-to-panel@jderose9.github.com"), "Dash to Panel");
        assert_eq!(name("other@example.com"), "other@example.com");
    }
}
