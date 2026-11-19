/// yt-dlp argument construction for each subcommand.
pub fn audio_args() -> Vec<String> {
    let mut args = base_args();
    args.extend(audio_specific_args());
    args
}

pub fn video_args(height: u32) -> Vec<String> {
    let mut args = base_args();
    args.extend(video_specific_args(height));
    args
}

pub fn album_args(urls: &[String]) -> Vec<String> {
    let mut args = base_args();
    args.extend(album_specific_args(urls));
    args
}

fn base_args() -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--geo-bypass".into(),
        "--no-cache-dir".into(),
        "--no-restrict-filenames".into(),
        "--concurrent-fragments".into(),
        "8".into(),
    ];

    // Title cleanup regexes
    let regexes: &[(&str, &str, &str)] = &[
        // Strip "Official Video", "Music Video", "Lyric Video", etc.
        (
            "title",
            r"(?i)\s*[\(\[]?(official\s+(music\s+)?video|music\s+video|lyric(s?\s+)?video|lyrical\s+video|official\s+audio|visualizer|audio\s+only|full\s+video|full\s+song)\s*[\)\]]?\s*$",
            "",
        ),
        // Strip resolution tags like [1080p], (4K)
        (
            "title",
            r"(?i)\s*[\(\[][\s\w]*(hd|4k|uhd|2160p|1440p|1080p|720p|480p|360p)[\)\]]\s*",
            "",
        ),
        // Strip standalone "Official" suffix
        ("title", r"(?i)\s*\(?official\)?\s*$", ""),
        // Strip "Remastered" / "Remastered 2024" suffix
        ("title", r"(?i)\s*\(?remastered\s*(\d{4})?\)?\s*$", ""),
        // Normalize "feat." from brackets
        (
            "title",
            r"(?i)\s*[\(\[](feat[.]?\s+[^)\]]+)[\)\]]\s*",
            " (feat. $1)",
        ),
        // Collapse double dashes/underscores
        ("title", r"[\-_]{2,}", " "),
        // Trim leading/trailing whitespace/dashes
        ("title", r"^[\s\-_]+|[\s\-_]+$", ""),
    ];

    for (field, pattern, replacement) in regexes {
        args.push("--replace-in-metadata".into());
        args.push((*field).into());
        args.push((*pattern).into());
        args.push((*replacement).into());
    }

    args
}

fn audio_specific_args() -> Vec<String> {
    vec![
        "-o".into(),
        "%(title)s [%(abr)s].%(ext)s".into(),
        "-f".into(),
        "bestaudio/best".into(),
        "--no-playlist".into(),
    ]
}

fn video_specific_args(h: u32) -> Vec<String> {
    let args = vec![
        "--sub-langs".into(),
        "en.*".into(),
        "--write-subs".into(),
        "--no-playlist".into(),
        "-o".into(),
        "%(title)s (%(upload_date>%Y-%m-%d)s) [%(height)sp %(id)s].%(ext)s".into(),
        "-f".into(),
        format!("bestvideo[height<={h}]+bestaudio/best[height<={h}]/best"),
    ];
    args
}

fn album_specific_args(urls: &[String]) -> Vec<String> {
    // Bandcamp exposes the artist as the per-entry `album_artist`/`artist`, so we
    // can build an "Artist - Album" folder. YouTube's per-entry `uploader` is the
    // channel name (not the artist), so for YouTube we keep the bare album title.
    let folder = if has_bandcamp(urls) {
        "%(album_artist,artist|Unknown Artist)s - %(playlist_title)s"
    } else {
        "%(playlist_title)s"
    };
    vec![
        "-o".into(),
        format!("{folder}/%(autonumber)s - %(title)s [%(abr)s].%(ext)s"),
        "-f".into(),
        "bestaudio/best".into(),
        "--yes-playlist".into(),
    ]
}

/// True if any URL targets Bandcamp, where the per-entry uploader/artist is the
/// real artist (not a channel name).
fn has_bandcamp(urls: &[String]) -> bool {
    urls.iter().any(|u| u.contains("bandcamp.com"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_args_contains_base_flags() {
        let args = audio_args();
        assert!(args.contains(&"--geo-bypass".to_string()));
        assert!(args.contains(&"--no-cache-dir".to_string()));
        assert!(args.contains(&"--no-restrict-filenames".to_string()));
        assert!(args.contains(&"--concurrent-fragments".to_string()));
        assert!(args.contains(&"8".to_string()));
    }

    #[test]
    fn audio_args_selects_bestaudio() {
        let args = audio_args();
        let f_idx = args.iter().position(|a| a == "-f").unwrap();
        assert_eq!(args[f_idx + 1], "bestaudio/best");
    }

    #[test]
    fn audio_args_disables_playlist() {
        let args = audio_args();
        assert!(args.contains(&"--no-playlist".to_string()));
    }

    #[test]
    fn audio_args_has_title_template() {
        let args = audio_args();
        let o_idx = args.iter().position(|a| a == "-o").unwrap();
        assert!(args[o_idx + 1].contains("%(title)s"));
        assert!(args[o_idx + 1].contains("%(abr)s"));
    }

    #[test]
    fn album_args_enables_playlist() {
        let args = album_args(&[]);
        assert!(args.contains(&"--yes-playlist".to_string()));
    }

    #[test]
    fn album_args_selects_bestaudio() {
        let args = album_args(&[]);
        let f_idx = args.iter().position(|a| a == "-f").unwrap();
        assert_eq!(args[f_idx + 1], "bestaudio/best");
    }

    #[test]
    fn album_args_has_album_template() {
        let args = album_args(&[]);
        let o_idx = args.iter().position(|a| a == "-o").unwrap();
        let template = &args[o_idx + 1];
        assert!(template.contains("%(playlist_title)s"));
        assert!(template.contains("%(autonumber)s"));
    }

    #[test]
    fn album_args_bandcamp_prefixes_folder_with_artist() {
        let urls = vec!["https://kylepreston.bandcamp.com/album/foo".to_string()];
        let args = album_args(&urls);
        let o_idx = args.iter().position(|a| a == "-o").unwrap();
        let template = &args[o_idx + 1];
        assert!(
            template.starts_with("%(album_artist,artist|Unknown Artist)s - %(playlist_title)s/"),
            "bandcamp template should prefix the folder with the artist, got: {template}"
        );
    }

    #[test]
    fn album_args_youtube_omits_artist_prefix() {
        let urls = vec!["https://www.youtube.com/playlist?list=PLx".to_string()];
        let args = album_args(&urls);
        let o_idx = args.iter().position(|a| a == "-o").unwrap();
        let template = &args[o_idx + 1];
        assert!(
            template.starts_with("%(playlist_title)s/"),
            "youtube template should keep the bare album title, got: {template}"
        );
    }

    #[test]
    fn album_args_mixed_urls_treat_as_bandcamp() {
        // One yt-dlp invocation uses one template; if any URL is Bandcamp we
        // prefer the artist-prefixed folder so Bandcamp albums stay correct.
        let urls = vec![
            "https://www.youtube.com/playlist?list=PLx".to_string(),
            "https://x.bandcamp.com/album/y".to_string(),
        ];
        let args = album_args(&urls);
        let o_idx = args.iter().position(|a| a == "-o").unwrap();
        assert!(args[o_idx + 1].contains("%(album_artist,artist|Unknown Artist)s"));
    }

    #[test]
    fn video_args_cap_height() {
        let args = video_args(1080);
        let f_idx = args.iter().position(|a| a == "-f").unwrap();
        let format = &args[f_idx + 1];
        assert!(format.contains("height<=1080"));
    }

    #[test]
    fn video_args_custom_height() {
        let args = video_args(2160);
        let f_idx = args.iter().position(|a| a == "-f").unwrap();
        let format = &args[f_idx + 1];
        assert!(format.contains("height<=2160"));
        assert!(!format.contains("height<=1080"));
    }

    #[test]
    fn video_args_includes_subtitles() {
        let args = video_args(1080);
        assert!(args.contains(&"--sub-langs".to_string()));
        assert!(args.contains(&"--write-subs".to_string()));
    }

    #[test]
    fn video_args_disables_playlist() {
        let args = video_args(1080);
        assert!(args.contains(&"--no-playlist".to_string()));
    }

    #[test]
    fn base_args_include_metadata_cleanup() {
        let args = audio_args();
        let rim_count = args
            .iter()
            .filter(|a| *a == "--replace-in-metadata")
            .count();
        assert_eq!(
            rim_count, 7,
            "expected 7 --replace-in-metadata entries, got {rim_count}"
        );
    }
}
