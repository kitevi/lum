# Yt RSS Resolves Channel Feed URL

## Status

Accepted

## Context

Resolving a YouTube link to its channel RSS feed was a repeated manual workflow: take a video or channel link, find the canonical `channel_id` (`UC...`, 24 characters) and build `https://www.youtube.com/feeds/videos.xml?channel_id=<id>`. It was done by curling channel HTML and grepping for `"channelId":"UC..."`, which is brittle (YouTube markup changes, JS-heavy pages, crawler blocks) and makes the user know which meta field to trust.

`lum yt` already shells out to `yt-dlp` with auto-provisioning (ADR-0005), and yt-dlp understands every URL shape involved, so reusing it needs no new dependency (`AGENTS.md`: prefer dependencies over own code).

## Decision

`lum yt rss [--id-only] [--videos-only] <URL...>` prints the channel RSS feed URL for each video or channel link, one line per link, in input order.

- **Input is classified before yt-dlp runs**, with the `url` crate already in the tree. Accepted are video links (`watch?v=`, `youtu.be/<id>`, `/shorts/`, `/live/`, `/embed/`, `/v/`, also on `youtube-nocookie.com`) and channel links (`/channel/UC...`, `/@handle`, `/c/<name>`, `/user/<name>`). Playlist links are rejected, even a video link that carries `&list=`: the command resolves a link to its channel, and playlist feeds are out of scope. Anything else, such as another host, a bare `UC...` ID, a bare handle or a bare video ID, is rejected as unsupported. Neither case calls yt-dlp.
- **One fetch resolves the channel:** `yt-dlp --no-download --no-warnings --no-progress --flat-playlist -J --playlist-items 1 -- <URL>`, which reads a single video page or a single channel-tab page. In flat mode a channel tab reports each entry's `channel_id` as `NA`, so the top-level `channel_id` is what channel links resolve through, and the entries are still scanned.
- **Exactly one distinct `UC...` ID must come out.** `NA` and missing values are dropped first. None is an error (unsupported, private or deleted). Two or more mean a video with several owners, which is an error that asks for a direct channel link.
- **Fail-fast.** Links are resolved in order and each success is printed as it resolves. The first failure stops the run with a non-zero exit, so a pipe can trust the exit code.
- **Output:** the `channel_id` feed, with videos, shorts and live, by default. `--videos-only` prints the long-form-only feed, through the playlist ID `UULF` + `channel_id[2..]`, a string transform that needs no further request. `--id-only` prints the bare channel ID, or the playlist ID together with `--videos-only`.
- **lum never fetches or parses the feed, and uses no API key.** Printing the URL is the contract, and the reader validates it.

`--videos-only` relies on an auto-playlist that YouTube does not document. It was found by trying and works as of 2026, and its feed title is just `Videos`. If YouTube drops it, `--videos-only` breaks at the reader, not in lum, and the default feed stays the stable path.

## Alternatives considered

- **Scraping channel HTML in Rust**, with or without a new scraper crate: duplicates what yt-dlp already does, breaks on markup changes and needs handling for every URL shape. ADR-0005 keeps that maintenance out of lum.
- **The YouTube Data API v3** (`channels.list`): needs an API key, quota and an auth surface for a two-line derivation.
- **oEmbed:** returns the author's name and URL, not the `channel_id`.
- **The name `feed` instead of `rss`:** `rss` matches how people say it, and `feed` would be a viable alias. None is added, to keep the surface minimal.
- **A top-level `lum rss`:** the need is YouTube-only, so `lum yt rss` keeps YouTube logic in `src/yt/`. Revisit if feeds from elsewhere appear.
- **`--shorts-only` and `--live-only`**, the `UUSH` and `UULV` auto-playlists with the same transform: deferred until someone asks. One flag is enough for now.

## Consequences

- `src/yt/rss.rs` holds the classification, the ID check, the feed builders and the yt-dlp call, and `src/yt/mod.rs` dispatches to it. Nothing is downloaded, so ffmpeg is not needed.
- There is no new dependency, and the same yt-dlp call works on Linux, macOS and Windows. `audio`, `video` and `album` are untouched.
- Unit tests cover the classification, the ID check, the `UULF` transform and the dedupe and multi-owner logic. `tests/yt_cli.rs` runs the command against a fake yt-dlp, including that a playlist never reaches it and that the run stops at the first failure. A live test that fetches both feeds is ignored by default.
- Out of scope: reading feed contents, polling, OPML, playlist feeds, clipboard or `--open` output, and URLs on stdin.
