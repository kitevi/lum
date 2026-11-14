use std::{fs, io, path::Path, time::SystemTime};

use anyhow::{Context, Result, bail};

use super::catalog::{self, AppSpec, Build};
use super::state::{self, AppState, ArtifactState};
use super::{launcher, package};

pub(crate) struct AppArtifact {
    pub version: String,
    pub release_tag: String,
    pub asset_name: String,
    pub download_url: String,
}

pub(crate) fn resolve_latest(spec: &AppSpec, build: &Build) -> Result<AppArtifact> {
    if let Ok(path) = std::env::var(catalog::test_artifact_env(spec)) {
        return Ok(AppArtifact {
            version: "test".into(),
            release_tag: "test".into(),
            asset_name: Path::new(&path)
                .file_name()
                .context("test artifact has no file name")?
                .to_string_lossy()
                .into_owned(),
            download_url: path,
        });
    }
    let release = crate::github::latest_release(spec.owner, spec.repo)?;
    let tag = release.tag_name.as_deref().unwrap_or_default().trim();
    let version = tag.trim_start_matches('v').to_owned();
    if version.is_empty() {
        bail!(
            "release metadata for {} is missing a version tag",
            spec.name
        );
    }
    let asset_name = build.asset.replace("{version}", &version);
    let download_url = release.asset_url(&asset_name)?.to_owned();
    Ok(AppArtifact {
        version,
        release_tag: tag.to_owned(),
        asset_name,
        download_url,
    })
}

/// Download, verify and unpack the app, then swap it in. A previous install
/// stays in place until the new one is complete.
pub(crate) fn install_app(
    spec: &AppSpec,
    build: &Build,
    artifact: &AppArtifact,
) -> Result<AppState> {
    let root = crate::paths::apps_dir()?;
    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;
    let target = state::app_path(spec, build)?;
    let downloads = tempfile::Builder::new().prefix("lum-apps-").tempdir()?;
    let download = downloads.path().join(&artifact.asset_name);
    crate::github::download_asset(&artifact.download_url, &download)?;
    let mut stored = state::load_state()?;
    let checksum = crate::github::sha256_file(&download)?;
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
        &checksum,
        &format!(
            "{} release {} asset {}",
            spec.name, artifact.release_tag, artifact.asset_name
        ),
    )?;

    // Unpack beside the target, so moving it into place is a rename.
    let staging = tempfile::Builder::new()
        .prefix(".lum-staging-")
        .tempdir_in(&root)?;
    let app = package::unpack(&build.package, &download, staging.path())?;
    replace(&app, &target, &staging.path().join("previous"))?;
    if let Err(error) = launcher::install(spec, &build.package, &target) {
        // The app itself is in place; rerunning with --force retries the launcher.
        eprintln!(
            "warning: {} is installed but its launcher is not: {error:#}",
            spec.name
        );
        tracing::warn!(app = spec.name, error = ?error, "failed to write the launcher");
    }

    let now = SystemTime::now();
    let app_state = AppState {
        installed: true,
        path: target,
        installed_version: artifact.version.clone(),
        installed_at: stored
            .apps
            .get(spec.name)
            .map_or(now, |previous| previous.installed_at),
        updated_at: now,
        artifact: ArtifactState {
            release_tag: artifact.release_tag.clone(),
            asset_name: artifact.asset_name.clone(),
            download_url: artifact.download_url.clone(),
            checksum: Some(checksum),
        },
    };
    stored.apps.insert(spec.name.to_owned(), app_state);
    state::save_state(&stored)?;
    Ok(stored.apps.remove(spec.name).unwrap())
}

/// Move `new` to `target`. An existing install moves to `previous` first and
/// comes back if the swap fails.
fn replace(new: &Path, target: &Path, previous: &Path) -> Result<()> {
    let had_previous = fs::symlink_metadata(target).is_ok();
    if had_previous {
        fs::rename(target, previous)
            .with_context(|| format!("failed to move {} aside", target.display()))?;
    }
    if let Err(error) = fs::rename(new, target) {
        if had_previous {
            let _ = fs::rename(previous, target);
        }
        return Err(error)
            .with_context(|| format!("failed to move the new install to {}", target.display()));
    }
    Ok(())
}

/// Remove the app and its launcher. Its settings and saves stay.
pub(crate) fn uninstall(spec: &AppSpec, build: &Build) -> Result<()> {
    let path = state::app_path(spec, build)?;
    let removed = match fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(&path),
        Ok(_) => fs::remove_file(&path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    };
    removed.with_context(|| format!("failed to remove {}", path.display()))?;
    launcher::remove(spec, &build.package)?;
    let mut stored = state::load_state()?;
    stored.apps.remove(spec.name);
    state::save_state(&stored)
}
