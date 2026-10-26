# Radio Subcommand

`lum radio` reimplements `ruv` as a Rust subcommand while delegating playback to `ffplay`.

## CLI Shape

- `lum radio` lists built-in stations grouped by genre.
- `lum radio list` explicitly lists built-in stations grouped by genre.
- `lum radio <code>` starts a station in the background, replacing any current station.
- `lum radio rand` starts a random built-in station, skipping the current one.
- `lum radio status` prints the remembered playback state.
- `lum radio stop` stops playback.
- Station codes are four lowercase alphanumeric characters, unique within the catalog and stable once published.
- Listings keep the plain output style, with each `code  description` row indented under its genre heading.

## Station Catalog

Each entry in `stations.rs` carries a code, a standalone description, a stream URL, a kind, and a genre. The genre is catalog data, not a display convention: `format_listing` groups every non-empty genre under its label in this order:

Electroacoustic and concrète, Experimental and avant-garde, Minimal and reductionist, Drone, Dark ambient, Space and ambient, Field recordings, Experimental electronic, World, Lo-fi.

Descriptions must stand alone because `playing` and `status` print only the code and description. Retiring a station needs no state migration: `status` prints the remembered description, and `rand` excludes a remembered code whether or not it is still in the catalog.

The RadCap entries use raw IP URLs, so they are the catalog entries most likely to stop working when a host moves. Check them before assuming a playback failure is local.

## Random Station Selection

`rand` is a reserved selector code, not a catalog entry:

- The pool is every station in `stations::all()`, drawn uniformly. Station kind is ignored, so YouTube and playlist stations are eligible.
- The station remembered in `radio-player.json` is excluded by code, whether or not its process is still alive. Resolution reads that state before playback is stopped, so a failed draw leaves the current station alone.
- If exclusion would empty the pool, resolution fails rather than replaying the current station.
- `lum radio list` prints the genre-grouped station table, then a short block of non-station commands that includes `rand`. Unknown-code errors print the same reference.
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
- `status` reports `playing` while the remembered runner process is alive, meaning it is actively playing or retrying with backoff.
- After 5 consecutive short-lived playback attempts (a session shorter than 60 seconds resets the streak), the runner gives up, sends a desktop notification, logs the failure, and exits; the next `status` then reports `stopped`. Failed yt-dlp resolutions count as failed attempts and back off like any other failure.
- Runner diagnostics (attempt failures, respawn decisions, ffplay warnings) go to the rotating `lum.log` in lum's log directory, not the terminal, because the runner runs detached with nulled stdio.
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
cargo run -- radio dron
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
