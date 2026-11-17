# Radio Plays Direct Streams Only

Status: Accepted

## Context

`lum radio` grew a second playback path for YouTube-backed stations. yt-dlp resolved a
channel or video page URL to a media URL before every ffplay attempt (ADR-0008), and a
separate playlist runner existed only to advance through per-track resolutions. That path
carried radio's only use of yt-dlp, its own retry semantics (resolution failures feeding the
respawn backoff, ADR-0014), the base ffplay command without reconnect flags (it had to observe
track EOF), and a hidden subcommand, `__radio_playlist_runner`.

Only two catalog entries used it: `ytlf` (the Lofi Girl channel `/live` page) and `aphx` (two
Aphex Twin album videos). Every other station is already a direct stream URL.

## Decision

Radio supports direct stream URLs only.

- A station is a URL `ffplay` can play; radio never resolves a page URL.
- `StationKind` and the `kind` field are removed. `Direct` was the only value left, and random
  selection already ignored it.
- The yt-dlp resolution path, the playlist runner, and the `ytlf` and `aphx` stations are removed.
  `__radio_direct_runner` is the only radio runner left.
- `yt-dlp` stays in the project for `lum yt` (ADR-0005, ADR-0013); it is no longer a radio
  dependency.
- The radio state enum keeps a `PlaylistRunner` variant so a state file written by the retired
  runner still parses: `status` then reports `stopped` once that process is gone, and `stop`
  still works. This follows the existing tolerance for retired station codes and legacy
  `Ffplay` state.

## Consequences

- The catalog is 15 direct stations. Losing `ytlf` and `aphx` loses the 24/7 lo-fi channel and
  the album-length curated sets; a replacement has to be a direct stream (an Icecast/MP3 URL),
  not a video page.
- Radio has one playback path: supervisor plus ffplay, with the reconnect and buffering
  arguments from ADR-0014 and ADR-0020 unchanged and now applied to every station.
- Re-adding video-page stations means reversing this ADR, not just adding a URL to
  `stations.rs`; the catalog has no field that could express one.
