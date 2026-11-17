//! `lum display`: HiDPI on and off, and sRGB color, for monitors that aren't
//! Retina, and the tray that shows HiDPI while it is off. macOS only. See
//! `README.md`.

#[cfg(target_os = "macos")]
mod macos;
mod modes;
#[cfg(target_os = "macos")]
mod srgb;
#[cfg(target_os = "macos")]
mod tray;

use anyhow::Result;

use crate::cli::DisplayCommand;

pub fn run(command: Option<DisplayCommand>) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use macos::HidpiState;
        match command {
            None => macos::run(None),
            Some(DisplayCommand::On) => macos::run(Some(HidpiState::On)),
            Some(DisplayCommand::Off) => macos::run(Some(HidpiState::Off)),
            Some(DisplayCommand::Srgb) => srgb::run(),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = command;
        unsupported()
    }
}

/// Hosts the display tray (`__display_tray`), which `lum display off` starts.
pub fn run_tray() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        tray::run()
    }
    #[cfg(not(target_os = "macos"))]
    {
        unsupported()
    }
}

/// The display tray's lines, for a station's tray to show while the display
/// tray hides: none unless `lum display off` started it. The modes are read at
/// most every 10 seconds, however often this is called.
pub(crate) fn tray_lines() -> impl FnMut() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        let mut lines = tray::OffLines::default();
        move || {
            if tray::up() {
                lines.get().unwrap_or_default()
            } else {
                // Read afresh once a display tray starts.
                lines.forget();
                Vec::new()
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new
    }
}

#[cfg(not(target_os = "macos"))]
fn unsupported() -> Result<()> {
    anyhow::bail!("lum display is only supported on macOS")
}
