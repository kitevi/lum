use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use tempfile::TempDir;

fn lum_with_empty_path(home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("PATH", ""); // ensure yt-dlp and ffmpeg are not found on PATH
    cmd
}

/// Create a fake yt-dlp binary on PATH so we can test ffmpeg check.
fn lum_with_fake_ytdlp(home: &TempDir) -> Command {
    let bin_dir = home.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();

    // Write a script that exits 0 so `which` finds it
    let yt_dlp_path = bin_dir.join("yt-dlp");
    #[cfg(unix)]
    {
        std::fs::write(&yt_dlp_path, "#!/bin/sh\nexit 0\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&yt_dlp_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("PATH", &bin_dir);
    cmd
}

fn lum_with_fake_ffmpeg_and_ytdlp_artifact(home: &TempDir) -> Command {
    let bin_dir = home.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();

    let ffmpeg_path = bin_dir.join("ffmpeg");
    let ytdlp_artifact = home.path().join("yt-dlp-artifact");
    #[cfg(unix)]
    {
        std::fs::write(&ffmpeg_path, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::write(&ytdlp_artifact, "#!/bin/sh\nexit 0\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&ffmpeg_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&ytdlp_artifact, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("PATH", &bin_dir)
        .env("LUM_YT_DLP_TEST_ARTIFACT", &ytdlp_artifact);
    cmd
}

fn lum_with_fake_ytdlp_and_ffmpeg_artifact(home: &TempDir) -> Command {
    let bin_dir = home.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();

    let yt_dlp_path = bin_dir.join("yt-dlp");
    let ffmpeg_artifact = home.path().join("ffmpeg-artifact");
    #[cfg(unix)]
    {
        std::fs::write(&yt_dlp_path, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::write(&ffmpeg_artifact, "#!/bin/sh\nexit 0\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&yt_dlp_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&ffmpeg_artifact, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("PATH", &bin_dir)
        .env("LUM_FFMPEG_TEST_ARTIFACT", &ffmpeg_artifact);
    cmd
}

fn lum_with_fake_ytdlp_and_ffmpeg(home: &TempDir) -> Command {
    let bin_dir = home.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();

    let yt_dlp_path = bin_dir.join("yt-dlp");
    let ffmpeg_path = bin_dir.join("ffmpeg");
    #[cfg(unix)]
    {
        std::fs::write(&yt_dlp_path, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::write(&ffmpeg_path, "#!/bin/sh\nexit 0\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&yt_dlp_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&ffmpeg_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("PATH", &bin_dir);
    cmd
}

fn write_provisioned_ffmpeg(home: &TempDir, contents: &str, last_downloaded: u64) {
    let deps_dir = home.path().join("data").join("lum").join("deps");
    std::fs::create_dir_all(&deps_dir).unwrap();

    let ffmpeg = deps_dir.join("ffmpeg");
    std::fs::write(&ffmpeg, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&ffmpeg, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let state = serde_json::json!({ "last_downloaded": last_downloaded });
    std::fs::write(
        deps_dir.join("ffmpeg.json"),
        serde_json::to_string_pretty(&state).unwrap(),
    )
    .unwrap();
}

fn provisioned_ffmpeg_path(home: &TempDir) -> std::path::PathBuf {
    home.path()
        .join("data")
        .join("lum")
        .join("deps")
        .join("ffmpeg")
}

#[test]
fn yt_aud_auto_provisions_yt_dlp_when_missing() {
    let home = TempDir::new().unwrap();

    lum_with_fake_ffmpeg_and_ytdlp_artifact(&home)
        .args(["yt", "aud", "https://example.com/video"])
        .assert()
        .success();

    let deps_ytdlp = home
        .path()
        .join("data")
        .join("lum")
        .join("deps")
        .join("yt-dlp");
    let deps_state = home
        .path()
        .join("data")
        .join("lum")
        .join("deps")
        .join("yt-dlp.json");
    assert!(deps_ytdlp.exists());
    assert!(deps_state.exists());
}

#[test]
fn yt_vid_auto_provisions_ffmpeg_when_missing() {
    let home = TempDir::new().unwrap();

    lum_with_fake_ytdlp_and_ffmpeg_artifact(&home)
        .args(["yt", "vid", "https://example.com/video"])
        .assert()
        .success();

    let deps_ffmpeg = home
        .path()
        .join("data")
        .join("lum")
        .join("deps")
        .join("ffmpeg");
    let deps_state = home
        .path()
        .join("data")
        .join("lum")
        .join("deps")
        .join("ffmpeg.json");
    assert!(deps_ffmpeg.exists());
    assert!(deps_state.exists());
}

#[test]
fn yt_vid_uses_ffmpeg_from_path_without_provisioning() {
    let home = TempDir::new().unwrap();

    lum_with_fake_ytdlp_and_ffmpeg(&home)
        .args(["yt", "vid", "https://example.com/video"])
        .assert()
        .success();

    let deps_ffmpeg = home
        .path()
        .join("data")
        .join("lum")
        .join("deps")
        .join("ffmpeg");
    assert!(!deps_ffmpeg.exists());
}

#[test]
fn yt_vid_reuses_fresh_provisioned_ffmpeg() {
    let home = TempDir::new().unwrap();
    let recent_epoch_secs = 4_102_444_800; // 2100-01-01T00:00:00Z
    write_provisioned_ffmpeg(&home, "#!/bin/sh\nexit 0\n", recent_epoch_secs);

    lum_with_fake_ytdlp_and_ffmpeg_artifact(&home)
        .args(["yt", "vid", "https://example.com/video"])
        .assert()
        .success();

    let installed = std::fs::read_to_string(provisioned_ffmpeg_path(&home)).unwrap();
    assert_eq!(installed, "#!/bin/sh\nexit 0\n");
}

#[test]
fn yt_vid_refreshes_stale_provisioned_ffmpeg() {
    let home = TempDir::new().unwrap();
    write_provisioned_ffmpeg(&home, "#!/bin/sh\nexit 1\n", 0);

    lum_with_fake_ytdlp_and_ffmpeg_artifact(&home)
        .args(["yt", "vid", "https://example.com/video"])
        .assert()
        .success();

    let installed = std::fs::read_to_string(provisioned_ffmpeg_path(&home)).unwrap();
    assert_eq!(installed, "#!/bin/sh\nexit 0\n");
}

#[test]
fn yt_aud_requires_at_least_one_url() {
    let home = TempDir::new().unwrap();

    lum_with_empty_path(&home)
        .args(["yt", "aud"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("required"));
}

#[test]
fn yt_vid_requires_at_least_one_url() {
    let home = TempDir::new().unwrap();

    lum_with_empty_path(&home)
        .args(["yt", "vid"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("required"));
}

#[test]
fn yt_alb_requires_at_least_one_url() {
    let home = TempDir::new().unwrap();

    lum_with_empty_path(&home)
        .args(["yt", "alb"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("required"));
}

#[test]
fn yt_rejects_unknown_subcommand() {
    let home = TempDir::new().unwrap();

    lum_with_empty_path(&home)
        .args(["yt", "download"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("download").or(predicates::str::contains("subcommand")));
}

#[test]
fn yt_aud_does_not_require_ffmpeg() {
    let home = TempDir::new().unwrap();

    // yt-dlp is available (fake), but ffmpeg is not. Audio should still run.
    lum_with_fake_ytdlp(&home)
        .args(["yt", "aud", "https://example.com/video"])
        .assert()
        .success();
}

#[test]
fn yt_vid_fails_when_ffmpeg_not_found() {
    let home = TempDir::new().unwrap();

    // yt-dlp is available (fake), but ffmpeg is not. Video needs ffmpeg for muxing.
    lum_with_fake_ytdlp(&home)
        .env("LUM_FFMPEG_DISABLE_AUTO_PROVISION", "1")
        .args(["yt", "vid", "https://example.com/video"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("ffmpeg"));
}

#[test]
fn yt_vid_accepts_height_flag() {
    let home = TempDir::new().unwrap();

    lum_with_fake_ytdlp(&home)
        .env("LUM_FFMPEG_DISABLE_AUTO_PROVISION", "1")
        .args(["yt", "vid", "--height", "2160", "https://example.com/video"])
        .assert()
        .failure()
        // Should fail because ffmpeg missing, not because --height is invalid
        .stderr(predicates::str::contains("ffmpeg"));
}

// --- `lum yt rss` ---

const RSS_TEST_CHANNEL_URL: &str = "https://www.youtube.com/@phoboukaideimou/videos";
const RSS_TEST_VIDEO_URL: &str = "https://www.youtube.com/watch?v=_aCA6vWGTno";
const RSS_TEST_CHANNEL_FEED: &str =
    "https://www.youtube.com/feeds/videos.xml?channel_id=UCOC7Er4tw8VhkdB2D0wz2_A\n";
const RSS_TEST_VIDEOS_ONLY_FEED: &str =
    "https://www.youtube.com/feeds/videos.xml?playlist_id=UULFOC7Er4tw8VhkdB2D0wz2_A\n";

/// Fake yt-dlp that prints a canned yt-dlp `-J` payload given as `body`,
/// ignoring its arguments.
fn lum_with_canned_ytdlp(home: &TempDir, body: &str) -> Command {
    let bin_dir = home.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();

    let yt_dlp_path = bin_dir.join("yt-dlp");
    #[cfg(unix)]
    {
        std::fs::write(&yt_dlp_path, format!("#!/bin/sh\nprintf '%s\n' '{body}'\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&yt_dlp_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("PATH", &bin_dir);
    cmd
}

#[test]
fn yt_rss_requires_at_least_one_url() {
    let home = TempDir::new().unwrap();

    lum_with_empty_path(&home)
        .args(["yt", "rss"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("required"));
}

#[test]
fn yt_rss_prints_channel_feed_for_channel_url() {
    let home = TempDir::new().unwrap();

    lum_with_canned_ytdlp(&home, "{\"id\":\"UCOC7Er4tw8VhkdB2D0wz2_A\",\"channel_id\":\"UCOC7Er4tw8VhkdB2D0wz2_A\",\"entries\":[{\"id\":\"_aCA6vWGTno\",\"channel_id\":null},{\"id\":\"IKU-3IdrWAQ\",\"channel_id\":\"NA\"}]}\n")
        .args(["yt", "rss", RSS_TEST_CHANNEL_URL])
        .assert()
        .success()
        .stdout(RSS_TEST_CHANNEL_FEED);
}

#[test]
fn yt_rss_resolves_video_url_to_same_feed() {
    let home = TempDir::new().unwrap();

    lum_with_canned_ytdlp(
        &home,
        "{\"id\":\"_aCA6vWGTno\",\"channel_id\":\"UCOC7Er4tw8VhkdB2D0wz2_A\"}\n",
    )
    .args(["yt", "rss", RSS_TEST_VIDEO_URL])
    .assert()
    .success()
    .stdout(RSS_TEST_CHANNEL_FEED);
}

#[test]
fn yt_rss_id_only_prints_bare_id() {
    let home = TempDir::new().unwrap();

    lum_with_canned_ytdlp(
        &home,
        "{\"id\":\"_aCA6vWGTno\",\"channel_id\":\"UCOC7Er4tw8VhkdB2D0wz2_A\"}\n",
    )
    .args(["yt", "rss", "--id-only", RSS_TEST_CHANNEL_URL])
    .assert()
    .success()
    .stdout("UCOC7Er4tw8VhkdB2D0wz2_A\n");
}

#[test]
fn yt_rss_videos_only_prints_playlist_feed() {
    let home = TempDir::new().unwrap();

    lum_with_canned_ytdlp(
        &home,
        "{\"id\":\"_aCA6vWGTno\",\"channel_id\":\"UCOC7Er4tw8VhkdB2D0wz2_A\"}\n",
    )
    .args(["yt", "rss", "--videos-only", RSS_TEST_CHANNEL_URL])
    .assert()
    .success()
    .stdout(RSS_TEST_VIDEOS_ONLY_FEED);
}

#[test]
fn yt_rss_videos_only_id_only_prints_playlist_id() {
    let home = TempDir::new().unwrap();

    lum_with_canned_ytdlp(
        &home,
        "{\"id\":\"_aCA6vWGTno\",\"channel_id\":\"UCOC7Er4tw8VhkdB2D0wz2_A\"}\n",
    )
    .args([
        "yt",
        "rss",
        "--videos-only",
        "--id-only",
        RSS_TEST_CHANNEL_URL,
    ])
    .assert()
    .success()
    .stdout("UULFOC7Er4tw8VhkdB2D0wz2_A\n");
}

/// Fake yt-dlp that appends every argument it receives to `log_file`, then
/// prints canned channel IDs (one per `\n` in `body`).
fn lum_with_logging_ytdlp(home: &TempDir, log_file: &std::path::Path, body: &str) -> Command {
    let bin_dir = home.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();

    let yt_dlp_path = bin_dir.join("yt-dlp");
    #[cfg(unix)]
    {
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nprintf '%s\n' '{}'\n",
            log_file.display(),
            body
        );
        std::fs::write(&yt_dlp_path, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&yt_dlp_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("PATH", &bin_dir);
    cmd
}

#[test]
fn yt_rss_rejects_playlist_without_calling_ytdlp() {
    let home = TempDir::new().unwrap();
    let log = home.path().join("ytdlp.log");

    lum_with_logging_ytdlp(
        &home,
        &log,
        "{\"id\":\"_aCA6vWGTno\",\"channel_id\":\"UCOC7Er4tw8VhkdB2D0wz2_A\"}\n",
    )
    .args(["yt", "rss", "https://www.youtube.com/playlist?list=PL123"])
    .assert()
    .failure()
    .stdout(predicates::str::is_empty())
    .stderr(predicates::str::contains(
        "playlist links are not supported",
    ));
    assert!(!log.exists(), "yt-dlp must not be invoked for playlists");
}

#[test]
fn yt_rss_rejects_watch_with_list_as_playlist() {
    let home = TempDir::new().unwrap();
    let log = home.path().join("ytdlp.log");

    lum_with_logging_ytdlp(
        &home,
        &log,
        "{\"id\":\"_aCA6vWGTno\",\"channel_id\":\"UCOC7Er4tw8VhkdB2D0wz2_A\"}\n",
    )
    .args([
        "yt",
        "rss",
        "https://www.youtube.com/watch?v=_aCA6vWGTno&list=PL123",
    ])
    .assert()
    .failure()
    .stderr(predicates::str::contains(
        "playlist links are not supported",
    ));
    assert!(!log.exists(), "yt-dlp must not be invoked for playlists");
}

#[test]
fn yt_rss_rejects_non_youtube_link() {
    let home = TempDir::new().unwrap();
    let log = home.path().join("ytdlp.log");

    lum_with_logging_ytdlp(
        &home,
        &log,
        "{\"id\":\"_aCA6vWGTno\",\"channel_id\":\"UCOC7Er4tw8VhkdB2D0wz2_A\"}\n",
    )
    .args(["yt", "rss", "https://example.com/video"])
    .assert()
    .failure()
    .stdout(predicates::str::is_empty())
    .stderr(predicates::str::contains("unsupported YouTube link"));
    assert!(!log.exists(), "yt-dlp must not be invoked for bad links");
}

#[test]
fn yt_rss_rejects_multi_owner_video() {
    let home = TempDir::new().unwrap();

    lum_with_canned_ytdlp(
        &home,
        "{\"channel_id\":\"UCOC7Er4tw8VhkdB2D0wz2_A\",\"entries\":[{\"channel_id\":\"UCOC7Er4tw8VhkdB2D0wz2_A\"},{\"channel_id\":\"UCAAAAAAAAAAAAAAAAAAAAAA\"}]}\n",
    )
    .args(["yt", "rss", RSS_TEST_VIDEO_URL])
    .assert()
    .failure()
    .stderr(
        predicates::str::contains("multiple owners/creators")
            .and(predicates::str::contains("direct channel link")),
    );
}

#[test]
fn yt_rss_fails_fast_and_stops_at_first_failure() {
    let home = TempDir::new().unwrap();
    let log = home.path().join("ytdlp.log");
    let good1 = "https://www.youtube.com/watch?v=goodvideo00001";
    let bad = "https://www.youtube.com/playlist?list=PL123";
    let good3 = "https://www.youtube.com/watch?v=goodvideo00003";

    lum_with_logging_ytdlp(
        &home,
        &log,
        "{\"id\":\"_aCA6vWGTno\",\"channel_id\":\"UCOC7Er4tw8VhkdB2D0wz2_A\"}\n",
    )
    .args(["yt", "rss", good1, bad, good3])
    .assert()
    .failure()
    .stdout(RSS_TEST_CHANNEL_FEED)
    .stderr(predicates::str::contains(
        "playlist links are not supported",
    ));

    let logged = std::fs::read_to_string(&log).unwrap();
    assert!(logged.contains(good1), "first URL must be resolved");
    assert!(
        !logged.contains(good3),
        "fail-fast: third URL never resolved"
    );
    assert!(!logged.contains("list=PL123"), "playlist never resolved");
}
/// Fake yt-dlp that writes `message` to stderr and exits with `code`.
fn lum_with_failing_ytdlp(home: &TempDir, code: i32, message: &str) -> Command {
    let bin_dir = home.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();

    let yt_dlp_path = bin_dir.join("yt-dlp");
    #[cfg(unix)]
    {
        let script = format!("#!/bin/sh\necho {message} >&2\nexit {code}\n");
        std::fs::write(&yt_dlp_path, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&yt_dlp_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("PATH", &bin_dir);
    cmd
}

#[test]
fn yt_rss_reports_ytdlp_failure() {
    let home = TempDir::new().unwrap();

    lum_with_failing_ytdlp(&home, 3, "channel_tab_unavailable")
        .args(["yt", "rss", RSS_TEST_CHANNEL_URL])
        .assert()
        .failure()
        .stdout(predicates::str::is_empty())
        .stderr(
            predicates::str::contains("yt-dlp exited with code")
                .and(predicates::str::contains("channel_tab_unavailable")),
        );
}

#[test]
fn yt_rss_reports_unparseable_output() {
    let home = TempDir::new().unwrap();

    lum_with_canned_ytdlp(&home, "not json\n")
        .args(["yt", "rss", RSS_TEST_VIDEO_URL])
        .assert()
        .failure()
        .stdout(predicates::str::is_empty())
        .stderr(predicates::str::contains("could not read channel_id"));
}

/// Live contract test against real yt-dlp + YouTube (ignored by default).
/// Pins the undocumented feed-URL conventions. Requires network + yt-dlp:
/// `cargo test -- --ignored yt_rss_live`.
#[test]
#[ignore]
fn yt_rss_live_resolves_both_feeds() {
    let expected = format!("{RSS_TEST_CHANNEL_FEED}{RSS_TEST_CHANNEL_FEED}");
    Command::cargo_bin("lum")
        .unwrap()
        .args(["yt", "rss", RSS_TEST_CHANNEL_URL, RSS_TEST_VIDEO_URL])
        .assert()
        .success()
        .stdout(expected);
}
