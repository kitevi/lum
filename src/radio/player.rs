use std::{
    ffi::OsStr,
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};
use sysinfo::{Pid, ProcessesToUpdate, Signal, System};

use crate::ffmpeg;

pub(super) struct ExternalPlayer;

#[derive(Debug, Clone, Copy)]
pub(super) struct PlayerProcess {
    pub(super) pid: u32,
    pub(super) start_time: Option<u64>,
}

impl ExternalPlayer {
    pub(super) fn start_station_runner(code: &str) -> Result<PlayerProcess> {
        let exe = std::env::current_exe().context("failed to resolve lum executable")?;
        let child = station_runner_command(exe, code)
            .spawn()
            .context("failed to start radio station runner")?;
        let pid = child.id();
        Ok(PlayerProcess {
            pid,
            start_time: process_start_time(pid),
        })
    }

    pub(super) fn start_playlist(code: &str) -> Result<PlayerProcess> {
        let exe = std::env::current_exe().context("failed to resolve lum executable")?;
        let child = playlist_runner_command(exe, code)
            .spawn()
            .context("failed to start radio playlist runner")?;
        let pid = child.id();
        Ok(PlayerProcess {
            pid,
            start_time: process_start_time(pid),
        })
    }

    pub(super) fn is_alive(pid: u32, start_time: Option<u64>) -> bool {
        process_alive(pid, start_time)
    }

    pub(super) fn stop(pid: u32, start_time: Option<u64>) {
        kill_pid(pid, start_time);
    }

    pub(super) fn is_alive_any(pid: u32, start_time: Option<u64>) -> bool {
        process_alive_any(pid, start_time)
    }

    pub(super) fn stop_any(pid: u32, start_time: Option<u64>) {
        kill_pid_any(pid, start_time);
    }

    pub(super) async fn play_until_exit(url: String) -> Result<()> {
        Self::play_ffplay_until_exit(ffplay_command, url).await
    }

    /// Like `play_until_exit`, but with live-stream reconnect recovery so
    /// dropped or stalled connections heal without exiting.
    pub(super) async fn play_live_until_exit(url: String) -> Result<()> {
        Self::play_ffplay_until_exit(live_stream_ffplay_command, url).await
    }

    async fn play_ffplay_until_exit(
        build_command: impl Fn(String, &str) -> Command + Send + 'static,
        url: String,
    ) -> Result<()> {
        let ffplay = ffmpeg::resolve_ffplay().await?;
        let ffplay = ffplay.to_string_lossy().into_owned();
        // The blocking wait must run off the async runtime: the station runner
        // shares its runtime thread with other work in tests and in production.
        let status = tokio::task::spawn_blocking(move || build_command(ffplay, &url).status())
            .await
            .context("ffplay task join failed")?
            .context("failed to start ffplay")?;
        if !status.success() {
            bail!("ffplay exited with code {:?}", status.code());
        }
        Ok(())
    }
}

pub(super) fn ffplay_command(ffplay: impl AsRef<std::ffi::OsStr>, url: &str) -> Command {
    ffplay_command_with(ffplay, url, &[])
}

/// ffplay command for live streams: adds transparent reconnect behavior so
/// dropped or stalled connections heal in-process without leaving silence.
/// Only used for live stations; playlist playback must see track EOF to
/// advance, so it uses the base command.
fn live_stream_ffplay_command(ffplay: impl AsRef<std::ffi::OsStr>, url: &str) -> Command {
    ffplay_command_with(
        ffplay,
        url,
        &[
            "-reconnect",
            "1",
            "-reconnect_streamed",
            "1",
            "-reconnect_delay_max",
            "5",
        ],
    )
}

fn ffplay_command_with(
    ffplay: impl AsRef<std::ffi::OsStr>,
    url: &str,
    extra_args: &[&str],
) -> Command {
    let mut command = Command::new(ffplay);
    let mut args = vec![
        "-nodisp",
        "-hide_banner",
        "-loglevel",
        "error",
        // Exit at end of stream so a supervisor can decide to respawn.
        "-autoexit",
        // Give up on a connection that delivers no data for 15s instead of
        // blocking forever on a stalled stream.
        "-rw_timeout",
        "15000000",
    ];
    args.extend_from_slice(extra_args);
    args.push(url);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn station_runner_command(exe: impl AsRef<Path>, code: &str) -> Command {
    let mut command = Command::new(exe.as_ref());
    command
        .args(["__radio_direct_runner", code])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn playlist_runner_command(exe: impl AsRef<Path>, code: &str) -> Command {
    let mut command = Command::new(exe.as_ref());
    command
        .args(["__radio_playlist_runner", code])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn process_alive(pid: u32, start_time: Option<u64>) -> bool {
    with_ffplay_process(pid, start_time, |_| ()).is_some()
}

fn kill_pid(pid: u32, start_time: Option<u64>) {
    let _ = with_ffplay_process(pid, start_time, |process| {
        match process.kill_with(Signal::Term) {
            Some(true) => true,
            _ => process.kill(),
        }
    });
}

fn process_alive_any(pid: u32, start_time: Option<u64>) -> bool {
    with_process(pid, |process| {
        process_start_time_matches(process.start_time(), start_time)
    })
    .unwrap_or(false)
}

fn kill_pid_any(pid: u32, start_time: Option<u64>) {
    let pid = Pid::from_u32(pid);
    let mut system = System::new_all();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let Some(process) = system.process(pid) else {
        return;
    };
    if !process_start_time_matches(process.start_time(), start_time) {
        return;
    }

    kill_descendants(&system, pid);
    terminate_process(process);
}

fn kill_descendants(system: &System, parent: Pid) {
    let children: Vec<_> = system
        .processes()
        .iter()
        .filter_map(|(pid, process)| (process.parent() == Some(parent)).then_some(*pid))
        .collect();

    for child in children {
        kill_descendants(system, child);
        if let Some(process) = system.process(child) {
            terminate_process(process);
        }
    }
}

fn terminate_process(process: &sysinfo::Process) -> bool {
    match process.kill_with(Signal::Term) {
        Some(true) => true,
        _ => process.kill(),
    }
}

fn process_start_time(pid: u32) -> Option<u64> {
    with_process(pid, |process| process.start_time())
}

fn with_ffplay_process<R>(
    pid: u32,
    start_time: Option<u64>,
    f: impl FnOnce(&sysinfo::Process) -> R,
) -> Option<R> {
    with_process(pid, |process| {
        (is_ffplay_process(process) && process_start_time_matches(process.start_time(), start_time))
            .then(|| f(process))
    })
    .flatten()
}

fn with_process<R>(pid: u32, f: impl FnOnce(&sysinfo::Process) -> R) -> Option<R> {
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).map(f)
}

fn is_ffplay_process(process: &sysinfo::Process) -> bool {
    process
        .exe()
        .and_then(|path| path.file_name())
        .is_some_and(is_ffplay_name)
        || is_ffplay_name(process.name())
}

fn is_ffplay_name(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    name.eq_ignore_ascii_case("ffplay") || name.eq_ignore_ascii_case("ffplay.exe")
}

fn process_start_time_matches(actual: u64, expected: Option<u64>) -> bool {
    expected.is_none_or(|expected| actual == expected)
}

#[cfg(test)]
pub(super) fn playback_tests_enabled() -> bool {
    std::env::var("LUM_RADIO_PLAYBACK_TESTS").is_ok_and(|value| value != "0")
}

/// Two seconds of silent MP3, generated once with the system ffmpeg. The
/// canonical file is only ever published through an atomic rename from a
/// process-unique staging file, so a crashed generator cannot leave a
/// partial file for a later run to read.
#[cfg(test)]
pub(super) fn fixture_mp3() -> Option<Vec<u8>> {
    let ffmpeg = which::which("ffmpeg").ok()?;
    let path = std::env::temp_dir().join("lum-radio-fixture-silence.mp3");
    if !path.exists() {
        let staging = std::env::temp_dir().join(format!(
            "lum-radio-fixture-silence-{}.tmp",
            std::process::id()
        ));
        let status = Command::new(ffmpeg)
            .args([
                "-f", "lavfi", "-i", "anullsrc=r=44100:cl=mono", "-t", "2", "-b:a", "32k",
                "-f", "mp3", "-y",
            ])
            .arg(&staging)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok()?;
        if !status.success() {
            return None;
        }
        if std::fs::rename(&staging, &path).is_err() {
            let _ = std::fs::remove_file(&staging);
        }
    }
    let bytes = std::fs::read(path).ok()?;
    // A valid 2s/32kbps fixture is several KB; anything tiny is a truncated
    // or planted file, so skip the probes rather than act on it.
    (bytes.len() > 1024).then_some(bytes)
}

#[cfg(test)]
#[derive(Debug, Clone, Copy)]
enum FixtureMode {
    /// Close each connection after writing the fixture audio.
    DropEach,
    /// Hold each connection open without sending anything.
    StallEach,
}

/// Fixture HTTP server for playback probes: accepts connections until `stop`
/// is set, serving `loops` passes of the fixture bytes per connection, then
/// dropping or stalling the connection depending on `mode`. Counts accepted
/// connections in `accepts` so tests can observe reconnects and respawns.
#[cfg(test)]
fn spawn_repeating_fixture(
    bytes: Vec<u8>,
    loops: u32,
    mode: FixtureMode,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    accepts: std::sync::Arc<std::sync::atomic::AtomicUsize>,
) -> std::net::SocketAddr {
    use std::io::ErrorKind;
    use std::net::TcpListener;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture listener");
    let addr = listener.local_addr().expect("fixture local address");
    std::thread::spawn(move || {
        listener
            .set_nonblocking(true)
            .expect("nonblocking fixture listener");
        loop {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    accepts.fetch_add(1, Ordering::Relaxed);
                    let bytes = bytes.clone();
                    std::thread::spawn(move || {
                        serve_fixture_connection(stream, bytes, loops, mode);
                    });
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => return,
            }
        }
    });
    addr
}

#[cfg(test)]
fn serve_fixture_connection(
    mut stream: std::net::TcpStream,
    bytes: Vec<u8>,
    loops: u32,
    mode: FixtureMode,
) {
    use std::io::Write as _;
    use std::net::Shutdown;
    use std::time::Duration;

    let header = b"HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nConnection: close\r\n\r\n";
    if stream.write_all(header).is_err() {
        return;
    }
    for _ in 0..loops {
        if stream.write_all(&bytes).is_err() {
            return;
        }
    }
    match mode {
        FixtureMode::DropEach => {
            let _ = stream.shutdown(Shutdown::Both);
        }
        FixtureMode::StallEach => {
            std::thread::sleep(Duration::from_secs(120));
        }
    }
}
#[cfg(test)]
mod tests {

    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    #[test]
    fn ffplay_command_uses_quiet_self_healing_audio_args() {
        let command = ffplay_command("ffplay", "https://example.test/stream");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();

        assert_eq!(
            args,
            [
                "-nodisp",
                "-hide_banner",
                "-loglevel",
                "error",
                "-autoexit",
                "-rw_timeout",
                "15000000",
                "https://example.test/stream"
            ]
        );
    }

    #[test]
    fn live_stream_ffplay_command_adds_reconnect_recovery_args() {
        let command = live_stream_ffplay_command("ffplay", "https://example.test/live");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();

        assert_eq!(
            args,
            [
                "-nodisp",
                "-hide_banner",
                "-loglevel",
                "error",
                "-autoexit",
                "-rw_timeout",
                "15000000",
                "-reconnect",
                "1",
                "-reconnect_streamed",
                "1",
                "-reconnect_delay_max",
                "5",
                "https://example.test/live"
            ]
        );
    }

    #[test]
    fn playlist_runner_command_invokes_hidden_top_level_command() {
        let command = playlist_runner_command("/bin/lum", "aphx");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();

        assert_eq!(args, ["__radio_playlist_runner", "aphx"]);
    }

    #[test]
    fn non_ffplay_pid_is_not_considered_alive() {
        assert!(!ExternalPlayer::is_alive(std::process::id(), None));
    }

    #[test]
    fn expected_process_start_time_must_match_when_known() {
        assert!(process_start_time_matches(42, None));
        assert!(process_start_time_matches(42, Some(42)));
        assert!(!process_start_time_matches(42, Some(7)));
    }

    #[test]
    fn station_runner_command_invokes_hidden_top_level_command() {
        let command = station_runner_command("/bin/lum", "ssom");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();

        assert_eq!(args, ["__radio_direct_runner", "ssom"]);
    }

    async fn spawn_probe_ffplay(url: &str) -> Result<PlayerProcess> {
        let ffplay = ffmpeg::resolve_ffplay().await?;
        let child = live_stream_ffplay_command(ffplay, url)
            .spawn()
            .context("failed to start ffplay probe")?;
        let pid = child.id();
        Ok(PlayerProcess {
            pid,
            start_time: process_start_time(pid),
        })
    }

    /// Connection-drop probe: with the reconnect flags, ffplay must stay up
    /// and pull a fresh connection from the fixture server instead of sitting
    /// silent after a drop.
    #[tokio::test]
    async fn ffplay_reconnects_when_the_stream_connection_drops() {
        if !playback_tests_enabled() {
            eprintln!("skipping: set LUM_RADIO_PLAYBACK_TESTS=1 to run");
            return;
        }
        let Some(bytes) = fixture_mp3() else {
            eprintln!("skipping: ffmpeg not available for fixture generation");
            return;
        };
        let stop = Arc::new(AtomicBool::new(false));
        let accepts = Arc::new(AtomicUsize::new(0));
        let addr = spawn_repeating_fixture(
            bytes,
            3,
            FixtureMode::DropEach,
            stop.clone(),
            accepts.clone(),
        );
        let player = spawn_probe_ffplay(&format!("http://{addr}/stream.mp3"))
            .await
            .expect("ffplay probe spawn");
        // Each connection feeds ~6s of audio, so by +20s at least one
        // transparent reconnect must have pulled a second connection.
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline && accepts.load(Ordering::Relaxed) < 2 {
            std::thread::sleep(Duration::from_millis(200));
        }
        let served = accepts.load(Ordering::Relaxed);
        assert!(
            served >= 2,
            "ffplay never reconnected after the drop (connections served: {served})"
        );
        assert!(
            ExternalPlayer::is_alive(player.pid, player.start_time),
            "ffplay exited during reconnectable drops"
        );
        stop.store(true, Ordering::Relaxed);
        // Kill the probe ffplay explicitly; it need not exit on its own.
        ExternalPlayer::stop(player.pid, player.start_time);
    }

    /// Stream-stall probe: the server holds connections open without sending,
    /// so the read timeout must fire and drive a reconnect to a fresh
    /// connection instead of leaving ffplay silent forever.
    #[tokio::test]
    async fn ffplay_reconnects_when_the_stream_stalls() {
        if !playback_tests_enabled() {
            eprintln!("skipping: set LUM_RADIO_PLAYBACK_TESTS=1 to run");
            return;
        }
        let Some(bytes) = fixture_mp3() else {
            eprintln!("skipping: ffmpeg not available for fixture generation");
            return;
        };
        let stop = Arc::new(AtomicBool::new(false));
        let accepts = Arc::new(AtomicUsize::new(0));
        let addr = spawn_repeating_fixture(
            bytes,
            3,
            FixtureMode::StallEach,
            stop.clone(),
            accepts.clone(),
        );
        let player = spawn_probe_ffplay(&format!("http://{addr}/stream.mp3"))
            .await
            .expect("ffplay probe spawn");
        // ~6s of buffered audio, then a 15s read timeout before the first
        // reconnect: look for the second connection by +30s.
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline && accepts.load(Ordering::Relaxed) < 2 {
            std::thread::sleep(Duration::from_millis(200));
        }
        let served = accepts.load(Ordering::Relaxed);
        assert!(
            served >= 2,
            "read timeout never drove a reconnect (connections served: {served})"
        );
        assert!(
            ExternalPlayer::is_alive(player.pid, player.start_time),
            "ffplay exited during the stall/reconnect cycle"
        );
        stop.store(true, Ordering::Relaxed);
        // Same explicit cleanup as the drop probe.
        ExternalPlayer::stop(player.pid, player.start_time);
    }
}
