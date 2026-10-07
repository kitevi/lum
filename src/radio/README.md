# Radio Subcommand

`lum radio` reimplements `ruv` as a Rust subcommand while delegating playback to `ffplay`.

## CLI Shape

- `lum radio` lists built-in stations.
- `lum radio list` explicitly lists built-in stations.
- `lum radio <code>` starts a station in the background, replacing any current station.
- `lum radio rand` starts a random built-in station, skipping the current one.
- `lum radio status` prints the remembered playback state.
- `lum radio stop` stops playback.
- Preserve existing `ruv` station codes and the plain output style.

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

`ffplay` is preferred from `$PATH`. If it is not on `$PATH`, lum looks for a provisioned `ffplay` next to its managed ffmpeg binary.

The old pure-Rust foreground audio path is no longer the product direction for `lum radio`. Do not reintroduce CPAL/Symphonia/ring-buffer terminal playback unless an ADR reverses this decision.

## Supported Streams

Built-in direct stations are passed to `ffplay` as URLs. YouTube live stations are supported when yt-dlp can resolve the page URL and ffplay can play the resulting stream or HLS playlist.

The `ytlf` station points at the channel `/live` URL (`https://www.youtube.com/@LofiGirl/live`), so yt-dlp resolves whatever Lofi Girl currently features live and the station survives their 24/7 stream rotations. Do not point it back at a `watch?v=` video URL: those video IDs rot each time the channel rotates its streams.

Out of scope unless a real station requires it:

- user-configurable stations
- station aliases
- custom audio decoding inside lum

## Runtime Semantics

Controls are process-backed, not terminal-key backed:

- Direct and YouTube stations remember the supervisor runner process; `stop` kills the runner tree (runner plus its ffplay child) and clears state.
- `status` reports `playing` while the remembered runner process is alive, meaning it is actively playing or retrying with backoff.
- After 5 consecutive short-lived playback attempts (a session shorter than 60 seconds resets the streak), the runner gives up, sends a desktop notification, logs the failure, and exits; the next `status` then reports `stopped`. Failed yt-dlp resolutions count as failed attempts and back off like any other failure.
- Runner diagnostics (attempt failures, respawn decisions) go to the rotating `lum.log` in lum's log directory, not the terminal, because the runner runs detached with nulled stdio.
- Starting a new station stops the remembered process and starts the new station.
- Playlist stations keep the pre-existing runner semantics.

State is stored in lum's platform-native state directory as `radio-player.json`, including the remembered runner PID and process start time when available. Legacy state files written before runners existed remember an ffplay PID directly and still work.

User-facing output remains plain/script-friendly:

- `playing <code> <description>`
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
cargo run -- radio atma
cargo run -- radio status
cargo run -- radio stop
```

Verify random selection: play any station, then confirm `rand` never repeats it and that `status` reports the station that was drawn.

```sh
cargo run -- radio rand
cargo run -- radio status
cargo run -- radio rand
cargo run -- radio stop
```
