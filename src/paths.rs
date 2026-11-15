use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::{BaseDirs, ProjectDirs, UserDirs};

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("dev", "ppowo", "lum").context("failed to determine platform directories")
}

fn xdg_config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
}

fn xdg_data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from)
}

fn xdg_state_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME").map(PathBuf::from)
}

pub(crate) fn config_file(filename: &str) -> Result<PathBuf> {
    let dir = match xdg_config_dir() {
        Some(dir) => dir.join("lum"),
        None => project_dirs()?.config_dir().to_path_buf(),
    };
    Ok(dir.join(filename))
}

pub(crate) fn data_dir(name: &str) -> Result<PathBuf> {
    let dir = match xdg_data_dir() {
        Some(dir) => dir.join("lum"),
        None => project_dirs()?.data_dir().to_path_buf(),
    };
    Ok(dir.join(name))
}

pub(crate) fn state_dir(name: &str) -> Result<PathBuf> {
    let dir = match xdg_state_dir() {
        Some(dir) => dir.join("lum"),
        None => {
            let dirs = project_dirs()?;
            dirs.state_dir()
                .unwrap_or_else(|| dirs.data_dir())
                .to_path_buf()
        }
    };
    Ok(dir.join(name))
}

pub(crate) fn bin_dir() -> Result<PathBuf> {
    data_dir("bin")
}

pub(crate) fn env_state_file() -> Result<PathBuf> {
    config_file("env-state.json")
}

pub(crate) fn tools_state_file() -> Result<PathBuf> {
    config_file("tools-state.json")
}

pub(crate) fn apps_state_file() -> Result<PathBuf> {
    config_file("apps-state.json")
}

/// Install root for "lum apps": $LUM_APPS_DIR, else the per-user apps folder
/// of this OS (created on demand by installs).
pub(crate) fn apps_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("LUM_APPS_DIR")
        && !dir.is_empty()
    {
        return Ok(PathBuf::from(dir));
    }
    match std::env::consts::OS {
        "macos" => Ok(home_dir()?.join("Applications").join("lum")),
        "windows" => Ok(base_dirs()?.data_local_dir().join("Programs").join("lum")),
        _ => data_dir("apps"),
    }
}

/// Directories scanned for foreign (unmanaged) copies of a managed macOS bundle.
/// Overridable for tests via $LUM_APPS_FOREIGN_SCAN_DIRS (path list).
pub(crate) fn foreign_scan_dirs() -> Vec<PathBuf> {
    if let Some(dirs) = std::env::var_os("LUM_APPS_FOREIGN_SCAN_DIRS") {
        return std::env::split_paths(&dirs).collect();
    }
    let mut dirs = vec![PathBuf::from("/Applications")];
    if let Ok(home) = home_dir() {
        dirs.push(home.join("Applications"));
    }
    dirs
}

pub(crate) fn git_id_config_file() -> Result<PathBuf> {
    config_file("git-identities.json")
}

pub(crate) fn git_id_data_dir() -> Result<PathBuf> {
    data_dir("git-id")
}

pub(crate) fn repos_mirror_config_file() -> Result<PathBuf> {
    config_file("repos.json")
}

pub(crate) fn repos_mirror_watch_state_file() -> Result<PathBuf> {
    state_dir("repos-watch-state.json")
}

pub(crate) fn yt_deps_dir() -> Result<PathBuf> {
    data_dir("deps")
}

pub(crate) fn log_dir() -> Result<PathBuf> {
    state_dir("logs")
}

/// $XDG_CONFIG_HOME when it is absolute, else ~/.config. Cross-platform apps
/// such as mGBA read it on macOS too, where lum's own config lives elsewhere.
pub(crate) fn xdg_config_home() -> Result<PathBuf> {
    xdg_home("XDG_CONFIG_HOME", ".config")
}

/// $XDG_DATA_HOME when it is absolute, else ~/.local/share.
pub(crate) fn xdg_data_home() -> Result<PathBuf> {
    xdg_home("XDG_DATA_HOME", ".local/share")
}

fn xdg_home(variable: &str, fallback: &str) -> Result<PathBuf> {
    match std::env::var_os(variable).map(PathBuf::from) {
        Some(dir) if dir.is_absolute() => Ok(dir),
        _ => home_path(fallback),
    }
}

/// The roaming application data folder: %APPDATA% on Windows.
pub(crate) fn roaming_app_data() -> Result<PathBuf> {
    Ok(base_dirs()?.config_dir().to_path_buf())
}

fn base_dirs() -> Result<BaseDirs> {
    BaseDirs::new().context("failed to determine platform directories")
}

/// The ROMs folder in the home folder. `lum backup roms` backs it up, and lum
/// points app saves at it where they would otherwise land elsewhere.
pub(crate) const ROMS_FOLDER: &str = "ROMs";

pub(crate) fn roms_dir() -> Result<PathBuf> {
    home_path(ROMS_FOLDER)
}

/// Resolve home directory or bail.
pub(crate) fn home_dir() -> Result<PathBuf> {
    UserDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .context("cannot determine home directory")
}

/// Join a relative path to the home directory.
pub(crate) fn home_path(relative: &str) -> Result<PathBuf> {
    Ok(home_dir()?.join(relative))
}

/// Expand a `~/` prefix to the home directory, if present.
pub(crate) fn expand_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = UserDirs::new().map(|d| d.home_dir().to_path_buf())
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

/// Canonicalize a path, falling back to a cleaned non-canonical version.
pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.components().collect())
}
