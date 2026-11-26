# yt-dlp, ffmpeg and ffplay Come From PATH

Status: Accepted. Supersedes ADR-0009 and the auto-provisioning in ADR-0005.

## Context

`lum yt` runs yt-dlp for every command and needs ffmpeg for `video`, where yt-dlp merges video and audio with it. `lum radio` plays through ffplay. When one of them was not on `PATH`, lum downloaded it into `data_dir()/deps/`: yt-dlp from its latest GitHub release, checked again at most once a day (ADR-0005), and ffmpeg from BtbN's builds on Linux and Windows and from martin-riedl.de's on macOS, downloaded again after 14 days, with ffplay taken from the same archive when it held one (ADR-0009). The downloads were trusted over HTTPS, without checksums.

Two gaps showed in that path:

- lum started yt-dlp without `--ffmpeg-location`, and yt-dlp then runs a bare `ffmpeg` from `PATH`. On Linux and macOS the copy lum downloaded for `lum yt video` never reached it.
- The macOS archive holds no ffplay, so radio on macOS needed a user-installed one anyway.

## Decision

lum never downloads yt-dlp, ffmpeg or ffplay. It looks each one up on `PATH` with `which`, as it does for git and brdump (ADR-0035), and a command that needs a missing one fails before it does anything:

- Every `lum yt` command needs yt-dlp.
- `lum yt video` also needs ffmpeg. `audio`, `album` and `rss` do not.
- `lum radio play` and `random` need ffplay, and fail before they stop the current station or start a runner. The runner looks ffplay up again before every attempt.

## Consequences

- `src/ffmpeg.rs` and `src/yt/deps.rs` are gone, with their state files, staleness rules and archive extraction, and the `LUM_FFMPEG_DISABLE_AUTO_PROVISION`, `LUM_FFMPEG_TEST_ARTIFACT` and `LUM_YT_DLP_TEST_ARTIFACT` variables. `lzma-rust2` stays for the tar.xz archives of `lum tools`.
- Whatever installed yt-dlp keeps it current: Homebrew, a distribution package, Scoop or pip. An outdated yt-dlp fails against YouTube until it is updated.
- A machine without these programs needs them installed first. ffplay ships with most ffmpeg packages, Homebrew's included.
- Copies lum downloaded earlier stay in `data_dir()/deps/`, unused; delete the folder by hand.
