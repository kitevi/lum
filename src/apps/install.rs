use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context, Result};
use ignore::Walk;

use super::catalog::{self, AppSpec};
use super::state::{AppState, ArtifactState, app_path, apps_dir, load_state, save_state};

pub(crate) struct AppArtifact {
    pub version: String,
    pub release_tag: String,
    pub asset_name: String,
    pub download_url: String,
}

pub(crate) fn resolve_latest(spec: &AppSpec) -> Result<AppArtifact> {
    if let Ok(path) = std::env::var(catalog::test_artifact_env(spec)) {
        return Ok(AppArtifact {
            version: "test".into(),
            release_tag: "test".into(),
            asset_name: Path::new(&path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            download_url: path,
        });
    }
    let release = crate::github::latest_release(spec.owner, spec.repo)?;
    let tag = release.tag_name.as_deref().unwrap_or_default().trim();
    let version = tag.trim_start_matches('v').to_owned();
    if version.is_empty() {
        anyhow::bail!(
            "release metadata for {} is missing a version tag",
            spec.name
        );
    }
    let asset_name = format!("{}_{}.zip", spec.asset_prefix, version);
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .with_context(|| format!("release asset {asset_name} not found"))?;
    Ok(AppArtifact {
        version,
        release_tag: tag.to_owned(),
        asset_name: asset.name.clone(),
        download_url: asset.browser_download_url.clone(),
    })
}

pub(crate) fn install_app(
    spec: &AppSpec,
    artifact: &AppArtifact,
    preserve_installed_at: Option<SystemTime>,
) -> Result<AppState> {
    let target = app_path(spec)?;
    let dir = apps_dir()?;
    fs::create_dir_all(&dir)?;
    let temp_dir = tempfile::Builder::new().prefix("lum-apps-").tempdir()?;
    let download = temp_dir.path().join(&artifact.asset_name);
    crate::github::download_asset(&artifact.download_url, &download)?;
    let mut stored = load_state()?;
    let downloaded_sha256 = crate::github::sha256_file(&download)?;
    let pinned = stored
        .apps
        .get(spec.name)
        .filter(|previous| {
            previous.artifact.release_tag == artifact.release_tag
                && previous.artifact.asset_name == artifact.asset_name
        })
        .and_then(|previous| previous.artifact.checksum.as_deref());
    crate::github::verify_checksum(
        pinned,
        &downloaded_sha256,
        &format!(
            "{} release {} asset {}",
            spec.name, artifact.release_tag, artifact.asset_name
        ),
    )?;
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir_all(&extract_dir)?;
    let file = fs::File::open(&download)?;
    zip::ZipArchive::new(file)?.extract(&extract_dir)?;
    let bundle = find_bundle_dir(&extract_dir, spec.app_bundle)?;
    if target.exists() {
        fs::remove_dir_all(&target)?;
    }
    fs_extra::dir::copy(&bundle, &dir, &fs_extra::dir::CopyOptions::new())
        .with_context(|| format!("failed to copy {} to {}", spec.app_bundle, dir.display()))?;

    let now = SystemTime::now();
    let installed_at = preserve_installed_at
        .or_else(|| stored.apps.get(spec.name).map(|s| s.installed_at))
        .unwrap_or(now);
    let app_state = AppState {
        installed: true,
        path: target,
        installed_version: artifact.version.clone(),
        installed_at,
        updated_at: now,
        artifact: ArtifactState {
            release_tag: artifact.release_tag.clone(),
            asset_name: artifact.asset_name.clone(),
            download_url: artifact.download_url.clone(),
            checksum: Some(downloaded_sha256),
        },
    };
    stored.apps.insert(spec.name.to_owned(), app_state);
    save_state(&stored)?;
    Ok(stored.apps.remove(spec.name).unwrap())
}

fn find_bundle_dir(root: &Path, bundle: &str) -> Result<PathBuf> {
    for entry in Walk::new(root) {
        let entry = entry.with_context(|| format!("failed to walk {}", root.display()))?;
        if entry.file_type().is_some_and(|t| t.is_dir())
            && entry.file_name() == std::ffi::OsStr::new(bundle)
        {
            return Ok(entry.into_path());
        }
    }
    anyhow::bail!("{bundle} not found in extracted archive")
}
