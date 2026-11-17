//! `lum display`: HiDPI on and off for external monitors. macOS only.
//! See `README.md`.

#[cfg(target_os = "macos")]
mod macos;
mod modes;

use anyhow::Result;

use crate::cli::HidpiState;

pub fn run(state: Option<HidpiState>) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::run(state)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = state;
        anyhow::bail!("lum display is only supported on macOS")
    }
}
