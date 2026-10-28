use std::{collections::BTreeMap, path::Path};

use anyhow::{Context, Result};

use super::catalog::{self, VsixSpec};
use super::editor::{self, Editor};

pub(crate) struct VsixArtifact {
    pub version: String,
    pub release_tag: String,
    pub asset_name: String,
    pub download_url: String,
    /// SHA-256 published by GitHub for the asset, when available.
    pub sha256: Option<String>,
}

pub(crate) fn resolve_latest(spec: &VsixSpec) -> Result<VsixArtifact> {
    if let Ok(path) = std::env::var(catalog::test_artifact_env(spec)) {
        let asset_name = Path::new(&path)
            .file_name()
            .context("test artifact path has no file name")?
            .to_string_lossy()
            .into_owned();
        let version = spec
            .version_from_asset(&asset_name)
            .with_context(|| {
                format!(
                    "test artifact {asset_name} does not match {}",
                    spec.asset_name("<version>")
                )
            })?
            .to_owned();
        return Ok(VsixArtifact {
            version,
            release_tag: "test".into(),
            asset_name,
            download_url: path,
            sha256: None,
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
    let asset_name = spec.asset_name(&version);
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .with_context(|| format!("release asset {asset_name} not found"))?;
    Ok(VsixArtifact {
        version,
        release_tag: tag.to_owned(),
        asset_name,
        download_url: asset.browser_download_url.clone(),
        sha256: sha256_from_digest(asset.digest.as_deref()),
    })
}

/// Download, verify, and install the artifact, then confirm the editor reports
/// the new version. Returns the editor's installed extensions afterwards.
pub(crate) fn install(
    spec: &VsixSpec,
    artifact: &VsixArtifact,
    editor: &Editor,
) -> Result<BTreeMap<String, String>> {
    let temp_dir = tempfile::Builder::new().prefix("lum-vsix-").tempdir()?;
    let download = temp_dir.path().join(&artifact.asset_name);
    crate::github::download_asset(&artifact.download_url, &download)?;
    let downloaded_sha256 = crate::github::sha256_file(&download)?;
    crate::github::verify_checksum(
        artifact.sha256.as_deref(),
        &downloaded_sha256,
        &format!(
            "{} release {} asset {}",
            spec.name, artifact.release_tag, artifact.asset_name
        ),
    )?;
    editor.install(&download)?;

    let installed = editor.installed_extensions()?;
    match editor::installed_version(&installed, spec.extension_id) {
        Some(version) if version == artifact.version => Ok(installed),
        Some(version) => anyhow::bail!(
            "{} reports {}@{version} after installing {}",
            editor.name,
            spec.extension_id,
            artifact.version
        ),
        None => anyhow::bail!(
            "{} does not list {} after installing {}",
            editor.name,
            spec.extension_id,
            artifact.asset_name
        ),
    }
}

/// GitHub reports asset digests as `sha256:<hex>`; other algorithms are ignored.
fn sha256_from_digest(digest: Option<&str>) -> Option<String> {
    digest?
        .trim()
        .strip_prefix("sha256:")
        .filter(|hex| !hex.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_from_digest_accepts_only_sha256() {
        assert_eq!(
            sha256_from_digest(Some("sha256:8dda79dc")).as_deref(),
            Some("8dda79dc")
        );
        assert_eq!(sha256_from_digest(Some("sha512:abc")), None);
        assert_eq!(sha256_from_digest(Some("sha256:")), None);
        assert_eq!(sha256_from_digest(None), None);
    }
}
