use std::{collections::BTreeMap, fs, path::PathBuf, time::SystemTime};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::catalog::AppSpec;

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
    pub fn effective_version(&self) -> Option<&str> {
        self.stored_version.as_deref()
    }
}

pub(crate) fn local_status(spec: &AppSpec) -> Result<LocalStatus> {
    let stored = load_state()?;
    let path = stored
        .apps
        .get(spec.name)
        .map(|s| s.path.clone())
        .unwrap_or(app_path(spec)?);
    let exists = path.exists();
    let managed = stored.apps.get(spec.name).is_some_and(|s| s.installed);
    let stored_version = stored
        .apps
        .get(spec.name)
        .map(|s| s.installed_version.clone());
    Ok(LocalStatus {
        managed,
        path,
        exists,
        stored_version,
    })
}

/// Bundles with the managed app's name found outside lum's install root.
pub(crate) fn foreign_copies(spec: &AppSpec) -> Result<Vec<PathBuf>> {
    let managed = app_path(spec)?;
    let mut found = Vec::new();
    for dir in crate::paths::foreign_scan_dirs() {
        let candidate = dir.join(spec.app_bundle);
        if candidate.exists() && candidate != managed {
            found.push(candidate);
        }
    }
    Ok(found)
}

pub(crate) fn installed_at(spec: &AppSpec) -> Result<Option<SystemTime>> {
    Ok(load_state()?.apps.get(spec.name).map(|s| s.installed_at))
}

pub(crate) fn save_state(state: &AppsState) -> Result<()> {
    let path = crate::paths::apps_state_file()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(state)?)?;
    Ok(())
}

pub(crate) fn app_path(spec: &AppSpec) -> Result<PathBuf> {
    Ok(apps_dir()?.join(spec.app_bundle))
}

pub(crate) fn apps_dir() -> Result<PathBuf> {
    crate::paths::apps_dir()
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
