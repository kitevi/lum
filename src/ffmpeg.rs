use std::{
    io::{self, Cursor},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::artifact;

const STALE_THRESHOLD: Duration = Duration::from_secs(14 * 24 * 60 * 60);
const TEST_ARTIFACT_ENV: &str = "LUM_FFMPEG_TEST_ARTIFACT";
const DISABLE_AUTO_PROVISION_ENV: &str = "LUM_FFMPEG_DISABLE_AUTO_PROVISION";

/// Resolve the ffmpeg binary to use.
///
/// 1. If ffmpeg is on $PATH, use that.
/// 2. Otherwise, use the auto-provisioned copy at `data_dir()/deps/ffmpeg`.
///    If the auto-provisioned copy doesn't exist yet, download it.
///    If it exists but is stale (>14 days since download), re-download it.
pub(crate) async fn resolve() -> Result<PathBuf> {
    if let Ok(path) = which::which("ffmpeg") {
        tracing::debug!("using ffmpeg from PATH: {}", path.display());
        return Ok(path);
    }

    if auto_provision_disabled() {
        return Err(not_installed_error());
    }

    let dir = deps_dir()?;
    let local = dir.join(ffmpeg_binary_name());
    let state_path = dir.join("ffmpeg.json");

    if local.exists() {
        if is_stale(&state_path) {
            tracing::info!("auto-provisioned ffmpeg is stale, refreshing");
            if let Err(error) = try_update(&local, &state_path).await {
                tracing::warn!(error = %error, "failed to update ffmpeg; using cached copy");
            }
        }
        return Ok(local);
    }

    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create deps dir {}", dir.display()))?;
    install_ffmpeg(&local, &state_path).await?;
    Ok(local)
}

/// Resolve the ffplay binary to use for radio playback.
pub(crate) async fn resolve_ffplay() -> Result<PathBuf> {
    if let Ok(path) = which::which("ffplay") {
        tracing::debug!("using ffplay from PATH: {}", path.display());
        return Ok(path);
    }

    let dir = deps_dir()?;
    let local = dir.join(ffplay_binary_name());
    if local.exists() {
        return Ok(local);
    }

    // Refresh the managed ffmpeg bundle; supported archives usually include ffplay too.
    let _ = resolve().await;
    if local.exists() {
        return Ok(local);
    }

    Err(anyhow::anyhow!(
        "ffplay is not installed. Install ffmpeg with ffplay, or refresh lum's ffmpeg dependency cache"
    ))
}

fn auto_provision_disabled() -> bool {
    std::env::var_os(DISABLE_AUTO_PROVISION_ENV).is_some()
}

fn not_installed_error() -> anyhow::Error {
    anyhow::anyhow!(
        "ffmpeg is not installed. Install it:\n  \
         macOS: brew install ffmpeg\n  \
         Linux: sudo apt install ffmpeg\n  \
         Windows: scoop install ffmpeg"
    )
}

fn is_stale(state_path: &Path) -> bool {
    let data = match std::fs::read_to_string(state_path) {
        Ok(data) => data,
        Err(_) => return true,
    };
    let state: FfmpegState = match serde_json::from_str(&data) {
        Ok(state) => state,
        Err(_) => return true,
    };
    let last_downloaded = match state.last_downloaded {
        Some(ts) => SystemTime::UNIX_EPOCH + Duration::from_secs(ts),
        None => return true,
    };
    SystemTime::now()
        .duration_since(last_downloaded)
        .unwrap_or(Duration::ZERO)
        > STALE_THRESHOLD
}

async fn try_update(local: &Path, state_path: &Path) -> Result<()> {
    install_ffmpeg(local, state_path).await
}

async fn install_ffmpeg(local: &Path, state_path: &Path) -> Result<()> {
    if let Ok(test_artifact) = std::env::var(TEST_ARTIFACT_ENV) {
        install_from_local_artifact(Path::new(&test_artifact), local, state_path)?;
        return Ok(());
    }

    download_ffmpeg(local, state_path).await
}

async fn download_ffmpeg(local: &Path, state_path: &Path) -> Result<()> {
    let dl = resolve_download(std::env::consts::OS, std::env::consts::ARCH)?;

    let (download_url, kind) = match &dl {
        FfmpegDownload::Direct { url, kind } => (url.to_string(), *kind),
        FfmpegDownload::BtbN { asset_name, kind } => {
            let info = fetch_latest_release_info(asset_name).await?;
            (info.download_url, *kind)
        }
    };

    tracing::info!("downloading ffmpeg from {download_url}");

    let archive_data = reqwest::get(&download_url)
        .await
        .with_context(|| format!("failed to download ffmpeg from {download_url}"))?
        .error_for_status()?
        .bytes()
        .await?;

    extract_and_install(kind, &archive_data, local)?;
    write_state(state_path)
}

struct ReleaseInfo {
    download_url: String,
}

async fn fetch_latest_release_info(asset_name: &str) -> Result<ReleaseInfo> {
    let repo = "BtbN/FFmpeg-Builds";
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let resp: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .header("User-Agent", "lum")
        .send()
        .await
        .with_context(|| format!("failed to query GitHub API for {repo}"))?
        .error_for_status()?
        .json()
        .await?;

    let download_url = resp["assets"]
        .as_array()
        .and_then(|assets| {
            assets
                .iter()
                .find(|asset| asset["name"].as_str() == Some(asset_name))
        })
        .and_then(|asset| asset["browser_download_url"].as_str())
        .with_context(|| format!("GitHub release missing asset '{asset_name}'"))?
        .to_string();

    Ok(ReleaseInfo { download_url })
}

#[derive(Debug, Clone, Copy)]
enum ArchiveKind {
    TarXz,
    Zip,
}

#[derive(Debug, Clone)]
enum FfmpegDownload {
    BtbN {
        asset_name: &'static str,
        kind: ArchiveKind,
    },
    Direct {
        url: &'static str,
        kind: ArchiveKind,
    },
}

fn resolve_download(os: &str, arch: &str) -> Result<FfmpegDownload> {
    match (os, arch) {
        ("linux", "x86_64") => Ok(FfmpegDownload::BtbN {
            asset_name: "ffmpeg-master-latest-linux64-gpl.tar.xz",
            kind: ArchiveKind::TarXz,
        }),
        ("linux", "aarch64") => Ok(FfmpegDownload::BtbN {
            asset_name: "ffmpeg-master-latest-linuxarm64-gpl.tar.xz",
            kind: ArchiveKind::TarXz,
        }),
        ("windows", "x86_64") => Ok(FfmpegDownload::BtbN {
            asset_name: "ffmpeg-master-latest-win64-gpl.zip",
            kind: ArchiveKind::Zip,
        }),
        ("windows", "aarch64") => Ok(FfmpegDownload::BtbN {
            asset_name: "ffmpeg-master-latest-winarm64-gpl.zip",
            kind: ArchiveKind::Zip,
        }),
        ("macos", "aarch64") => Ok(FfmpegDownload::Direct {
            url: "https://ffmpeg.martin-riedl.de/redirect/latest/macos/arm64/snapshot/ffmpeg.zip",
            kind: ArchiveKind::Zip,
        }),
        ("macos", "x86_64") => Ok(FfmpegDownload::Direct {
            url: "https://ffmpeg.martin-riedl.de/redirect/latest/macos/amd64/snapshot/ffmpeg.zip",
            kind: ArchiveKind::Zip,
        }),
        _ => anyhow::bail!("ffmpeg auto-provisioning is not supported on {os} {arch}"),
    }
}

fn extract_and_install(kind: ArchiveKind, archive_data: &[u8], local: &Path) -> Result<()> {
    match kind {
        ArchiveKind::TarXz => extract_tar_xz_ffmpeg(archive_data, local),
        ArchiveKind::Zip => extract_zip_ffmpeg(archive_data, local),
    }
}

fn extract_tar_xz_ffmpeg(archive_data: &[u8], local: &Path) -> Result<()> {
    let decoder = lzma_rust2::XzReader::new(Cursor::new(archive_data), false);
    let mut archive = tar::Archive::new(decoder);
    let dir = local
        .parent()
        .context("ffmpeg install path has no parent")?;
    let ffplay = dir.join(ffplay_binary_name());
    let mut found_ffmpeg = false;

    for entry in archive
        .entries()
        .context("failed to read ffmpeg tar archive")?
    {
        let mut entry = entry?;
        let path = entry.path()?;
        let destination = if path.ends_with(Path::new("bin").join("ffmpeg")) {
            found_ffmpeg = true;
            Some(local)
        } else if path.ends_with(Path::new("bin").join("ffplay")) {
            Some(ffplay.as_path())
        } else {
            None
        };

        if let Some(destination) = destination {
            let temp = tempfile::NamedTempFile::new()?;
            entry.unpack(temp.path())?;
            artifact::install_executable(temp.path(), destination)?;
        }
    }

    if found_ffmpeg {
        Ok(())
    } else {
        anyhow::bail!("ffmpeg binary not found in downloaded archive")
    }
}

fn extract_zip_ffmpeg(archive_data: &[u8], local: &Path) -> Result<()> {
    let reader = Cursor::new(archive_data);
    let mut archive = zip::ZipArchive::new(reader).context("failed to read ffmpeg zip archive")?;
    let dir = local
        .parent()
        .context("ffmpeg install path has no parent")?;
    let ffplay = dir.join(ffplay_binary_name());
    let mut found_ffmpeg = false;

    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let path = Path::new(file.name());
        let filename = path.file_name().and_then(|n| n.to_str());
        let destination = match filename {
            Some("ffmpeg") | Some("ffmpeg.exe") => {
                found_ffmpeg = true;
                Some(local)
            }
            Some("ffplay") | Some("ffplay.exe") => Some(ffplay.as_path()),
            _ => None,
        };

        if let Some(destination) = destination {
            let temp = tempfile::NamedTempFile::new()?;
            let mut out = std::fs::File::create(temp.path())?;
            io::copy(&mut file, &mut out)?;
            artifact::install_executable(temp.path(), destination)?;
        }
    }

    if found_ffmpeg {
        Ok(())
    } else {
        anyhow::bail!("ffmpeg binary not found in downloaded archive")
    }
}

fn install_from_local_artifact(source: &Path, local: &Path, state_path: &Path) -> Result<()> {
    artifact::install_executable(source, local).with_context(|| {
        format!(
            "failed to copy test ffmpeg artifact from {} to {}",
            source.display(),
            local.display()
        )
    })?;
    write_state(state_path)
}

fn write_state(state_path: &Path) -> Result<()> {
    let state = FfmpegState {
        last_downloaded: Some(now_epoch_secs()),
    };
    std::fs::write(state_path, serde_json::to_string_pretty(&state)?)?;
    Ok(())
}

fn deps_dir() -> Result<PathBuf> {
    crate::paths::yt_deps_dir()
}

fn ffmpeg_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

fn ffplay_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffplay.exe"
    } else {
        "ffplay"
    }
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct FfmpegState {
    last_downloaded: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extract_tar_xz_installs_ffmpeg_and_ffplay_only() {
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join(ffmpeg_binary_name());
        let archive = include_bytes!("../tests/fixtures/executables.tar.xz");

        extract_and_install(ArchiveKind::TarXz, archive, &local).unwrap();

        assert_eq!(std::fs::read(local).unwrap(), b"fixture ffmpeg\n");
        assert_eq!(
            std::fs::read(dir.path().join(ffplay_binary_name())).unwrap(),
            b"fixture ffplay\n"
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn extract_tar_xz_rejects_corrupt_header_without_replacing_binary() {
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join(ffmpeg_binary_name());
        std::fs::write(&local, b"existing").unwrap();
        let mut archive = include_bytes!("../tests/fixtures/executables.tar.xz").to_vec();
        archive[0] ^= 0xff;

        assert!(extract_and_install(ArchiveKind::TarXz, &archive, &local).is_err());

        assert_eq!(std::fs::read(local).unwrap(), b"existing");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn extract_zip_with_root_level_ffmpeg() {
        let dir = tempfile::TempDir::new().unwrap();
        let local = dir.path().join("ffmpeg");

        // Build a zip containing just "ffmpeg" at root (Martin-Riedl layout)
        let mut zip_data = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(Cursor::new(&mut zip_data));
            let options = zip::write::FileOptions::<'_, ()>::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file("ffmpeg", options).unwrap();
            zip.write_all(b"fake ffmpeg binary content").unwrap();
            zip.finish().unwrap();
        }

        extract_zip_ffmpeg(&zip_data, &local).unwrap();

        assert!(local.exists());
        let contents = std::fs::read(&local).unwrap();
        assert_eq!(contents, b"fake ffmpeg binary content");
    }

    #[test]
    fn resolve_download_maps_macos_arm64_to_martin_riedl() {
        let dl = resolve_download("macos", "aarch64").unwrap();
        match dl {
            FfmpegDownload::Direct { url, kind } => {
                assert!(url.contains("macos/arm64/"), "url: {url}");
                assert!(matches!(kind, ArchiveKind::Zip));
            }
            other => panic!("expected Direct, got {other:?}"),
        }
    }

    #[test]
    fn resolve_download_maps_macos_x86_64_to_martin_riedl() {
        let dl = resolve_download("macos", "x86_64").unwrap();
        match dl {
            FfmpegDownload::Direct { url, kind } => {
                assert!(url.contains("macos/amd64/"), "url: {url}");
                assert!(matches!(kind, ArchiveKind::Zip));
            }
            other => panic!("expected Direct, got {other:?}"),
        }
    }

    #[test]
    fn resolve_download_maps_linux_x86_64_to_btbn() {
        let dl = resolve_download("linux", "x86_64").unwrap();
        match dl {
            FfmpegDownload::BtbN { asset_name, kind } => {
                assert!(asset_name.contains("linux64"));
                assert!(matches!(kind, ArchiveKind::TarXz));
            }
            other => panic!("expected BtbN, got {other:?}"),
        }
    }

    #[test]
    fn resolve_download_errors_on_unsupported() {
        assert!(resolve_download("freebsd", "x86_64").is_err());
    }
}
