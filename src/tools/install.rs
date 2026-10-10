use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use xz2::read::XzDecoder;

use super::catalog::ToolSpec;
use super::platform::{ArchiveType, Artifact};
use super::state::{ArtifactState, ToolState, bin_dir, load_state, save_state, tool_path};
use crate::artifact;

pub(crate) fn install_artifact(
    spec: &ToolSpec,
    artifact: &Artifact,
    preserve_installed_at: Option<SystemTime>,
) -> Result<ToolState> {
    fs::create_dir_all(bin_dir()?)?;
    let target = tool_path(spec)?;
    let temp_dir = tempfile::Builder::new().prefix("lum-tools-").tempdir()?;
    let download = temp_dir.path().join(&artifact.asset_name);
    crate::github::download_asset(&artifact.download_url, &download)?;
    let mut stored = load_state()?;
    let downloaded_sha256 = crate::github::sha256_file(&download)?;
    let pinned = stored
        .tools
        .get(spec.name)
        .filter(|previous| {
            previous.artifact.release_tag == artifact.release_tag
                && previous.artifact.asset_name == artifact.asset_name
        })
        .and_then(|previous| previous.artifact.checksum.as_deref());
    let expected = artifact
        .checksum_sha256
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or(pinned);
    crate::github::verify_checksum(
        expected,
        &downloaded_sha256,
        &format!(
            "{} release {} asset {}",
            spec.name, artifact.release_tag, artifact.asset_name
        ),
    )?;
    let source = materialize_binary(&download, temp_dir.path(), artifact)?;
    artifact::install_executable(&source, &target)?;

    let now = SystemTime::now();
    let installed_at = preserve_installed_at
        .or_else(|| stored.tools.get(spec.name).map(|s| s.installed_at))
        .unwrap_or(now);
    let tool_state = ToolState {
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
    stored.tools.insert(spec.name.to_owned(), tool_state);
    save_state(&stored)?;
    Ok(stored.tools.remove(spec.name).unwrap())
}

fn materialize_binary(download: &Path, temp_dir: &Path, artifact: &Artifact) -> Result<PathBuf> {
    if artifact.archive_type == ArchiveType::Binary {
        return Ok(download.to_owned());
    }
    let extract_dir = temp_dir.join("extract");
    fs::create_dir_all(&extract_dir)?;
    extract_archive(download, &extract_dir, artifact.archive_type)?;
    let expected = extract_dir.join(Path::new(&artifact.binary_path));
    if expected.is_file() {
        return Ok(expected);
    }
    let basename = Path::new(&artifact.binary_path)
        .file_name()
        .context("artifact binary path missing filename")?;
    find_file_named(&extract_dir, basename).with_context(|| {
        format!(
            "could not find {} in extracted archive",
            artifact.binary_path
        )
    })
}

fn extract_archive(src: &Path, dest: &Path, archive_type: ArchiveType) -> Result<()> {
    match archive_type {
        ArchiveType::Zip => {
            let file = fs::File::open(src)?;
            zip::ZipArchive::new(file)?.extract(dest)?;
        }
        ArchiveType::TarGz => {
            let file = fs::File::open(src)?;
            tar::Archive::new(GzDecoder::new(file)).unpack(dest)?;
        }
        ArchiveType::TarXz => {
            let file = fs::File::open(src)?;
            tar::Archive::new(XzDecoder::new(file)).unpack(dest)?;
        }
        ArchiveType::Binary => {}
    }
    Ok(())
}

fn find_file_named(dir: &Path, filename: &std::ffi::OsStr) -> Result<PathBuf> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Ok(found) = find_file_named(&path, filename) {
                return Ok(found);
            }
        } else if path.file_name() == Some(filename) {
            return Ok(path);
        }
    }
    Err(anyhow::anyhow!("not found"))
}
