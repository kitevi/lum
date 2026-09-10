//! Resolve a YouTube video or channel link to its channel RSS feed.
//!
//! Only video and channel links are accepted; playlists are rejected outright
//! (YouTube only offers per-channel feeds). Resolution itself is delegated to
//! `yt-dlp -J --flat-playlist` so lum never scrapes YouTube HTML.

use std::path::Path;

use anyhow::{Context, Result};

/// Accepted link kinds. Anything else is rejected before yt-dlp is invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Video,
    Channel,
}

/// Why a link was rejected without invoking yt-dlp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejected {
    Playlist,
    Unsupported,
}

/// Feed URL carrying every upload type (videos + shorts + live).
pub fn channel_feed_url(channel_id: &str) -> String {
    format!("https://www.youtube.com/feeds/videos.xml?channel_id={channel_id}")
}

/// Long-form-videos-only playlist ID (`UULF` + channel suffix), or `None`
/// when `channel_id` is not a valid channel ID.
pub fn videos_only_playlist_id(channel_id: &str) -> Option<String> {
    if !is_channel_id(channel_id) {
        return None;
    }
    Some(format!("UULF{}", &channel_id[2..]))
}

/// Long-form-videos-only feed URL, or `None` for an invalid channel ID.
pub fn videos_only_feed_url(channel_id: &str) -> Option<String> {
    videos_only_playlist_id(channel_id).map(|playlist_id| {
        format!("https://www.youtube.com/feeds/videos.xml?playlist_id={playlist_id}")
    })
}

/// `UC` + 22 base64url-ish chars (ASCII only, so byte slicing is safe).
pub fn is_channel_id(value: &str) -> bool {
    value.len() == 24
        && value.starts_with("UC")
        && value[2..]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Classify a raw input string without any network access.
pub fn classify(raw: &str) -> Result<LinkKind, Rejected> {
    let url = url::Url::parse(raw).map_err(|_| Rejected::Unsupported)?;
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let is_youtube = host == "youtube.com" || host.ends_with(".youtube.com");
    let is_short = host == "youtu.be";
    let is_nocookie = host == "youtube-nocookie.com" || host.ends_with(".youtube-nocookie.com");
    if !is_youtube && !is_short && !is_nocookie {
        return Err(Rejected::Unsupported);
    }
    // Playlists are rejected even when they smuggle a video id
    // (`watch?v=...&list=...`, `youtu.be/<id>?list=...`).
    if url.path() == "/playlist" || url.query_pairs().any(|(k, _)| k == "list") {
        return Err(Rejected::Playlist);
    }
    if is_short {
        return match url.path_segments() {
            Some(mut segments) => match (segments.next(), segments.next()) {
                (Some(id), None) if !id.is_empty() => Ok(LinkKind::Video),
                _ => Err(Rejected::Unsupported),
            },
            None => Err(Rejected::Unsupported),
        };
    }
    let raw_path = url.path();
    // Trailing slashes do not change the resource, so strip one before matching.
    let path = raw_path.strip_suffix('/').unwrap_or(raw_path);
    if path == "/watch" {
        let has_video = url.query_pairs().any(|(k, v)| k == "v" && !v.is_empty());
        return if has_video {
            Ok(LinkKind::Video)
        } else {
            Err(Rejected::Unsupported)
        };
    }
    for prefix in ["/shorts/", "/live/", "/embed/", "/v/"] {
        if let Some(rest) = path.strip_prefix(prefix)
            && !rest.is_empty()
            && !rest.contains('/')
        {
            return Ok(LinkKind::Video);
        }
    }
    if path.starts_with("/channel/")
        || path.starts_with("/@")
        || path.starts_with("/c/")
        || path.starts_with("/user/")
    {
        return Ok(LinkKind::Channel);
    }
    Err(Rejected::Unsupported)
}

/// Collect distinct, valid channel IDs from yt-dlp `-J` output: the top-level
/// `channel_id` first, then per-entry IDs in order. `null`/`NA`/missing values
/// are ignored (flat listings report `NA` per entry).
pub fn channel_ids_from_json(value: &serde_json::Value) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let mut push = |raw: Option<&str>| {
        if let Some(id) = raw
            && is_channel_id(id)
            && !ids.iter().any(|seen| seen == id)
        {
            ids.push(id.to_string());
        }
    };
    push(value.get("channel_id").and_then(|v| v.as_str()));
    if let Some(entries) = value.get("entries").and_then(|v| v.as_array()) {
        for entry in entries {
            push(entry.get("channel_id").and_then(|v| v.as_str()));
        }
    }
    ids
}

/// Resolve one link to its canonical channel ID via yt-dlp.
pub fn resolve_channel_id(binary: &Path, raw_url: &str) -> Result<String> {
    match classify(raw_url) {
        Ok(_) => {}
        Err(Rejected::Playlist) => anyhow::bail!(
            "playlist links are not supported: {raw_url} (give a video or channel link instead)"
        ),
        Err(Rejected::Unsupported) => anyhow::bail!(
            "unsupported YouTube link: {raw_url} (give a video or channel link instead)"
        ),
    }

    let output = std::process::Command::new(binary)
        .args([
            "--no-download",
            "--no-warnings",
            "--no-progress",
            "--flat-playlist",
            "-J",
            "--playlist-items",
            "1",
            "--",
            raw_url,
        ])
        .output()
        .with_context(|| format!("failed to run yt-dlp to resolve {raw_url}"))?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to resolve channel for {raw_url}: yt-dlp exited with code {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("could not read channel_id from yt-dlp output for {raw_url}"))?;
    let ids = channel_ids_from_json(&parsed);
    match ids.as_slice() {
        [only] => Ok(only.clone()),
        [] => {
            anyhow::bail!("no channel_id found for {raw_url} (unsupported, private, or deleted?)")
        }
        _ => anyhow::bail!(
            "video has multiple owners/creators for {raw_url} (found: {}) — give a direct channel link instead",
            ids.join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CH: &str = "UCOC7Er4tw8VhkdB2D0wz2_A";
    const CH_FEED: &str =
        "https://www.youtube.com/feeds/videos.xml?channel_id=UCOC7Er4tw8VhkdB2D0wz2_A";
    const VF_PL: &str = "UULFOC7Er4tw8VhkdB2D0wz2_A";
    const VF_FEED: &str =
        "https://www.youtube.com/feeds/videos.xml?playlist_id=UULFOC7Er4tw8VhkdB2D0wz2_A";

    #[test]
    fn channel_id_validation_accepts_real_id() {
        assert!(is_channel_id(CH));
    }

    #[test]
    fn channel_id_validation_rejects_junk() {
        for junk in [
            "",
            "NA",
            "UCshort",
            "UCOC7Er4tw8VhkdB2D0wz2_",   // 23 chars
            "UCOC7Er4tw8VhkdB2D0wz2_AB", // 25 chars
            "XCOC7Er4tw8VhkdB2D0wz2_A",  // wrong prefix
            "UC OC7Er4tw8VhkdB2D0wz2_A", // space
        ] {
            assert!(!is_channel_id(junk), "should reject {junk:?}");
        }
    }

    #[test]
    fn channel_feed_url_is_exact() {
        assert_eq!(channel_feed_url(CH), CH_FEED);
    }

    #[test]
    fn videos_only_playlist_id_replaces_uc_with_uulf() {
        assert_eq!(videos_only_playlist_id(CH).as_deref(), Some(VF_PL));
    }

    #[test]
    fn videos_only_playlist_id_rejects_invalid() {
        assert_eq!(videos_only_playlist_id("NA"), None);
        assert_eq!(videos_only_playlist_id(""), None);
    }

    #[test]
    fn videos_only_feed_url_is_exact() {
        assert_eq!(videos_only_feed_url(CH).as_deref(), Some(VF_FEED));
        assert_eq!(videos_only_feed_url("NA"), None);
    }

    #[test]
    fn classify_accepts_video_links() {
        for url in [
            "https://www.youtube.com/watch?v=_aCA6vWGTno",
            "https://youtu.be/_aCA6vWGTno",
            "https://www.youtube.com/shorts/6ssqb1nkSJI",
            "https://www.youtube.com/live/abc123XYZ_-",
            "https://www.youtube.com/embed/_aCA6vWGTno",
            "https://m.youtube.com/watch?v=_aCA6vWGTno",
        ] {
            assert_eq!(classify(url), Ok(LinkKind::Video), "should accept {url}");
        }
    }

    #[test]
    fn classify_accepts_channel_links() {
        for url in [
            "https://www.youtube.com/@phoboukaideimou/videos",
            "https://www.youtube.com/@phoboukaideimou",
            "https://www.youtube.com/channel/UCOC7Er4tw8VhkdB2D0wz2_A",
            "https://www.youtube.com/c/SomeName",
            "https://www.youtube.com/user/SomeName",
        ] {
            assert_eq!(classify(url), Ok(LinkKind::Channel), "should accept {url}");
        }
    }

    #[test]
    fn classify_rejects_playlists_even_with_video_id() {
        for url in [
            "https://www.youtube.com/playlist?list=PL123",
            "https://www.youtube.com/watch?v=_aCA6vWGTno&list=PL123",
            "https://youtu.be/_aCA6vWGTno?list=PL123",
        ] {
            assert_eq!(
                classify(url),
                Err(Rejected::Playlist),
                "should reject {url}"
            );
        }
    }

    #[test]
    fn classify_rejects_non_video_channel_input() {
        for url in [
            "https://example.com/video",
            "UCOC7Er4tw8VhkdB2D0wz2_A",
            "@phoboukaideimou",
            "_aCA6vWGTno",
            "not a url",
            "https://www.youtube.com/",
            "https://www.youtube.com/watch",
            "https://vimeo.com/123",
        ] {
            assert_eq!(
                classify(url),
                Err(Rejected::Unsupported),
                "should reject {url}"
            );
        }
    }

    #[test]
    fn extractor_reads_video_json() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"id":"_aCA6vWGTno","title":"Anatomy Of A Fall","channel_id":"UCOC7Er4tw8VhkdB2D0wz2_A"}"# ,
        )
        .unwrap();
        assert_eq!(channel_ids_from_json(&v), vec![CH.to_string()]);
    }

    #[test]
    fn extractor_ignores_na_entries_in_channel_json() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"id":"UCOC7Er4tw8VhkdB2D0wz2_A","channel_id":"UCOC7Er4tw8VhkdB2D0wz2_A","entries":[{"id":"_aCA6vWGTno","channel_id":null},{"id":"IKU-3IdrWAQ","channel_id":"NA"}]}"# ,
        )
        .unwrap();
        assert_eq!(channel_ids_from_json(&v), vec![CH.to_string()]);
    }

    #[test]
    fn extractor_collects_distinct_ids_in_order() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"channel_id":"UCOC7Er4tw8VhkdB2D0wz2_A","entries":[{"channel_id":"UCOC7Er4tw8VhkdB2D0wz2_A"},{"channel_id":"UCAAAAAAAAAAAAAAAAAAAAAA"}]}"# ,
        )
        .unwrap();
        assert_eq!(
            channel_ids_from_json(&v),
            vec![CH.to_string(), "UCAAAAAAAAAAAAAAAAAAAAAA".to_string()]
        );
    }

    #[test]
    fn extractor_returns_empty_without_ids() {
        let v: serde_json::Value = serde_json::from_str(r#"{"id":"PL123"}"#).unwrap();
        assert!(channel_ids_from_json(&v).is_empty());
    }

    #[test]
    fn classify_accepts_trailing_slash_variants() {
        for url in [
            "https://www.youtube.com/watch/?v=_aCA6vWGTno",
            "https://www.youtube.com/shorts/6ssqb1nkSJI/",
            "https://www.youtube.com/@phoboukaideimou/",
        ] {
            assert!(classify(url).is_ok(), "should accept {url}");
        }
    }

    #[test]
    fn classify_accepts_nocookie_embeds() {
        assert_eq!(
            classify("https://www.youtube-nocookie.com/embed/_aCA6vWGTno"),
            Ok(LinkKind::Video)
        );
    }
}
