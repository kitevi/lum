# Radio Subcommand

`lum radio` reimplements `ruv` as a Rust subcommand while delegating playback to `ffplay`.

## CLI Shape

- `lum radio list` lists built-in stations one row per station.
- `lum radio play <code>` starts a station in the background, replacing any current station. An unknown code fails and prints the list.
- `lum radio random` starts a random built-in station, skipping the current one.
- `lum radio status` prints the remembered playback state: `Radio: playing`, `Radio: paused`, or `Radio: stopped`.
- `lum radio stop` stops playback and removes the tray.
- Station codes are five lowercase letters, unique within the catalog and stable once published.
- Listings are one aligned row per station: `code  name  tag, tag`.

## Station Catalog

Each entry in `stations.rs` carries a code, a standalone name, a direct stream URL, and a list of tags. Tags (genre, mood, `live`) replace the old genre grouping and long descriptions to keep the listing compact; every station needs at least one.

Names must stand alone because `playing` and `status` print only the code and name. Retiring a station needs no state migration: `status` prints the remembered name, and `random` excludes a remembered code whether or not it is still in the catalog.

The RadCap entries use raw IP URLs, so they are the catalog entries most likely to stop working when a host moves. Check them before assuming a playback failure is local.

## Random Station Selection

`random` draws from the catalog:

- The pool is every station in `stations::all()`, drawn uniformly.
- The station remembered in `radio-player.json` is excluded by code, whether or not its process is still alive. Resolution reads that state before playback is stopped, so a failed draw leaves the current station alone.
- If exclusion would empty the pool, resolution fails rather than replaying the current station.

## Playback Stack

Radio playback runs `ffplay`, which comes with ffmpeg:

- A station runs under a hidden `__radio_direct_runner` supervisor process that loops `ffplay` until stopped.
- `ffplay` plays the station's stream URL directly; radio never resolves a page URL.

`ffplay` recovery arguments (see ADR-0014):

- `-autoexit` so the supervisor can observe the end of an attempt and decide to respawn.
- `-rw_timeout 15000000` so a connection that delivers no data for 15 seconds errors out instead of blocking forever.
- Every station gets `-reconnect 1 -reconnect_streamed 1 -reconnect_delay_max 5`, which heals dropped and stalled connections in-process without leaving silence.
- Every station also gets `-infbuf -af adelay=delays=5000:all=1` (see ADR-0020): 5s of leading silence while an uncapped input queue fills, so streams that arrive at exactly realtime (the RadCap hosts) can jitter without audible gaps.
- ffplay runs at `-loglevel warning`, and the runner forwards its stderr to `lum.log` as `warn` events. Healthy streams log nothing.

`ffplay` comes from `$PATH` only; lum never downloads it (ADR-0036). Without it, `play` and `random` fail before they stop the current station or start a runner, and the runner looks it up again before every attempt.

The old pure-Rust foreground audio path is no longer the product direction for `lum radio`. Do not reintroduce CPAL/Symphonia/ring-buffer terminal playback unless an ADR reverses this decision.

## Tray

While a station runner is alive it shows a system tray icon (ADR-0022) whose menu holds the `Stop <station>` / `Play <station>` toggle, a `Stations` submenu, `Random station` and `Quit radio`. The tray is an extra, not a requirement: if it cannot be created, the runner logs a warning and keeps playing.

- The runner's main thread owns the tray and every menu object; the supervisor runs on a worker thread that starts first. They share only `Playback` (`playback.rs`): the pause flag and the running ffplay. Never touch tray or menu objects from the worker: macOS requires the main thread, and release builds use `panic = "abort"`, so a panic would take playback down with it.
- Stop kills ffplay and leaves the runner idle. Play respawns it with a fresh failure streak. A pause is not a stream failure: it never backs off or counts toward giving up. Backoff sleeps wake early on a pause. Resume rejoins the live stream, so ADR-0020's 5s of leading silence applies again.
- The tray stays visible while paused, because that is how Play works. `lum radio stop`, `Quit radio`, starting another station, or the runner giving up removes it.
- `Stations` lists the whole catalog in `lum radio list` order; the current station is checked and inert. Picking another one, or `Random station` (same rules as `lum radio random`), does what the CLI does without a terminal: the runner starts the new station's runner, records it in `radio-player.json`, then silences itself and exits. The successor starts first, so a failed start leaves the current station playing. The new runner brings its own tray; the menu of a live tray never changes station.
- `Quit radio` is `lum radio stop` from the tray: the runner removes the state file if it still names its own PID, kills its ffplay and exits, so `status` reports `stopped`.
- These requests run on the tray's thread (`handle_tray_request` in `mod.rs`). The runner then returns from `main` without joining the supervisor, which `Playback::halt` has parked for good; do not make the supervisor loops learn a second way to end.
- The runner records `paused` in `radio-player.json` so `status` stays truthful; see Runtime Semantics.
- While the status tray is up (`lum display off` on macOS, `lum net on` on Linux and macOS), the menu starts with its lines and the icon becomes a monitor with the radio over its corner, the screen filled in for shaping. How the tray is hosted, built and drawn is in `src/tray/README.md`.
- Windows caveat: `lum radio stop` ends the runner with TerminateProcess, and Windows can leave the icon of a killed process in the notification area until the mouse passes over it. A graceful stop path would fix that if it matters.

## Supported Streams

Built-in stations are passed to `ffplay` as URLs. Some RadCap and NTS endpoints serve ADTS AAC (`audio/aacp`); `ffplay` decodes it with no extra configuration.

Out of scope unless a real station requires it:

- user-configurable stations
- station aliases
- custom audio decoding inside lum

## Runtime Semantics

Controls are process-backed, not terminal-key backed:

- Stations remember the supervisor runner process; `stop` kills the runner tree (runner plus its ffplay child) and clears state.
- `status` reports `playing` while the remembered runner process is alive and not paused, meaning it is actively playing or retrying with backoff, and `paused` while the tray has playback stopped.
- After 5 consecutive short-lived playback attempts (a session shorter than 60 seconds resets the streak), the runner gives up, sends a desktop notification, logs the failure, and exits; the next `status` then reports `stopped`.
- Runner diagnostics (attempt failures, respawn decisions, ffplay warnings) go to the rotating `lum.log` in lum's log directory, not the terminal, because the runner runs detached with nulled stdio.
- Starting a new station stops the remembered process and starts the new station.

State is stored in lum's platform-native state directory as `radio-player.json`, including the remembered runner PID and process start time when available, and `paused`. The runner itself updates `paused` when the tray toggles, only if the file names its own PID, and every write replaces the file atomically so a concurrent `status` never reads half a file. Legacy state files written before runners existed remember an ffplay PID directly and still work; files without `paused` read as not paused.

User-facing output remains plain/script-friendly. `play` prints `playing <code> <description>` and `stop` prints `stopped`; `status` prints one of:

- `Radio: playing <code> <description>`
- `Radio: paused <code> <description>`
- `Radio: stopped`

## Tests

Run normal tests:

```sh
cargo test --workspace
```

Playback-recovery probes need `ffplay`, `ffmpeg`, and an audio output device, so they are opt-in:

```sh
LUM_RADIO_PLAYBACK_TESTS=1 cargo test radio::
```

They drive the real ffplay spawn path against a local fixture server that drops, stalls, or 404s connections, and assert the recovery behavior (transparent reconnect, runner respawn).

Manual live-stream testing requires `ffplay`:

```sh
cargo run -- radio play space
cargo run -- radio status
cargo run -- radio stop
```

Pausing is covered by ordinary unit tests (`Playback`, and a supervisor that must never count a pause as a failure) plus an opt-in probe in the same `LUM_RADIO_PLAYBACK_TESTS=1` set, which drives a real ffplay through pause/resume cycles. The tray itself needs a desktop session with a tray host, so it is verified by hand:

```sh
cargo run -- radio play space
cargo run -- radio status   # Radio: playing space SomaFM Deep Space One
# A tray icon appears. Click its "Stop ..." item: audio stops and the item reads "Play ...".
cargo run -- radio status   # Radio: paused space SomaFM Deep Space One
# Click again: audio returns.
# Pick another station under "Stations": the icon is replaced and status names the new station.
# "Random station" does the same with a station other than the current one.
cargo run -- radio stop     # the tray icon disappears; so does it after "Quit radio"
```

To try the tray without touching your real radio state or speakers, run the commands with `XDG_STATE_HOME` pointing at a scratch directory and `SDL_AUDIODRIVER=dummy`. On Linux the click can be sent over D-Bus: find the item with `busctl --user list | grep StatusNotifierItem`, then call `com.canonical.dbusmenu` `Event` with the item's id and `clicked`. The toggle is id `1`; `GetLayout` prints the ids of the stations, `Random station` and `Quit radio`.

Verify random selection: play any station, then confirm `random` never repeats it and that `status` reports the station that was drawn.

```sh
cargo run -- radio random
cargo run -- radio status
cargo run -- radio random
cargo run -- radio stop
```
