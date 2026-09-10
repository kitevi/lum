pub mod args;
mod deps;
pub mod rss;

pub(crate) use deps::resolve_yt_dlp;

use std::path::Path;

use anyhow::{Context, Result};

use crate::cli::YtCommand;
use crate::ffmpeg;

pub async fn run(command: YtCommand) -> Result<()> {
    let yt_dlp = deps::resolve_yt_dlp().await?;

    match command {
        YtCommand::Aud { urls } => {
            let args = args::audio_args();
            let dest_dir = output_dirs::audio_dir();
            run_yt_dlp(&yt_dlp, &args, &dest_dir, &urls)
        }
        YtCommand::Vid { height, urls } => {
            check_ffmpeg().await?;
            let args = args::video_args(height);
            let dest_dir = output_dirs::video_dir();
            run_yt_dlp(&yt_dlp, &args, &dest_dir, &urls)
        }
        YtCommand::Alb { urls } => {
            let args = args::album_args(&urls);
            let dest_dir = output_dirs::audio_dir();
            run_yt_dlp(&yt_dlp, &args, &dest_dir, &urls)
        }
        YtCommand::Rss {
            id_only,
            videos_only,
            urls,
        } => {
            for raw_url in &urls {
                let channel_id = rss::resolve_channel_id(&yt_dlp, raw_url)?;
                // NOTE: resolve_channel_id only returns validated IDs, so the None
                // cases below are defensive (reachable only if validation changes).
                let line = match (videos_only, id_only) {
                    (true, true) => {
                        rss::videos_only_playlist_id(&channel_id).with_context(|| {
                            format!("invalid channel_id resolved for {raw_url}: {channel_id}")
                        })?
                    }
                    (true, false) => rss::videos_only_feed_url(&channel_id).with_context(|| {
                        format!("invalid channel_id resolved for {raw_url}: {channel_id}")
                    })?,
                    (false, true) => channel_id,
                    (false, false) => rss::channel_feed_url(&channel_id),
                };
                println!("{line}");
            }
            Ok(())
        }
    }
}

async fn check_ffmpeg() -> Result<()> {
    ffmpeg::resolve().await.map(|_| ()).map_err(|error| {
        anyhow::anyhow!(
            "ffmpeg is not available: {error}\n\nInstall it manually or let lum auto-provision it on Linux/Windows."
        )
    })
}

fn run_yt_dlp(
    binary: &Path,
    extra_args: &[String],
    dest_dir: &Path,
    urls: &[String],
) -> Result<()> {
    std::fs::create_dir_all(dest_dir)?;

    let mut cmd = std::process::Command::new(binary);
    cmd.args(extra_args).arg("-P").arg(dest_dir).args(urls);

    // Pass through stdio so yt-dlp owns the terminal experience
    use std::process::Stdio;
    cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());

    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("yt-dlp exited with code {:?}", status.code());
    }
    Ok(())
}

mod output_dirs {
    use std::path::PathBuf;

    pub fn audio_dir() -> PathBuf {
        if let Some(dirs) = directories::UserDirs::new()
            && let Some(audio) = dirs.audio_dir()
        {
            return audio.to_path_buf();
        }
        crate::paths::home_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("Music")
    }

    pub fn video_dir() -> PathBuf {
        if let Some(dirs) = directories::UserDirs::new()
            && let Some(video) = dirs.video_dir()
        {
            return video.to_path_buf();
        }
        crate::paths::home_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("Movies")
    }
}
