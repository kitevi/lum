# Yt Subcommand

`lum yt` downloads audio, video, or albums from YouTube using yt-dlp as a thin wrapper. It constructs the right arguments for each mode and shells out to yt-dlp directly.

## CLI Shape

```sh
lum yt audio <URL...>              # Audio → ~/Music
lum yt video [--height N] <URL...> # Video → ~/Movies
lum yt album <URL...>              # Album/playlist → ~/Music
lum yt rss [--id-only] [--videos-only] <URL...>  # Channel RSS feed URL(s)
```

## Dependencies

Both come from `$PATH`; lum never downloads them (ADR-0036). A command that needs a missing one fails before it does anything.

- **yt-dlp** — required by every command.
- **ffmpeg** — required by `video`, where yt-dlp merges video and audio with it. `audio`, `album` and `rss` don't need it.

Earlier versions downloaded both into `data_dir()/deps/`. lum ignores those copies now; delete the folder by hand.

## Architecture

- `rss.rs` — video/channel classification (playlists rejected), channel-ID resolution via a single `yt-dlp -J --flat-playlist` fetch, and feed-URL builders (per-channel `channel_id` feed plus long-form-only `UULF` playlist feed)
- `mod.rs` — CLI dispatch, the yt-dlp and ffmpeg lookups on `$PATH`, yt-dlp invocation with `-P` for output directory
- `args.rs` — argument construction for each subcommand (base flags, format selectors, output templates, metadata cleanup)

## Album folder naming

`lum yt album` builds the output folder from the album title. Bandcamp albums are prefixed with the artist (`Artist - Album`) because yt-dlp exposes the real artist per track (`album_artist`/`artist`). YouTube playlists keep the bare album title, because YouTube's per-entry `uploader` is the channel name, not the artist. Detection is by `bandcamp.com` in the URL (`has_bandcamp` in `args.rs`); if any URL is Bandcamp, the artist-prefixed template is used.

## What We Dropped vs the Go Version

- **aria2c** — replaced by `--concurrent-fragments 8`. YouTube uses DASH fragments; aria2c's per-file split settings were ineffective.
- **Screen height detection** — replaced by a default 1080p ceiling with optional `--height` override.
- **`--tmp` flag** — removed. Use `--simulate` or run yt-dlp directly for testing.
- **`os.Chdir`** — replaced by yt-dlp's `-P` flag to set output path without process-wide side effects.
