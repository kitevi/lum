use std::{collections::BTreeMap, fs, path::PathBuf, time::SystemTime};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::catalog::{AppSpec, Build, Package};

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct AppsState {
    #[serde(default)]
    pub version: String,
    pub apps: BTreeMap<String, AppState>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct AppState {
    pub installed: bool,
    pub path: PathBuf,
    pub installed_version: String,
    pub installed_at: SystemTime,
    pub updated_at: SystemTime,
    pub artifact: ArtifactState,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ArtifactState {
    pub release_tag: String,
    pub asset_name: String,
    pub download_url: String,
    #[serde(default)]
    pub checksum: Option<String>,
}

pub(crate) struct LocalStatus {
    pub managed: bool,
    pub path: PathBuf,
    pub exists: bool,
    pub stored_version: Option<String>,
}

impl LocalStatus {
    /// The version lum installed, if it is still on disk.
    pub(crate) fn installed_version(&self) -> Option<&str> {
        self.stored_version.as_deref().filter(|_| self.exists)
    }
}

pub(crate) fn local_status(spec: &AppSpec, build: &Build) -> Result<LocalStatus> {
    let stored = load_state()?;
    let entry = stored.apps.get(spec.name);
    let path = match entry {
        Some(entry) => entry.path.clone(),
        None => app_path(spec, build)?,
    };
    Ok(LocalStatus {
        managed: entry.is_some_and(|entry| entry.installed),
        exists: path.exists(),
        stored_version: entry.map(|entry| entry.installed_version.clone()),
        path,
    })
}

/// Where this build is installed: the bundle name on macOS, the app name elsewhere.
pub(crate) fn app_path(spec: &AppSpec, build: &Build) -> Result<PathBuf> {
    let name = match build.package {
        Package::Bundle { bundle } => bundle,
        Package::AppImage | Package::Folder { .. } => spec.name,
    };
    Ok(crate::paths::apps_dir()?.join(name))
}

/// Copies of a macOS bundle outside lum's install root. With two copies of one
/// bundle Launch Services may open either, so lum refuses to install beside one.
pub(crate) fn foreign_copies(spec: &AppSpec, build: &Build) -> Result<Vec<PathBuf>> {
    let Package::Bundle { bundle } = build.package else {
        return Ok(Vec::new());
    };
    let managed = app_path(spec, build)?;
    Ok(crate::paths::foreign_scan_dirs()
        .into_iter()
        .map(|dir| dir.join(bundle))
        .filter(|candidate| candidate.exists() && *candidate != managed)
        .collect())
}

pub(crate) fn save_state(state: &AppsState) -> Result<()> {
    let path = crate::paths::apps_state_file()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(state)?)?;
    Ok(())
}

pub(crate) fn load_state() -> Result<AppsState> {
    let path = crate::paths::apps_state_file()?;
    if !path.exists() {
        return Ok(AppsState {
            version: "1.0".into(),
            apps: BTreeMap::new(),
        });
    }
    let data = fs::read_to_string(&path)?;
    let mut stored: AppsState = serde_json::from_str(&data)?;
    if stored.version.is_empty() {
        stored.version = "1.0".into();
    }
    Ok(stored)
}
