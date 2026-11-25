use std::{fs, io, path::Path, time::SystemTime};

use anyhow::{Context, Result, bail};

use super::catalog::{self, AppSpec, Build, Package, Source};
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
    match spec.source {
        Source::GitHub { owner, repo } => {
            let release = crate::github::latest_release(owner, repo)?;
            let tag = release.tag_name.as_deref().unwrap_or_default();
            artifact(spec, build, tag, |name| release.asset_url(name))
        }
        Source::GitLab { project } => {
            let release = crate::gitlab::latest_release(project)?;
            artifact(spec, build, &release.tag_name, |name| {
                release.asset_url(name)
            })
        }
    }
}

/// The asset of `build` in the release tagged `tag`; `asset_url` looks up its
/// download URL by file name.
fn artifact<'a>(
    spec: &AppSpec,
    build: &Build,
    tag: &str,
    asset_url: impl FnOnce(&str) -> Result<&'a str>,
) -> Result<AppArtifact> {
    let tag = tag.trim();
    let version = tag.trim_start_matches('v').to_owned();
    if version.is_empty() {
        bail!(
            "release metadata for {} is missing a version tag",
            spec.name
        );
    }
    let asset_name = build.asset.replace("{version}", &version);
    let download_url = asset_url(&asset_name)?.to_owned();
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
    let keep = match build.package {
        Package::Folder { keep, .. } => keep,
        Package::AppImage | Package::Bundle { .. } => &[],
    };
    replace(&app, &target, &staging.path().join("previous"), keep)?;
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

/// Move `new` to `target`. An existing install moves to `previous` first, and
/// the `keep` entries its program wrote there move into the new install. If
/// anything fails, they move back and the previous install returns.
fn replace(new: &Path, target: &Path, previous: &Path, keep: &[&str]) -> Result<()> {
    let had_previous = fs::symlink_metadata(target).is_ok();
    if had_previous {
        fs::rename(target, previous)
            .with_context(|| format!("failed to move {} aside", target.display()))?;
    }
    let mut carried = Vec::new();
    let swapped = carry_over(previous, new, keep, &mut carried).and_then(|()| {
        fs::rename(new, target)
            .with_context(|| format!("failed to move the new install to {}", target.display()))
    });
    if swapped.is_err() {
        for name in carried {
            let _ = fs::rename(new.join(name), previous.join(name));
        }
        if had_previous {
            let _ = fs::rename(previous, target);
        }
    }
    swapped
}

/// Move the `keep` entries of the previous install into the new one. The copy
/// the program wrote wins over one the new build ships.
fn carry_over<'a>(
    previous: &Path,
    new: &Path,
    keep: &[&'a str],
    carried: &mut Vec<&'a str>,
) -> Result<()> {
    for &name in keep {
        let from = previous.join(name);
        if fs::symlink_metadata(&from).is_err() {
            continue;
        }
        let to = new.join(name);
        remove_path(&to)?;
        fs::rename(&from, &to).with_context(|| format!("failed to keep {}", from.display()))?;
        carried.push(name);
    }
    Ok(())
}

/// Remove a file, symlink or folder; a missing one is fine.
fn remove_path(path: &Path) -> Result<()> {
    let removed = match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    };
    removed.with_context(|| format!("failed to remove {}", path.display()))
}

/// Remove the app and its launcher. Its settings and saves stay.
pub(crate) fn uninstall(spec: &AppSpec, build: &Build) -> Result<()> {
    remove_path(&state::app_path(spec, build)?)?;
    launcher::remove(spec, &build.package)?;
    let mut stored = state::load_state()?;
    stored.apps.remove(spec.name);
    state::save_state(&stored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_update_keeps_what_the_program_wrote_beside_itself() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("snes9x");
        fs::create_dir_all(target.join("Saves")).unwrap();
        fs::write(target.join("snes9x-x64.exe"), "old").unwrap();
        fs::write(target.join("snes9x.conf"), "mine").unwrap();
        fs::write(target.join("Saves").join("game.srm"), "save").unwrap();
        let new = dir.path().join("new");
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("snes9x-x64.exe"), "new").unwrap();
        fs::write(new.join("snes9x.conf"), "shipped").unwrap();

        replace(
            &new,
            &target,
            &dir.path().join("previous"),
            &["snes9x.conf", "Saves", "Cheats"],
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(target.join("snes9x-x64.exe")).unwrap(),
            "new"
        );
        assert_eq!(
            fs::read_to_string(target.join("snes9x.conf")).unwrap(),
            "mine"
        );
        assert_eq!(
            fs::read_to_string(target.join("Saves").join("game.srm")).unwrap(),
            "save"
        );
        assert!(!target.join("Cheats").exists());
    }

    #[test]
    fn a_failed_swap_restores_the_previous_install() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("snes9x");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("snes9x.conf"), "mine").unwrap();

        let missing = dir.path().join("missing");
        let swapped = replace(
            &missing,
            &target,
            &dir.path().join("previous"),
            &["snes9x.conf"],
        );

        assert!(swapped.is_err());
        assert_eq!(
            fs::read_to_string(target.join("snes9x.conf")).unwrap(),
            "mine"
        );
    }
}
