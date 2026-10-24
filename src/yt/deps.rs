use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::artifact;
const STALE_THRESHOLD: Duration = Duration::from_secs(24 * 60 * 60);
const TEST_ARTIFACT_ENV: &str = "LUM_YT_DLP_TEST_ARTIFACT";

/// Resolve the yt-dlp binary to use.
///
/// 1. If yt-dlp is on $PATH, use that.
/// 2. Otherwise, use the auto-provisioned copy at `data_dir()/deps/yt-dlp`.
///    If the auto-provisioned copy doesn't exist yet, download it.
///    If it exists but is stale (>24h since last check), re-check and update.
pub fn resolve_yt_dlp() -> Result<PathBuf> {
    // Check PATH first
    if let Ok(path) = which::which("yt-dlp") {
        tracing::debug!("using yt-dlp from PATH: {}", path.display());
        return Ok(path);
    }

    // Check auto-provisioned copy
    let dir = deps_dir()?;
    let local = dir.join(yt_dlp_binary_name());
    let state_path = dir.join("yt-dlp.json");

    if local.exists() {
        if is_stale(&state_path) {
            tracing::info!("auto-provisioned yt-dlp is stale, checking for update");
            if let Err(e) = try_update(&local, &state_path) {
                tracing::warn!("failed to update yt-dlp: {e:#}; using cached copy");
            }
        }
        return Ok(local);
    }

    // Download for the first time
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create deps dir {}", dir.display()))?;
    if let Ok(test_artifact) = std::env::var(TEST_ARTIFACT_ENV) {
        install_from_local_artifact(Path::new(&test_artifact), &local, &state_path)?;
    } else {
        download_yt_dlp(&local, &state_path)?;
    }
    Ok(local)
}

fn is_stale(state_path: &Path) -> bool {
    let data = match std::fs::read_to_string(state_path) {
        Ok(d) => d,
        Err(_) => return true,
    };
    let state: DepState = match serde_json::from_str(&data) {
        Ok(s) => s,
        Err(_) => return true,
    };
    let last_checked = match state.last_checked {
        Some(ts) => SystemTime::UNIX_EPOCH + Duration::from_secs(ts),
        None => return true,
    };
    SystemTime::now()
        .duration_since(last_checked)
        .unwrap_or(Duration::ZERO)
        > STALE_THRESHOLD
}

fn try_update(local: &Path, state_path: &Path) -> Result<()> {
    let current_state: DepState = std::fs::read_to_string(state_path)
        .ok()
        .and_then(|d| serde_json::from_str(&d).ok())
        .unwrap_or_default();

    let latest = fetch_latest_release_info()?;
    if latest.version == current_state.version {
        // Same version — just update the timestamp
        let updated = DepState {
            version: current_state.version,
            last_checked: Some(now_epoch_secs()),
        };
        std::fs::write(state_path, serde_json::to_string_pretty(&updated)?)?;
        return Ok(());
    }

    tracing::info!(
        "updating yt-dlp {} → {}",
        current_state.version,
        latest.version
    );
    let url = &latest.download_url;
    let binary_data = crate::github::download_bytes(url)?;
    artifact::write_executable(local, &binary_data)?;

    let state = DepState {
        version: latest.version,
        last_checked: Some(now_epoch_secs()),
    };
    std::fs::write(state_path, serde_json::to_string_pretty(&state)?)?;
    Ok(())
}

fn download_yt_dlp(local: &Path, state_path: &Path) -> Result<()> {
    let info = fetch_latest_release_info()?;
    let version = info.version.clone();
    tracing::info!("downloading yt-dlp {version}");

    let url = info.download_url.as_str();
    let binary_data = crate::github::download_bytes(url)?;
    artifact::write_executable(local, &binary_data)?;

    let state = DepState {
        version: info.version,
        last_checked: Some(now_epoch_secs()),
    };
    std::fs::write(state_path, serde_json::to_string_pretty(&state)?)?;

    tracing::info!("yt-dlp {version} installed to {}", local.display());
    Ok(())
}

fn install_from_local_artifact(source: &Path, local: &Path, state_path: &Path) -> Result<()> {
    artifact::install_executable(source, local).with_context(|| {
        format!(
            "failed to copy test yt-dlp artifact from {} to {}",
            source.display(),
            local.display()
        )
    })?;
    let state = DepState {
        version: "test".into(),
        last_checked: Some(now_epoch_secs()),
    };
    std::fs::write(state_path, serde_json::to_string_pretty(&state)?)?;
    Ok(())
}

struct ReleaseInfo {
    version: String,
    download_url: String,
}

fn fetch_latest_release_info() -> Result<ReleaseInfo> {
    let release = crate::github::latest_release("yt-dlp", "yt-dlp")?;
    Ok(ReleaseInfo {
        download_url: release.asset_url(yt_dlp_asset_name())?.to_owned(),
        version: release
            .tag_name
            .context("GitHub release missing tag_name")?,
    })
}

fn yt_dlp_asset_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else {
        "yt-dlp"
    }
}

pub(crate) fn deps_dir() -> Result<PathBuf> {
    crate::paths::yt_deps_dir()
}

fn yt_dlp_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DepState {
    version: String,
    last_checked: Option<u64>,
}
