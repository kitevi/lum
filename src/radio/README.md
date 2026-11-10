# Radio Subcommand

`lum radio` reimplements `ruv` as a Rust subcommand while delegating playback to `ffplay`.

## CLI Shape

- `lum radio` lists built-in stations one row per station.
- `lum radio list` explicitly lists built-in stations one row per station.
- `lum radio <code>` starts a station in the background, replacing any current station.
- `lum radio rand` starts a random built-in station, skipping the current one.
- `lum radio status` prints the remembered playback state: `playing`, `paused`, or `stopped`.
- `lum radio stop` stops playback and removes the tray.
- Station codes are four lowercase alphanumeric characters, unique within the catalog and stable once published.
- Listings are one aligned row per station: `code  name  tag, tag`.

## Station Catalog

Each entry in `stations.rs` carries a code, a standalone name, a stream URL, a kind, and a list of tags. Tags (genre, mood, `live`, `playlist`) replace the old genre grouping and long descriptions to keep the listing compact; every station needs at least one.

Names must stand alone because `playing` and `status` print only the code and name. Retiring a station needs no state migration: `status` prints the remembered name, and `rand` excludes a remembered code whether or not it is still in the catalog.

The RadCap entries use raw IP URLs, so they are the catalog entries most likely to stop working when a host moves. Check them before assuming a playback failure is local.

## Random Station Selection

`rand` is a reserved selector code, not a catalog entry:

- The pool is every station in `stations::all()`, drawn uniformly. Station kind is ignored, so YouTube and playlist stations are eligible.
- The station remembered in `radio-player.json` is excluded by code, whether or not its process is still alive. Resolution reads that state before playback is stopped, so a failed draw leaves the current station alone.
- If exclusion would empty the pool, resolution fails rather than replaying the current station.
- `lum radio list` prints the station table, then a short block of non-station commands that includes `rand`. Unknown-code errors print the same reference.
- The selector arm is matched before the station catch-all, so the selector always wins if a catalog entry is ever named `rand`.

## Playback Stack

Radio playback uses the existing ffmpeg dependency path:

- Direct and YouTube stations run under a hidden `__radio_direct_runner` supervisor process that loops `ffplay` until stopped.
- `yt-dlp` resolves YouTube live station pages to stream URLs; the runner re-resolves on every playback attempt so expired stream URLs heal themselves.
- `ffplay` then plays the resolved YouTube stream URL.
- Playlist stations keep the separate `__radio_playlist_runner` loop, which must see track EOF to advance and therefore does not use the reconnect flags.

`ffplay` recovery arguments (see ADR-0014):

- `-autoexit` so the supervisor can observe the end of an attempt and decide to respawn.
- `-rw_timeout 15000000` so a connection that delivers no data for 15 seconds errors out instead of blocking forever.
- Live stations additionally get `-reconnect 1 -reconnect_streamed 1 -reconnect_delay_max 5`, which heals dropped and stalled connections in-process without leaving silence.
- Live stations also get `-infbuf -af adelay=delays=5000:all=1` (see ADR-0020): 5s of leading silence while an uncapped input queue fills, so streams that arrive at exactly realtime (the RadCap hosts) can jitter without audible gaps.
- ffplay runs at `-loglevel warning`, and the runner forwards its stderr to `lum.log` as `warn` events. Healthy streams log nothing.

`ffplay` is preferred from `$PATH`. If it is not on `$PATH`, lum looks for a provisioned `ffplay` next to its managed ffmpeg binary.

The old pure-Rust foreground audio path is no longer the product direction for `lum radio`. Do not reintroduce CPAL/Symphonia/ring-buffer terminal playback unless an ADR reverses this decision.

## Tray

While a station runner is alive it shows a system tray icon (ADR-0022) whose menu holds the `Stop <station>` / `Play <station>` toggle, a `Stations` submenu, `Random station` and `Quit radio` (ADR-0023). The tray is an extra, not a requirement: if it cannot be created, the runner logs a warning and keeps playing.

- The runner's main thread owns the tray and every menu object; the supervisor runs on a worker thread that starts first. They share only `Playback` (`playback.rs`): the pause flag and the running ffplay. Never touch tray or menu objects from the worker: macOS requires the main thread, and release builds use `panic = "abort"`, so a panic would take playback down with it.
- Stop kills ffplay and leaves the runner idle. Play respawns it with a fresh failure streak. A pause is not a stream failure: it never backs off or counts toward giving up. Backoff sleeps wake early on a pause. Playlist stations replay the current track on resume. Resume rejoins the live stream, so ADR-0020's 5s of leading silence applies again.
- The tray stays visible while paused, because that is how Play works. `lum radio stop`, `Quit radio`, starting another station, or the runner giving up removes it.
- `Stations` lists the whole catalog in `lum radio list` order; the current station is checked and inert. Picking another one, or `Random station` (same rules as `lum radio rand`), does what the CLI does without a terminal: the runner starts the new station's runner, records it in `radio-player.json`, then silences itself and exits. The successor starts first, so a failed start leaves the current station playing. The new runner brings its own tray; the menu of a live tray never changes station.
- `Quit radio` is `lum radio stop` from the tray: the runner removes the state file if it still names its own PID, kills its ffplay and exits, so `status` reports `stopped`.
- These requests run on the tray's thread (`handle_tray_request` in `mod.rs`). The runner then returns from `main` without joining the supervisor, which `Playback::halt` has parked for good; do not make the supervisor loops learn a second way to end.
- The runner records `paused` in `radio-player.json` so `status` stays truthful; see Runtime Semantics.
- Linux uses tray-icon's `ksni` backend: StatusNotifierItem over D-Bus, pure Rust, no GTK. It needs a tray host. KDE, Xfce and most bars have one; stock GNOME needs the AppIndicator extension. Without a host the icon is simply invisible. Do not switch to tray-icon's default backend: it links GTK 3, and then every lum command would need GTK 3 installed just to start.
- Windows and macOS run a `winit` event loop on the main thread (`tray.rs`). macOS uses the Accessory activation policy, so there is no Dock icon.
- The icon is a small radio drawn in code (`icon.rs`), so there is no image asset to ship. Its antenna is up while playing and folded away while paused, so the state shows in the silhouette and not only in the colour (a template has no colour, and green against grey is a weak cue for colour-blind users).
- The icon has to read on any panel, light or dark, because the host paints it over a panel we cannot see. The colour ink uses mid-tone fills, and a test holds them above WCAG's 3:1 contrast floor for graphics against real dark and light panels. The speaker and knobs are holes, so they take the panel's own colour, and a faint white rim around the outside keeps the icon visible on mid-tone panels while vanishing on light ones. macOS gets a black template image instead, which the system tints for the menu bar (`with_icon_templated`); that is why the state also changes the shape and, in the template, the opacity.
- The geometry sits on a 16-unit grid so the main edges stay crisp at the common 16 px tray size. There is no preview tool in the repo: when you change the drawing, composite `icon::rgba` over a dark and a light panel at 16, 24 and 32 px and look at it; thin shapes that look fine at 64 px turn to mush at 16.
- Clicking the icon itself is not a portable gesture across tray hosts, which is why everything is a menu item.
- Windows caveat: `lum radio stop` ends the runner with TerminateProcess, and Windows can leave the icon of a killed process in the notification area until the mouse passes over it. A graceful stop path would fix that if it matters.

## Supported Streams

Built-in direct stations are passed to `ffplay` as URLs. YouTube live stations are supported when yt-dlp can resolve the page URL and ffplay can play the resulting stream or HLS playlist. Some RadCap and NTS endpoints serve ADTS AAC (`audio/aacp`); `ffplay` decodes it with no extra configuration.

The `ytlf` station points at the channel `/live` URL (`https://www.youtube.com/@LofiGirl/live`), so yt-dlp resolves whatever Lofi Girl currently features live and the station survives their 24/7 stream rotations. Do not point it back at a `watch?v=` video URL: those video IDs rot each time the channel rotates its streams.

Out of scope unless a real station requires it:

- user-configurable stations
- station aliases
- custom audio decoding inside lum

## Runtime Semantics

Controls are process-backed, not terminal-key backed:

- Direct and YouTube stations remember the supervisor runner process; `stop` kills the runner tree (runner plus its ffplay child) and clears state.
- `status` reports `playing` while the remembered runner process is alive and not paused, meaning it is actively playing or retrying with backoff, and `paused` while the tray has playback stopped.
- After 5 consecutive short-lived playback attempts (a session shorter than 60 seconds resets the streak), the runner gives up, sends a desktop notification, logs the failure, and exits; the next `status` then reports `stopped`. Failed yt-dlp resolutions count as failed attempts and back off like any other failure.
- Runner diagnostics (attempt failures, respawn decisions, ffplay warnings) go to the rotating `lum.log` in lum's log directory, not the terminal, because the runner runs detached with nulled stdio.
- Starting a new station stops the remembered process and starts the new station.
- Playlist stations keep the pre-existing runner semantics, plus the tray pause.

State is stored in lum's platform-native state directory as `radio-player.json`, including the remembered runner PID and process start time when available, and `paused`. The runner itself updates `paused` when the tray toggles, only if the file names its own PID, and every write replaces the file atomically so a concurrent `status` never reads half a file. Legacy state files written before runners existed remember an ffplay PID directly and still work; files without `paused` read as not paused.

User-facing output remains plain/script-friendly:

- `playing <code> <description>`
- `paused <code> <description>`
- `stopped`

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

Manual live-stream testing requires `ffplay` and, for YouTube stations, `yt-dlp`:

```sh
cargo run -- radio dron
cargo run -- radio status
cargo run -- radio stop
```

Pausing is covered by ordinary unit tests (`Playback`, and a supervisor that must never count a pause as a failure) plus an opt-in probe in the same `LUM_RADIO_PLAYBACK_TESTS=1` set, which drives a real ffplay through pause/resume cycles. The tray itself needs a desktop session with a tray host, so it is verified by hand:

```sh
cargo run -- radio dron
cargo run -- radio status   # playing dron SomaFM Drone Zone
# A tray icon appears. Click its "Stop ..." item: audio stops and the item reads "Play ...".
cargo run -- radio status   # paused dron SomaFM Drone Zone
# Click again: audio returns.
# Pick another station under "Stations": the icon is replaced and status names the new station.
# "Random station" does the same with a station other than the current one.
cargo run -- radio stop     # the tray icon disappears; so does it after "Quit radio"
```

To try the tray without touching your real radio state or speakers, run the commands with `XDG_STATE_HOME` pointing at a scratch directory and `SDL_AUDIODRIVER=dummy`. On Linux the click can be sent over D-Bus: find the item with `busctl --user list | grep StatusNotifierItem`, then call `com.canonical.dbusmenu` `Event` with the item's id and `clicked`. The toggle is id `1`; `GetLayout` prints the ids of the stations, `Random station` and `Quit radio`.

Verify random selection: play any station, then confirm `rand` never repeats it and that `status` reports the station that was drawn.

```sh
cargo run -- radio rand
cargo run -- radio status
cargo run -- radio rand
cargo run -- radio stop
```
