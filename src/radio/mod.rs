mod icon;
mod playback;
mod player;
pub mod stations;
mod tray;

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::cli::RadioArgs;
use crate::paths;
use crate::yt::resolve_yt_dlp;
use playback::Playback;
use player::{ExternalPlayer, Outcome};
use stations::{Station, StationKind};

const COMMAND_HELP: &str = "Commands:\n  lum radio <code>  play a station (example: lum radio dron)\n  lum radio rand    play a random station, never the current one\n  lum radio status  show playback state\n  lum radio stop    stop playback";

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
    /// Mirrors the tray's pause toggle, written by the runner itself. Absent in older state files.
    #[serde(default)]
    paused: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
enum RadioProcessKind {
    #[default]
    Ffplay,
    PlaylistRunner,
    DirectRunner,
}

pub fn run(args: RadioArgs) -> Result<()> {
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
        RadioCommand::Play { selection } => play(*resolve_station(selection)?),
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
    fastrand::usize(0..len)
}

/// Picks one station from `pool`. `choose` receives the pool length and must
/// return an index below it, so tests can pin the draw without touching the RNG.
fn random_station_in(
    pool: &[&'static Station],
    choose: impl FnMut(usize) -> usize,
) -> Result<&'static Station> {
    stations::choose_station(pool, choose).ok_or_else(|| anyhow!("no random station available"))
}

fn play(station: Station) -> Result<()> {
    let _ = stop_existing();
    start(station)?;
    println!("playing {} {}", station.code, station.name);
    Ok(())
}

/// Starts `station`'s runner and makes it the remembered one. Whatever was
/// remembered before is the caller's to stop.
fn start(station: Station) -> Result<()> {
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
        description: station.name.to_string(),
        process_kind,
        paused: false,
    })
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
        println!("{}", status_line(&state));
    } else {
        let _ = remove_state();
        println!("stopped");
    }

    Ok(())
}

/// `status` output for a live runner: `paused` while the tray has playback
/// stopped, `playing` otherwise.
fn status_line(state: &RadioState) -> String {
    let verb = if state.paused { "paused" } else { "playing" };
    format!("{verb} {} {}", state.code, state.description)
}

fn playable_url(station: Station) -> Result<String> {
    match station.kind {
        StationKind::Direct => Ok(station.url.to_string()),
        StationKind::YouTube => resolve_youtube_stream_url(station.url),
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

fn resolve_youtube_stream_url(url: &str) -> Result<String> {
    let yt_dlp = resolve_yt_dlp()?;
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
/// The supervisor runs next to the station's tray, which can pause it.
pub(crate) fn run_direct_runner(code: String) -> Result<()> {
    let station =
        *stations::find(&code).with_context(|| format!("unknown radio station '{code}'"))?;
    run_hosted(station, move |playback| {
        run_station_stream_loop(station, playback)
    })
}

/// Runs `supervise` on a worker thread next to the station's tray (ADR-0022) and
/// returns its result. The runner stays alive while paused: only `supervise`
/// finishing, or `lum radio stop` killing the runner, removes the tray.
fn run_hosted(
    station: Station,
    supervise: impl FnOnce(&Playback) -> Result<()> + Send + 'static,
) -> Result<()> {
    let playback = Arc::new(Playback::with_observer(record_paused));
    let supervisor = Arc::clone(&playback);
    let halt = Arc::clone(&playback);
    tray::host(
        station,
        playback,
        move |request| handle_tray_request(station, &halt, request),
        move || supervise(&supervisor),
    )
}

/// Carries out a tray menu request on the tray's thread (ADR-0023) and returns
/// whether this runner should now exit. A switch starts the successor first, so a
/// failed start leaves the current station playing.
fn handle_tray_request(current: Station, playback: &Playback, request: tray::Request) -> bool {
    let next = match request {
        tray::Request::Quit => None,
        tray::Request::Switch(station) => Some(Ok(station)),
        tray::Request::Random => Some(random_station(Some(current.code.to_string()))),
    };
    let handed_over = match next {
        None => {
            tracing::info!(station = %current.code, "radio quit from the tray");
            // A leftover state file is harmless: `status` drops one whose runner is gone.
            if let Err(error) = forget_own_state() {
                tracing::warn!(error = %error, "failed to clear the radio state on quit");
            }
            Ok(())
        }
        Some(next) => next.and_then(|next| {
            tracing::info!(from = %current.code, to = %next.code, "radio station switched from the tray");
            start(*next)
        }),
    };
    match handed_over {
        Ok(()) => {
            playback.halt();
            true
        }
        Err(error) => {
            tracing::warn!(error = %error, "radio tray request failed; playback continues");
            false
        }
    }
}

/// Removes the state file if it still names this runner, so `status` reports
/// `stopped` after a tray Quit without ever clobbering a successor's entry.
fn forget_own_state() -> Result<()> {
    if read_state()?.is_some_and(|state| state.pid == std::process::id()) {
        remove_state()?;
    }
    Ok(())
}

/// Mirrors the tray's pause state into `radio-player.json` so `lum radio status`
/// stays truthful. Best effort: a failed write must never disturb playback.
fn record_paused(paused: bool) {
    tracing::info!(paused, "radio playback toggled from the tray");
    if let Err(error) = persist_paused(paused) {
        tracing::warn!(error = %error, "failed to record the radio pause state");
    }
}

fn persist_paused(paused: bool) -> Result<()> {
    let Some(state) = read_state()? else {
        return Ok(());
    };
    if let Some(state) = with_pause_state(state, std::process::id(), paused) {
        write_state(&state)?;
    }
    Ok(())
}

/// Only the runner a state file names may change its pause state, so a replaced
/// runner can never clobber its successor's entry.
fn with_pause_state(mut state: RadioState, runner_pid: u32, paused: bool) -> Option<RadioState> {
    (state.pid == runner_pid).then(|| {
        state.paused = paused;
        state
    })
}

fn run_station_stream_loop(station: Station, playback: &Playback) -> Result<()> {
    if station.kind == StationKind::YouTubePlaylist {
        bail!("playlist stations use the playlist runner");
    }
    supervise_station(station, playback, || play_station_once(station, playback))
}

/// One playback attempt: resolve the stream URL, then play it until ffplay exits.
/// Resolution failures count as attempt failures: they feed the retry loop
/// instead of killing the runner.
fn play_station_once(station: Station, playback: &Playback) -> Result<Outcome> {
    let url = playable_url(station)?;
    ExternalPlayer::play_live_until_exit(url, playback)
}

/// The respawn policy for one station: run `attempt` over and over until too
/// many fail in a row. A pause from the tray is not a failure; the supervisor
/// idles until playback resumes and then starts over with a clean streak.
fn supervise_station(
    station: Station,
    playback: &Playback,
    mut attempt: impl FnMut() -> Result<Outcome>,
) -> Result<()> {
    let mut retry = StreamRetry::new();
    loop {
        if playback.wait_until_playing() {
            retry = StreamRetry::new();
        }
        let started = Instant::now();
        match attempt() {
            Ok(Outcome::Paused) => {
                retry = StreamRetry::new();
                continue;
            }
            Ok(Outcome::Ended) => tracing::debug!(station = %station.code, "radio stream ended"),
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
                // Wakes early on a pause; the loop then idles until resumed.
                playback.sleep(delay);
            }
            Retry::GiveUp => {
                let message = format!(
                    "lum radio: {} stopped after {MAX_CONSECUTIVE_FAILURES} failed stream attempts",
                    station.name
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

pub(crate) fn run_playlist_runner(code: String) -> Result<()> {
    let station = *stations::find(&code)
        .with_context(|| format!("unknown radio playlist station '{code}'"))?;
    let urls = randomized_playlist_urls(&code)?;
    run_hosted(station, move |playback| {
        run_playlist_loop(&code, &urls, playback)
    })
}

fn run_playlist_loop(code: &str, urls: &[&'static str], playback: &Playback) -> Result<()> {
    loop {
        for (index, url) in urls.iter().enumerate() {
            let item_number = index + 1;
            // A pause stops the current track; resuming plays it again from the start.
            loop {
                playback.wait_until_playing();
                let stream_url = match resolve_youtube_stream_url(url) {
                    Ok(stream_url) => stream_url,
                    Err(error) => {
                        let message =
                            playlist_failure_message(code, item_number, url, &error.to_string());
                        tracing::error!(station = %code, item = item_number, url = %url, error = %error, "{message}");
                        bail!(message);
                    }
                };

                match ExternalPlayer::play_until_exit(stream_url, playback) {
                    Ok(Outcome::Ended) => break,
                    Ok(Outcome::Paused) => continue,
                    Err(error) => {
                        let message =
                            playlist_failure_message(code, item_number, url, &error.to_string());
                        tracing::error!(station = %code, item = item_number, url = %url, error = %error, "{message}");
                        bail!(message);
                    }
                }
            }
        }
    }
}

fn randomized_playlist_urls(code: &str) -> Result<Vec<&'static str>> {
    let mut urls = stations::playlist_urls(code)
        .with_context(|| format!("unknown radio playlist station '{code}'"))?
        .to_vec();
    fastrand::shuffle(&mut urls);
    Ok(urls)
}

fn playlist_failure_message(code: &str, item_number: usize, url: &str, error: &str) -> String {
    format!("radio playlist '{code}' failed at item {item_number} ({url}): {error}")
}

fn process_is_alive(state: &RadioState) -> bool {
    match state.process_kind {
        RadioProcessKind::Ffplay => ExternalPlayer::is_alive(state.pid, state.start_time),
        RadioProcessKind::PlaylistRunner | RadioProcessKind::DirectRunner => {
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

/// Replaces the state file atomically: the runner rewrites it on every tray
/// toggle, and a concurrent `lum radio status` must never read half a file.
fn write_state(state: &RadioState) -> Result<()> {
    use std::io::Write as _;

    let path = state_file()?;
    let dir = path
        .parent()
        .context("radio state path has no parent directory")?;
    std::fs::create_dir_all(dir)?;
    let mut staged = tempfile::NamedTempFile::new_in(dir)
        .with_context(|| format!("failed to stage radio state in {}", dir.display()))?;
    staged.write_all(serde_json::to_string_pretty(state)?.as_bytes())?;
    staged
        .persist(&path)
        .with_context(|| format!("failed to write radio state {}", path.display()))?;
    Ok(())
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
                arg: Some("conc".into())
            }),
            RadioCommand::Play {
                selection: Selection::Explicit("conc".into())
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
        // "atma" is retired from the catalog; state files written before the
        // catalog swap can still name it.
        let state: RadioState = serde_json::from_str(
            r#"{"pid":123,"start_time":456,"code":"atma","description":"atma.fm Channel 1"}"#,
        )
        .unwrap();

        assert_eq!(state.process_kind, RadioProcessKind::Ffplay);
    }

    #[test]
    fn legacy_state_defaults_to_not_paused() {
        let state: RadioState = serde_json::from_str(
            r#"{"pid":123,"start_time":456,"code":"dron","description":"Drone Zone","process_kind":"DirectRunner"}"#,
        )
        .unwrap();

        assert!(!state.paused);
    }

    fn runner_state(paused: bool) -> RadioState {
        RadioState {
            pid: 7,
            start_time: Some(1),
            code: "dron".into(),
            description: "Drone Zone".into(),
            process_kind: RadioProcessKind::DirectRunner,
            paused,
        }
    }

    #[test]
    fn pause_state_round_trips_through_the_state_file_format() {
        let json = serde_json::to_string(&runner_state(true)).unwrap();
        let state: RadioState = serde_json::from_str(&json).unwrap();

        assert!(state.paused);
    }

    #[test]
    fn status_line_distinguishes_playing_from_paused() {
        assert_eq!(status_line(&runner_state(false)), "playing dron Drone Zone");
        assert_eq!(status_line(&runner_state(true)), "paused dron Drone Zone");
    }

    #[test]
    fn only_the_runner_a_state_names_may_change_its_pause_state() {
        assert!(
            with_pause_state(runner_state(false), 8, true).is_none(),
            "a different process must leave the entry alone"
        );
        assert!(
            with_pause_state(runner_state(false), 7, true)
                .unwrap()
                .paused
        );
        assert!(
            !with_pause_state(runner_state(true), 7, false)
                .unwrap()
                .paused
        );
    }

    /// Pausing and resuming must never feed the give-up streak. Only real stream
    /// failures do, and `StreamRetry` has its own tests for those.
    #[test]
    fn pausing_is_not_a_stream_failure() {
        let station = *stations::find("dron").expect("catalog has dron");
        let playback = Arc::new(Playback::default());
        let (started_tx, started) = std::sync::mpsc::channel();
        let (kill_tx, killed) = std::sync::mpsc::channel::<()>();
        let supervisor = {
            let playback = Arc::clone(&playback);
            std::thread::spawn(move || {
                supervise_station(station, &playback, || {
                    let _ = started_tx.send(());
                    // Stands in for ffplay: runs until the test kills it, as a pause would.
                    killed.recv().map_err(|_| anyhow!("test over"))?;
                    Ok(Outcome::Paused)
                })
            })
        };

        // More cycles than the give-up threshold: had pauses counted as failures, the
        // supervisor would have backed off and then given up by now.
        for cycle in 1..=MAX_CONSECUTIVE_FAILURES + 2 {
            started
                .recv_timeout(Duration::from_secs(10))
                .unwrap_or_else(|_| panic!("attempt {cycle} never started"));
            assert!(playback.toggle(), "pause");
            kill_tx.send(()).expect("the supervisor is listening");
            assert!(!playback.toggle(), "resume");
        }

        started
            .recv_timeout(Duration::from_secs(10))
            .expect("the attempt after the last resume starts at once");
        assert!(!supervisor.is_finished(), "the supervisor must not give up");

        // Park it: paused, it idles for the rest of the process instead of failing
        // later, which would end in a desktop notification.
        assert!(playback.toggle());
        kill_tx.send(()).expect("the supervisor is listening");
    }

    #[test]
    fn unknown_station_error_lists_stations_and_the_random_selector() {
        let error = resolve_station(Selection::Explicit("nope".into()))
            .expect_err("an unknown code must fail");
        let message = error.to_string();

        assert!(message.starts_with("station not found: nope"));
        assert!(message.contains("conc  RadCap Musique Concrète"));
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
            let station = random_station(Some("conc".into())).expect("catalog has stations");

            assert_ne!(station.code, "conc");
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
    #[test]
    fn station_runner_respawns_after_a_permanent_stream_failure() {
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

                listener
                    .set_nonblocking(true)
                    .expect("nonblocking listener");
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
        let url: &'static str = Box::leak(format!("http://{addr}/stream.mp3").into_boxed_str());
        let station = Station {
            code: "fx404",
            name: "fixture 404 station",
            url,
            kind: StationKind::Direct,
            tags: &[],
        };
        let runner =
            std::thread::spawn(move || run_station_stream_loop(station, &Playback::default()));
        // The first attempt fails fast; after the 2s backoff the runner must
        // respawn ffplay, which connects as the second fixture connection.
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline && accepts.load(Ordering::Relaxed) < 2 {
            assert!(!runner.is_finished(), "runner exited before respawning");
            std::thread::sleep(Duration::from_millis(200));
        }
        let served = accepts.load(Ordering::Relaxed);
        assert!(
            served >= 2,
            "runner never respawned (connections served: {served})"
        );
        assert!(!runner.is_finished(), "runner gave up too early");
        stop.store(true, Ordering::Relaxed);
        // A thread cannot be aborted: with the fixture stopped, the runner's
        // next ffplay fails to connect and the test process exit ends it.
        let _ = Command::new("pkill").args(["-f", url]).status();
    }

    /// End-to-end pause probe on the real ffplay spawn path: a pause must kill
    /// ffplay and keep the runner idle, and every resume must reconnect without
    /// ever counting toward the give-up streak.
    #[test]
    fn station_runner_pauses_and_resumes_ffplay_without_giving_up() {
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
        // Each connection feeds ~6s of audio and then stalls, so ffplay stays up
        // for the whole probe and any extra connection is a respawn we caused.
        let addr = player::spawn_repeating_fixture(
            bytes,
            3,
            player::FixtureMode::StallEach,
            stop.clone(),
            accepts.clone(),
        );
        let url: &'static str = Box::leak(format!("http://{addr}/stream.mp3").into_boxed_str());
        let station = Station {
            code: "fxpz",
            name: "fixture pause station",
            url,
            kind: StationKind::Direct,
            tags: &[],
        };
        let playback = Arc::new(Playback::default());
        let runner = {
            let playback = Arc::clone(&playback);
            std::thread::spawn(move || run_station_stream_loop(station, &playback))
        };
        let wait_for_connections = |wanted: usize| {
            let deadline = Instant::now() + Duration::from_secs(20);
            while Instant::now() < deadline && accepts.load(Ordering::Relaxed) < wanted {
                assert!(
                    !runner.is_finished(),
                    "runner exited before connection {wanted}"
                );
                std::thread::sleep(Duration::from_millis(100));
            }
            let served = accepts.load(Ordering::Relaxed);
            assert!(
                served >= wanted,
                "ffplay never reached connection {wanted} (served: {served})"
            );
        };

        let cycles = MAX_CONSECUTIVE_FAILURES as usize + 1;
        for cycle in 1..=cycles {
            wait_for_connections(cycle);
            assert!(playback.toggle(), "pause");
            if cycle == 1 {
                // Longer than the first respawn backoff: a pause mistaken for a
                // failure would respawn ffplay right here.
                std::thread::sleep(Duration::from_millis(2500));
                assert_eq!(
                    accepts.load(Ordering::Relaxed),
                    1,
                    "ffplay respawned while paused"
                );
            }
            assert!(!playback.toggle(), "resume");
        }
        wait_for_connections(cycles + 1);
        assert!(
            !runner.is_finished(),
            "runner gave up although the user only paused it"
        );

        // Pause once more: this kills the last ffplay and parks the runner, so
        // nothing keeps playing or retrying after the probe.
        assert!(playback.toggle());
        stop.store(true, Ordering::Relaxed);
    }
}
