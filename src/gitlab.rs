//! Releases of projects on gitlab.com, for the apps published there.
//!
//! One endpoint, read with `crate::github`'s agent the way GitHub's releases
//! are: the `gitlab` crate would bring back reqwest, which ADR-0019 removed.

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct GitLabRelease {
    pub tag_name: String,
    assets: Assets,
}

#[derive(Debug, Deserialize)]
struct Assets {
    links: Vec<Link>,
}

/// A release asset. Its name is free text, such as "OctoMacLauncher 0.1.15 for
/// Mac (Apple Silicon)"; the file's name ends its URL.
#[derive(Debug, Deserialize)]
struct Link {
    url: String,
    /// The link under the release's own downloads path, when it has one.
    direct_asset_url: Option<String>,
}

/// The latest release of `project`, a path such as `group/name`.
pub(crate) fn latest_release(project: &str) -> Result<GitLabRelease> {
    // The API takes the URL-encoded path in place of the project's ID, and
    // redirects the permalink to the release itself.
    let url = format!(
        "https://gitlab.com/api/v4/projects/{}/releases/permalink/latest",
        project.replace('/', "%2F")
    );
    crate::github::agent()
        .get(&url)
        .call()
        .with_context(|| format!("failed to query GitLab API for {project}"))?
        .body_mut()
        .read_json()
        .with_context(|| format!("failed to parse GitLab release for {project}"))
}

impl GitLabRelease {
    /// The download URL of the asset whose file is `name`.
    pub(crate) fn asset_url(&self, name: &str) -> Result<&str> {
        self.assets
            .links
            .iter()
            .map(|link| link.direct_asset_url.as_deref().unwrap_or(&link.url))
            .find(|url| url.rsplit('/').next() == Some(name))
            .with_context(|| format!("GitLab release missing asset '{name}'"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What GitLab says about OctoMacLauncher 0.1.15, cut to what lum reads.
    const OCTOMAC: &str = r#"{
        "name": "OctoMacLauncher 0.1.15",
        "tag_name": "v0.1.15",
        "assets": {
            "count": 6,
            "sources": [
                {"format": "zip", "url": "https://gitlab.com/noinstances/octomac/-/archive/v0.1.15/octomac-v0.1.15.zip"}
            ],
            "links": [
                {
                    "name": "SHA-256 checksum",
                    "url": "https://gitlab.com/api/v4/projects/noinstances%2Foctomac/packages/generic/octomaclauncher/0.1.15/OctoMacLauncher-0.1.15-macOS-arm64.zip.sha256",
                    "direct_asset_url": "https://gitlab.com/noinstances/octomac/-/releases/v0.1.15/downloads/OctoMacLauncher-0.1.15-macOS-arm64.zip.sha256",
                    "link_type": "other"
                },
                {
                    "name": "OctoMacLauncher 0.1.15 for Mac (Apple Silicon)",
                    "url": "https://gitlab.com/api/v4/projects/noinstances%2Foctomac/packages/generic/octomaclauncher/0.1.15/OctoMacLauncher-0.1.15-macOS-arm64.zip",
                    "direct_asset_url": "https://gitlab.com/noinstances/octomac/-/releases/v0.1.15/downloads/OctoMacLauncher-0.1.15-macOS-arm64.zip",
                    "link_type": "package"
                }
            ]
        }
    }"#;

    #[test]
    fn assets_are_found_by_file_name_not_link_name() {
        let release: GitLabRelease = serde_json::from_str(OCTOMAC).unwrap();
        assert_eq!(release.tag_name, "v0.1.15");
        assert_eq!(
            release
                .asset_url("OctoMacLauncher-0.1.15-macOS-arm64.zip")
                .unwrap(),
            "https://gitlab.com/noinstances/octomac/-/releases/v0.1.15/downloads/OctoMacLauncher-0.1.15-macOS-arm64.zip"
        );
        assert!(
            release
                .asset_url("OctoMacLauncher 0.1.15 for Mac (Apple Silicon)")
                .is_err()
        );
        assert!(
            release
                .asset_url("OctoMacLauncher-0.1.14-macOS-arm64.zip")
                .is_err()
        );
    }

    #[test]
    fn a_link_without_a_downloads_path_is_named_by_its_url() {
        let release: GitLabRelease = serde_json::from_str(
            r#"{"tag_name": "v1.0", "assets": {"links": [{"name": "App", "url": "https://example.com/files/App-1.0.zip"}]}}"#,
        )
        .unwrap();
        assert_eq!(
            release.asset_url("App-1.0.zip").unwrap(),
            "https://example.com/files/App-1.0.zip"
        );
    }
}
