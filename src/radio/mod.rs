mod player;
pub mod stations;

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use rand::RngExt;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::cli::RadioArgs;
use crate::paths;
use crate::yt::resolve_yt_dlp;
use player::ExternalPlayer;
use stations::{Station, StationKind};

const COMMAND_HELP: &str = "Commands:\n  lum radio <code>  play a station (example: lum radio atma)\n  lum radio rand    play a random station, never the current one\n  lum radio status  show playback state\n  lum radio stop    stop playback";

/// How a play intent was formed. `Explicit` names a station code; `Random`
/// defers the choice to runtime selection over the station catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Explicit(String),
    Random,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RadioCommand {
    List,
    Status,
    Stop,
    Play { selection: Selection },
}

pub fn parse_command(args: &RadioArgs) -> RadioCommand {
    match args.arg.as_deref() {
        None | Some("list") => RadioCommand::List,
        Some("status") => RadioCommand::Status,
        Some("stop") => RadioCommand::Stop,
        Some(stations::RANDOM_CODE) => RadioCommand::Play {
            selection: Selection::Random,
        },
        Some(code) => RadioCommand::Play {
            selection: Selection::Explicit(code.to_string()),
        },
    }
}

/// Resolves a play intent to a concrete station, before any playback is stopped.
fn resolve_station(selection: Selection) -> Result<&'static Station> {
    match selection {
        Selection::Explicit(code) => stations::find(&code).with_context(|| {
            format!(
                "station not found: {code}\n\n{}\n\n{}",
                stations::format_reference(),
                COMMAND_HELP
            )
        }),
        Selection::Random => random_station(current_station_code()?),
    }
}

/// The station remembered in playback state, so a random draw can skip it.
fn current_station_code() -> Result<Option<String>> {
    Ok(read_state()?.map(|state| state.code))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RadioState {
    pid: u32,
    start_time: Option<u64>,
    code: String,
    description: String,
    #[serde(default)]
    process_kind: RadioProcessKind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
enum RadioProcessKind {
    #[default]
    Ffplay,
    PlaylistRunner,
    DirectRunner,
}

pub async fn run(args: RadioArgs) -> Result<()> {
    match parse_command(&args) {
        RadioCommand::List => {
            println!("{}\n\n{}", stations::format_reference(), COMMAND_HELP);
            Ok(())
        }
        RadioCommand::Status => {
            print_status()?;
            Ok(())
        }
        RadioCommand::Stop => stop(),
        RadioCommand::Play { selection } => play(*resolve_station(selection)?).await,
    }
}

/// Picks a random station from the catalog, skipping `exclude_code`.
fn random_station(exclude_code: Option<String>) -> Result<&'static Station> {
    let exclude_code = exclude_code.as_deref();
    let pool = stations::eligible_stations(exclude_code);
    let chosen = random_station_in(&pool, choose_index)?;
    tracing::debug!(
        chosen = chosen.code,
        excluded = exclude_code.unwrap_or(""),
        candidates = pool.len(),
        "random radio station"
    );
    Ok(chosen)
}

fn choose_index(len: usize) -> usize {
    rand::rng().random_range(0..len)
}

/// Picks one station from `pool`. `choose` receives the pool length and must
/// return an index below it, so tests can pin the draw without touching the RNG.
fn random_station_in(
    pool: &[&'static Station],
    choose: impl FnMut(usize) -> usize,
) -> Result<&'static Station> {
    stations::choose_station(pool, choose).ok_or_else(|| anyhow!("no random station available"))
}

async fn play(station: Station) -> Result<()> {
    let _ = stop_existing();
    let (player, process_kind) = match station.kind {
        StationKind::YouTubePlaylist => (
            ExternalPlayer::start_playlist(station.code)?,
            RadioProcessKind::PlaylistRunner,
        ),
        // Direct and YouTube stations run under a supervisor runner that
        // respawns playback when the stream fails; see ADR-0014.
        _ => (
            ExternalPlayer::start_station_runner(station.code)?,
            RadioProcessKind::DirectRunner,
        ),
    };
    write_state(&RadioState {
        pid: player.pid,
        start_time: player.start_time,
        code: station.code.to_string(),
        description: station.description.to_string(),
        process_kind,
    })?;
    println!("playing {} {}", station.code, station.description);
    Ok(())
}

fn stop() -> Result<()> {
    let _ = stop_existing()?;
    println!("stopped");
    Ok(())
}

fn print_status() -> Result<()> {
    let Some(state) = read_state()? else {
        println!("stopped");
        return Ok(());
    };

    if process_is_alive(&state) {
        println!("playing {} {}", state.code, state.description);
    } else {
        let _ = remove_state();
        println!("stopped");
    }

    Ok(())
}

async fn playable_url(station: Station) -> Result<String> {
    match station.kind {
        StationKind::Direct => Ok(station.url.to_string()),
        StationKind::YouTube => resolve_youtube_stream_url(station.url).await,
        StationKind::YouTubePlaylist => bail!("playlist stations are not playable yet"),
    }
}

fn stop_existing() -> Result<bool> {
    let Some(state) = read_state()? else {
        return Ok(false);
    };
    stop_process(&state);
    remove_state()?;
    Ok(true)
}

async fn resolve_youtube_stream_url(url: &str) -> Result<String> {
    let yt_dlp = resolve_yt_dlp().await?;
    let output = Command::new(yt_dlp)
        .args(youtube_stream_url_args(url))
        .output()
        .context("failed to run yt-dlp")?;
    if !output.status.success() {
        bail!(
            "yt-dlp failed to resolve YouTube station: {}",
            yt_dlp_reason(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .context("yt-dlp produced no stream URL")?
        .to_string())
}

/// yt-dlp buries the real cause under progress lines on stderr, so prefer its ERROR line.
/// Without this, a dead or unresolvable station URL reports only that resolution failed.
fn yt_dlp_reason(stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let reason = lines
        .iter()
        .find(|line| line.starts_with("ERROR"))
        .or_else(|| lines.first())
        .copied()
        .unwrap_or("yt-dlp reported no reason");
    reason.strip_prefix("ERROR: ").unwrap_or(reason).to_string()
}

fn youtube_stream_url_args(url: &str) -> [&str; 5] {
    ["-g", "--no-playlist", "-f", "bestaudio", url]
}

/// How long one playback attempt must last to count as a healthy session and
/// reset the failure streak.
const HEALTHY_SESSION_MIN: Duration = Duration::from_secs(60);

/// Consecutive short-lived attempts before a station runner gives up.
const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// What a station runner does after one playback attempt finishes.
#[derive(Debug, PartialEq, Eq)]
enum Retry {
    /// Respawn playback after the backoff delay.
    After(Duration),
    /// Too many consecutive short-lived failures: stop trying.
    GiveUp,
}

/// Escalating respawn backoff for a station runner: 2s, 5s, 15s, then 30s.
fn respawn_backoff(failure_index: u32) -> Duration {
    const STEPS: [u64; 4] = [2, 5, 15, 30];
    Duration::from_secs(STEPS[(failure_index as usize).min(STEPS.len() - 1)])
}

/// Failure accounting for one station runner. A session that lasted at least
/// `HEALTHY_SESSION_MIN` resets the streak, so long-running playback never
/// accumulates toward the give-up threshold.
#[derive(Debug, Default)]
struct StreamRetry {
    consecutive_failures: u32,
}

impl StreamRetry {
    fn new() -> Self {
        Self::default()
    }

    fn after_attempt(&mut self, attempt_duration: Duration) -> Retry {
        if attempt_duration >= HEALTHY_SESSION_MIN {
            self.consecutive_failures = 0;
        }
        self.consecutive_failures += 1;
        if self.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
            Retry::GiveUp
        } else {
            Retry::After(respawn_backoff(self.consecutive_failures - 1))
        }
    }
}

/// Runs one direct or YouTube station under a supervisor: respawn ffplay
/// whenever the stream ends or fails, re-resolving YouTube URLs each attempt,
/// and give up (with a notification) after too many consecutive failures.
pub(crate) async fn run_direct_runner(code: String) -> Result<()> {
    let station = stations::find(&code)
        .with_context(|| format!("unknown radio station '{code}'"))?;
    run_station_stream_loop(*station).await
}

async fn run_station_stream_loop(station: Station) -> Result<()> {
    let mut retry = StreamRetry::new();
    loop {
        let started = Instant::now();
        let attempt = match station.kind {
            StationKind::YouTubePlaylist => bail!("playlist stations use the playlist runner"),
            // Resolution failures count as attempt failures: they feed the
            // retry loop instead of killing the runner.
            StationKind::Direct | StationKind::YouTube => match playable_url(station).await {
                Ok(url) => ExternalPlayer::play_live_until_exit(url).await,
                Err(error) => Err(error),
            },
        };
        match attempt {
            Ok(()) => tracing::debug!(station = %station.code, "radio stream ended"),
            Err(error) => {
                tracing::warn!(station = %station.code, error = %error, "radio stream attempt failed")
            }
        }
        match retry.after_attempt(started.elapsed()) {
            Retry::After(delay) => {
                tracing::info!(
                    station = %station.code,
                    delay = delay.as_secs(),
                    "respawning radio stream"
                );
                tokio::time::sleep(delay).await;
            }
            Retry::GiveUp => {
                let message = format!(
                    "lum radio: {} stopped after {MAX_CONSECUTIVE_FAILURES} failed stream attempts",
                    station.description
                );
                tracing::error!(station = %station.code, "{message}");
                notify_stream_lost(&message);
                bail!("{message}");
            }
        }
    }
}

fn notify_stream_lost(message: &str) {
    if let Err(error) = notify_rust::Notification::new()
        .summary("lum radio")
        .body(message)
        .show()
    {
        tracing::warn!(error = %error, "failed to show the radio give-up notification");
    }
}

pub(crate) async fn run_playlist_runner(code: String) -> Result<()> {
    let urls = randomized_playlist_urls(&code)?;
    loop {
        for (index, url) in urls.iter().enumerate() {
            let item_number = index + 1;
            let stream_url = match resolve_youtube_stream_url(url).await {
                Ok(stream_url) => stream_url,
                Err(error) => {
                    let message =
                        playlist_failure_message(&code, item_number, url, &error.to_string());
                    tracing::error!(station = %code, item = item_number, url = %url, error = %error, "{message}");
                    bail!(message);
                }
            };

            if let Err(error) = ExternalPlayer::play_until_exit(stream_url).await {
                let message = playlist_failure_message(&code, item_number, url, &error.to_string());
                tracing::error!(station = %code, item = item_number, url = %url, error = %error, "{message}");
                bail!(message);
            }
        }
    }
}

fn randomized_playlist_urls(code: &str) -> Result<Vec<&'static str>> {
    let mut urls = stations::playlist_urls(code)
        .with_context(|| format!("unknown radio playlist station '{code}'"))?
        .to_vec();
    urls.shuffle(&mut rand::rng());
    Ok(urls)
}

fn playlist_failure_message(code: &str, item_number: usize, url: &str, error: &str) -> String {
    format!("radio playlist '{code}' failed at item {item_number} ({url}): {error}")
}

fn process_is_alive(state: &RadioState) -> bool {
    match state.process_kind {
        RadioProcessKind::Ffplay => ExternalPlayer::is_alive(state.pid, state.start_time),
        RadioProcessKind::PlaylistRunner
        | RadioProcessKind::DirectRunner => {
            ExternalPlayer::is_alive_any(state.pid, state.start_time)
        }
    }
}

fn stop_process(state: &RadioState) {
    match state.process_kind {
        RadioProcessKind::Ffplay => ExternalPlayer::stop(state.pid, state.start_time),
        RadioProcessKind::PlaylistRunner | RadioProcessKind::DirectRunner => {
            ExternalPlayer::stop_any(state.pid, state.start_time)
        }
    }
}

fn state_file() -> Result<PathBuf> {
    paths::state_dir("radio-player.json")
}

fn read_state() -> Result<Option<RadioState>> {
    let path = state_file()?;
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read radio state {}", path.display()))?;
    Ok(Some(serde_json::from_str(&data)?))
}

fn write_state(state: &RadioState) -> Result<()> {
    let path = state_file()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(state)?)
        .with_context(|| format!("failed to write radio state {}", path.display()))
}

fn remove_state() -> Result<()> {
    let path = state_file()?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::RadioArgs;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn routes_list_when_no_arg() {
        assert_eq!(parse_command(&RadioArgs { arg: None }), RadioCommand::List);
    }

    #[test]
    fn routes_status_as_command() {
        assert_eq!(
            parse_command(&RadioArgs {
                arg: Some("status".into())
            }),
            RadioCommand::Status
        );
    }

    #[test]
    fn routes_code_as_play_command() {
        assert_eq!(
            parse_command(&RadioArgs {
                arg: Some("atma".into())
            }),
            RadioCommand::Play {
                selection: Selection::Explicit("atma".into())
            }
        );
    }

    #[test]
    fn routes_rand_as_random_selection() {
        assert_eq!(
            parse_command(&RadioArgs {
                arg: Some("rand".into())
            }),
            RadioCommand::Play {
                selection: Selection::Random
            }
        );
    }

    #[test]
    fn pause_routes_as_play_command() {
        assert_eq!(
            parse_command(&RadioArgs {
                arg: Some("pause".into())
            }),
            RadioCommand::Play {
                selection: Selection::Explicit("pause".into())
            }
        );
    }

    #[test]
    fn resume_routes_as_play_command() {
        assert_eq!(
            parse_command(&RadioArgs {
                arg: Some("resume".into())
            }),
            RadioCommand::Play {
                selection: Selection::Explicit("resume".into())
            }
        );
    }

    #[test]
    fn legacy_state_defaults_to_ffplay_process_kind() {
        let state: RadioState = serde_json::from_str(
            r#"{"pid":123,"start_time":456,"code":"atma","description":"atma.fm Channel 1"}"#,
        )
        .unwrap();

        assert_eq!(state.process_kind, RadioProcessKind::Ffplay);
    }

    #[test]
    fn unknown_station_error_lists_stations_and_the_random_selector() {
        let error = resolve_station(Selection::Explicit("nope".into()))
            .expect_err("an unknown code must fail");
        let message = error.to_string();

        assert!(message.starts_with("station not found: nope"));
        assert!(message.contains("atma  atma.fm Channel 1"));
        assert!(message.contains("rand  Play a random built-in station"));
    }

    #[test]
    fn random_selection_fails_when_no_station_is_eligible() {
        let pool: &[&'static Station] = &[];

        let error = random_station_in(pool, |_| 0).expect_err("an empty pool must fail");

        assert_eq!(error.to_string(), "no random station available");
    }

    #[test]
    fn random_station_draws_from_the_catalog_and_skips_the_remembered_one() {
        for _ in 0..200 {
            let station = random_station(Some("atma".into())).expect("catalog has stations");

            assert_ne!(station.code, "atma");
            assert!(stations::all().iter().any(|s| s.code == station.code));
        }
    }

    #[test]
    fn random_station_draws_from_the_catalog_without_an_exclusion() {
        for _ in 0..200 {
            let station = random_station(None).expect("catalog has stations");

            assert!(stations::all().iter().any(|s| s.code == station.code));
        }
    }

    #[test]
    fn playlist_failure_message_names_station_item_and_error() {
        let message =
            playlist_failure_message("aphx", 2, "https://example.test/watch", "yt-dlp failed");
        assert!(message.contains("aphx"));
        assert!(message.contains("item 2"));
        assert!(message.contains("https://example.test/watch"));
        assert!(message.contains("yt-dlp failed"));
    }

    #[test]
    fn playlist_runner_randomizes_a_copy_of_station_urls() {
        let mut randomized = randomized_playlist_urls("aphx").unwrap();
        randomized.sort_unstable();

        let mut expected = stations::playlist_urls("aphx").unwrap().to_vec();
        expected.sort_unstable();

        assert_eq!(randomized, expected);
    }

    #[test]
    fn yt_dlp_failure_reports_the_reason_yt_dlp_printed() {
        let stderr = b"[youtube] Extracting URL: https://example.test/watch\n\
                      [youtube] X4VbdwhkE10: Downloading webpage\n\
                      ERROR: [youtube] X4VbdwhkE10: This live stream recording is not available.\n";

        assert_eq!(
            yt_dlp_reason(stderr),
            "[youtube] X4VbdwhkE10: This live stream recording is not available."
        );
    }

    #[test]
    fn yt_dlp_failure_without_an_error_line_keeps_the_last_output() {
        assert_eq!(
            yt_dlp_reason(b"[youtube] X4VbdwhkE10: Downloading webpage\nWARNING: no formats\n"),
            "[youtube] X4VbdwhkE10: Downloading webpage"
        );
    }

    #[test]
    fn yt_dlp_failure_without_any_output_still_explains_itself() {
        assert_eq!(yt_dlp_reason(b"\n"), "yt-dlp reported no reason");
    }

    #[test]
    fn youtube_resolution_requests_audio_only_for_radio() {
        assert_eq!(
            youtube_stream_url_args("https://www.youtube.com/watch?v=oR4gjzXs5EE"),
            [
                "-g",
                "--no-playlist",
                "-f",
                "bestaudio",
                "https://www.youtube.com/watch?v=oR4gjzXs5EE",
            ]
        );
    }

    #[test]
    fn respawn_backoff_escalates_and_caps_at_thirty_seconds() {
        assert_eq!(respawn_backoff(0), Duration::from_secs(2));
        assert_eq!(respawn_backoff(1), Duration::from_secs(5));
        assert_eq!(respawn_backoff(2), Duration::from_secs(15));
        assert_eq!(respawn_backoff(3), Duration::from_secs(30));
        assert_eq!(respawn_backoff(99), Duration::from_secs(30));
    }

    #[test]
    fn stream_retry_gives_up_after_five_consecutive_short_attempts() {
        let mut retry = StreamRetry::new();
        let short = Duration::from_secs(1);
        for expected in [2, 5, 15, 30] {
            assert_eq!(
                retry.after_attempt(short),
                Retry::After(Duration::from_secs(expected))
            );
        }
        assert_eq!(retry.after_attempt(short), Retry::GiveUp);
    }

    #[test]
    fn stream_retry_resets_after_a_healthy_session() {
        let mut retry = StreamRetry::new();
        for _ in 0..4 {
            retry.after_attempt(Duration::from_secs(1));
        }
        // The 61s session is healthy, so the streak resets and backoff restarts.
        assert_eq!(
            retry.after_attempt(Duration::from_secs(61)),
            Retry::After(Duration::from_secs(2))
        );
        assert_eq!(
            retry.after_attempt(Duration::from_secs(1)),
            Retry::After(Duration::from_secs(5))
        );
    }

    /// End-to-end respawn probe: the runner must spawn a fresh ffplay after a
    /// permanent stream failure (HTTP 404, like an expired stream URL).
    #[tokio::test]
    async fn station_runner_respawns_after_a_permanent_stream_failure() {
        if !player::playback_tests_enabled() {
            eprintln!("skipping: set LUM_RADIO_PLAYBACK_TESTS=1 to run");
            return;
        }
        let Some(bytes) = player::fixture_mp3() else {
            eprintln!("skipping: ffmpeg not available for fixture generation");
            return;
        };
        let stop = Arc::new(AtomicBool::new(false));
        let accepts = Arc::new(AtomicUsize::new(0));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind fixture");
        let addr = listener.local_addr().expect("fixture address");
        {
            let stop = stop.clone();
            let accepts = accepts.clone();
            std::thread::spawn(move || {
                use std::io::ErrorKind;

                listener.set_nonblocking(true).expect("nonblocking listener");
                loop {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let attempt = accepts.fetch_add(1, Ordering::Relaxed);
                            let bytes = bytes.clone();
                            let stop = stop.clone();
                            std::thread::spawn(move || {
                                use std::io::Write as _;

                                let mut stream = stream;
                                if attempt == 0 {
                                    // A permanent HTTP failure, like an expired
                                    // stream URL that reconnect cannot heal.
                                    let _ = stream.write_all(
                                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                                    );
                                    return;
                                }
                                let _ = stream.write_all(
                                    b"HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nConnection: close\r\n\r\n",
                                );
                                loop {
                                    if stop.load(Ordering::Relaxed) {
                                        return;
                                    }
                                    if stream.write_all(&bytes).is_err() {
                                        return;
                                    }
                                }
                            });
                        }
                        Err(ref error) if error.kind() == ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(50));
                        }
                        Err(_) => return,
                    }
                }
            });
        }
        // Station is Copy and the runner takes it by value; only the URL
        // must be leaked, because the catalog models URLs as &'static str.
        let url: &'static str =
            Box::leak(format!("http://{addr}/stream.mp3").into_boxed_str());
        let station = Station {
            code: "fx404",
            description: "fixture 404 station",
            url,
            kind: StationKind::Direct,
        };
        let runner = tokio::spawn(run_station_stream_loop(station));
        // The first attempt fails fast; after the 2s backoff the runner must
        // respawn ffplay, which connects as the second fixture connection.
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline && accepts.load(Ordering::Relaxed) < 2 {
            assert!(!runner.is_finished(), "runner exited before respawning");
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let served = accepts.load(Ordering::Relaxed);
        assert!(
            served >= 2,
            "runner never respawned (connections served: {served})"
        );
        assert!(!runner.is_finished(), "runner gave up too early");
        stop.store(true, Ordering::Relaxed);
        runner.abort();
        let _ = Command::new("pkill").args(["-f", url]).status();
    }
}
