use std::{fs, io, path::Path, time::Duration};

use anyhow::{Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
pub(crate) struct GitHubRelease {
    pub tag_name: Option<String>,
    pub name: Option<String>,
    pub assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
}

pub(crate) fn latest_release(owner: &str, repo: &str) -> Result<GitHubRelease> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent("lum-tools/1.0")
        .build()?;
    let text = client.get(&url).send()?.error_for_status()?.text()?;
    serde_json::from_str(&text)
        .with_context(|| format!("failed to parse GitHub release for {owner}/{repo}"))
}

/// Download "url" to "dest". A url that names an existing local file is copied
/// instead of fetched — the network-free test hook shared by "tools" and "apps".
pub(crate) fn download_asset(url: &str, dest: &Path) -> Result<()> {
    let path = Path::new(url);
    if path.exists() {
        fs::copy(path, dest)?;
        return Ok(());
    }
    let mut response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(600))
        .user_agent("lum-tools/1.0")
        .build()?
        .get(url)
        .send()?
        .error_for_status()?;
    let mut out = fs::File::create(dest)?;
    io::copy(&mut response, &mut out)?;
    Ok(())
}

pub(crate) fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = io::Read::read(&mut file, &mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Missing or blank expected digests are treated as unpinned.
pub(crate) fn verify_checksum(expected: Option<&str>, actual: &str, subject: &str) -> Result<()> {
    let Some(expected) = expected.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    if !actual.eq_ignore_ascii_case(expected) {
        anyhow::bail!(
            "checksum mismatch for {subject}: expected sha256 {expected}, downloaded {actual}"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_file_matches_known_digest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("asset");
        std::fs::write(&path, b"hello").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn verify_checksum_ignores_unpinned_values() {
        assert!(verify_checksum(None, "abc", "tool release v1").is_ok());
        assert!(verify_checksum(Some(""), "abc", "tool release v1").is_ok());
        assert!(verify_checksum(Some("   "), "abc", "tool release v1").is_ok());
    }

    #[test]
    fn verify_checksum_accepts_matching_digest_case_insensitively() {
        assert!(verify_checksum(Some("ABCDEF"), "abcdef", "tool release v1").is_ok());
    }

    #[test]
    fn verify_checksum_rejects_mismatched_digest() {
        let error = verify_checksum(Some("aabbcc"), "deadbeef", "tool release v1").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("checksum mismatch for tool release v1"));
        assert!(message.contains("aabbcc"));
        assert!(message.contains("deadbeef"));
    }
}
