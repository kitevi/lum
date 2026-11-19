use std::path::PathBuf;

use anyhow::{Context, Result};

use super::catalog::FontSpec;

/// Returns the user-level font directory for the current OS.
///
/// - macOS: ~/Library/Fonts
/// - Linux: ~/.local/share/fonts
/// - Windows: %LOCALAPPDATA%\Microsoft\Windows\Fonts
pub(crate) fn user_font_dir() -> Result<PathBuf> {
    match std::env::consts::OS {
        "macos" => {
            let home = crate::paths::home_dir()?;
            Ok(home.join("Library").join("Fonts"))
        }
        "linux" => {
            // Respect XDG_DATA_HOME if set (useful for testing too)
            if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
                Ok(PathBuf::from(xdg).join("fonts"))
            } else {
                let home = crate::paths::home_dir()?;
                Ok(home.join(".local").join("share").join("fonts"))
            }
        }
        "windows" => {
            let local_app_data = std::env::var("LOCALAPPDATA")
                .context("LOCALAPPDATA environment variable not set")?;
            Ok(PathBuf::from(local_app_data)
                .join("Microsoft")
                .join("Windows")
                .join("Fonts"))
        }
        _ => anyhow::bail!(
            "unsupported operating system for font installation: {}",
            std::env::consts::OS
        ),
    }
}

/// Returns the installation directory for a specific font.
/// This is a subdirectory under the user font dir named after the font.
pub(crate) fn font_install_dir(spec: &FontSpec) -> Result<PathBuf> {
    Ok(user_font_dir()?.join(spec.dir_name))
}

/// Best-effort font cache refresh.
/// - Linux/macOS: runs `fc-cache` if available
/// - Windows: no action needed (automatic)
pub(crate) fn refresh_font_cache() {
    if cfg!(unix) {
        let _ = std::process::Command::new("fc-cache")
            .args(["-f", "-v"])
            .status();
    }
}
