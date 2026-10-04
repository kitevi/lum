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

- `ffplay` does audio playback for direct streams.
- `yt-dlp` resolves YouTube live station pages to stream URLs.
- `ffplay` then plays the resolved YouTube stream URL.

`ffplay` is preferred from `$PATH`. If it is not on `$PATH`, lum looks for a provisioned `ffplay` next to its managed ffmpeg binary.

The old pure-Rust foreground audio path is no longer the product direction for `lum radio`. Do not reintroduce CPAL/Symphonia/ring-buffer terminal playback unless an ADR reverses this decision.

## Supported Streams

Built-in direct stations are passed to `ffplay` as URLs. YouTube live stations are supported when yt-dlp can resolve the page URL and ffplay can play the resulting stream or HLS playlist.

Out of scope unless a real station requires it:

- user-configurable stations
- station aliases
- custom audio decoding inside lum

## Runtime Semantics

Controls are process-backed, not terminal-key backed:

- `stop` kills the remembered `ffplay` process only if the current process still matches ffplay identity/start-time state, then clears state.
- Starting a new station stops the remembered process and starts the new station.

State is stored in lum's platform-native state directory as `radio-player.json`, including the remembered ffplay PID and process start time when available.

User-facing output remains plain/script-friendly:

- `playing <code> <description>`
- `stopped`

## Tests

Run normal tests:

```sh
cargo test --workspace
```

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
