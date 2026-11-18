//! `lum display`: HiDPI on and off, and sRGB color, for monitors that aren't
//! Retina, and the status tray line that shows HiDPI while it is off. macOS only. See
//! `README.md`.

#[cfg(target_os = "macos")]
mod macos;
mod modes;
#[cfg(target_os = "macos")]
mod srgb;

use anyhow::Result;

use crate::cli::DisplayCommand;

pub fn run(command: DisplayCommand) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use macos::HidpiState;
        match command {
            DisplayCommand::Status => macos::run(None),
            DisplayCommand::On => macos::run(Some(HidpiState::On)),
            DisplayCommand::Off => macos::run(Some(HidpiState::Off)),
            DisplayCommand::Srgb => srgb::run(),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = command;
        unsupported()
    }
}

/// A line for each monitor with HiDPI off, for the status tray: none when it is on
/// everywhere, and `None` when no monitor can be read. Only macOS has any.
pub(crate) fn off_lines() -> Option<Vec<String>> {
    #[cfg(target_os = "macos")]
    {
        macos::off_lines()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some(Vec::new())
    }
}

#[cfg(not(target_os = "macos"))]
fn unsupported() -> Result<()> {
    anyhow::bail!("lum display is only supported on macOS")
}
