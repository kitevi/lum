use anyhow::{Context, Result, bail};

use super::catalog::ToolSpec;
use crate::github::GitHubRelease;

#[derive(Debug)]
pub(crate) struct Artifact {
    pub version: String,
    pub release_tag: String,
    pub asset_name: String,
    pub download_url: String,
    pub archive_type: ArchiveType,
    pub binary_path: String,
    pub checksum_sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArchiveType {
    Binary,
    Zip,
    TarGz,
    TarXz,
}

pub(crate) fn artifact_for_release(spec: &ToolSpec, release: &GitHubRelease) -> Result<Artifact> {
    let tag = release.tag_name.as_deref().unwrap_or_default().trim();
    let version = release_version(spec, release)?;
    let (asset_name, binary_path) = asset_for_current_platform(spec)?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .with_context(|| format!("release asset {asset_name} not found"))?;
    Ok(Artifact {
        version,
        release_tag: tag.to_owned(),
        asset_name: asset.name.clone(),
        download_url: asset.browser_download_url.clone(),
        archive_type: detect_archive_type(&asset.name),
        binary_path,
        checksum_sha256: None,
    })
}

fn release_version(spec: &ToolSpec, release: &GitHubRelease) -> Result<String> {
    let tag = release.tag_name.as_deref().unwrap_or_default().trim();
    let name = release.name.as_deref().unwrap_or_default().trim();
    let value = tag.trim_start_matches('v').to_owned();
    if !value.is_empty() {
        Ok(value)
    } else if !name.is_empty() {
        Ok(name.trim_start_matches('v').to_owned())
    } else {
        bail!(
            "release metadata for {} is missing a version tag",
            spec.name
        )
    }
}

pub(crate) fn detect_archive_type(name: &str) -> ArchiveType {
    if name.ends_with(".tar.gz") {
        ArchiveType::TarGz
    } else if name.ends_with(".tar.xz") {
        ArchiveType::TarXz
    } else if name.ends_with(".zip") {
        ArchiveType::Zip
    } else {
        ArchiveType::Binary
    }
}

pub(crate) fn installed_filename(binary: &str) -> String {
    if cfg!(windows) && !binary.to_ascii_lowercase().ends_with(".exe") {
        format!("{binary}.exe")
    } else {
        binary.to_owned()
    }
}

// No tool in the catalog yet, so there is no release asset mapping to resolve.
fn asset_for_current_platform(spec: &ToolSpec) -> Result<(String, String)> {
    bail!("no release asset mapping for {}", spec.name)
}
