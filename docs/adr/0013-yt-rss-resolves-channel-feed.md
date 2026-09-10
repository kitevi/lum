# Yt RSS Resolves Channel Feed URL

## Status

Accepted

## Context

Resolving a YouTube link to its channel RSS feed is a repeated manual workflow:

1. Take a YouTube **video or channel** link (channel page, `@handle`, `watch?v=`, `youtu.be`, `/shorts/`, `/live/`). Playlists and anything that is not a video-or-channel link is rejected.
2. Find the canonical `channel_id` (`UC...`, 24 chars).
3. Build `https://www.youtube.com/feeds/videos.xml?channel_id=<id>`.

Today this is done by curling channel HTML and grepping for
`"channelId":"UC..."` / `"externalId":"UC..."`. That scrape is brittle
(YouTube markup changes, JS-heavy pages, crawler blocks) and requires the
user to know which meta field to trust.

Example from real usage:

- Input: `https://www.youtube.com/@phoboukaideimou/videos`
- Channel ID: `UCOC7Er4tw8VhkdB2D0wz2_A`
- Feed: `https://www.youtube.com/feeds/videos.xml?channel_id=UCOC7Er4tw8VhkdB2D0wz2_A`
- Verified: feed returns Atom with `<yt:channelId>`, latest entry
  `yt:video:_aCA6vWGTno` (`Anatomy Of A Fall - Video Essay SPOILERS`).

`lum yt` already owns the YouTube-resolution problem. It shells out to `yt-dlp`
as a thin wrapper with auto-provisioning (ADR-0005). Reusing that path makes
RSS resolution robust for every URL shape yt-dlp understands, with zero new
dependencies (per `AGENTS.md`: prefer dependencies over own code,
cross-platform, check blessed.rs first — no new crate needed here).

## Decision (proposed)

Add a fourth `lum yt` subcommand:

```sh
lum yt rss [--id-only] [--videos-only] <URL...>   # Channel RSS feed URL(s) -> stdout
```

Parallel to existing shape (`aud` / `vid` / `alb` each take `urls: Vec<String>`
with `required = true`):

```rust
// src/cli.rs
pub enum YtCommand {
    Aud { urls: Vec<String> },
    Vid { height: Option<u32>, urls: Vec<String> },
    Alb { urls: Vec<String> },
    /// Print channel RSS feed URL(s) for YouTube video or channel URL(s).
    /// --videos-only prints the long-form-videos-only feed (no shorts/live).
    Rss {
        /// Print bare channel ID(s) instead of feed URL(s).
        /// With --videos-only, prints the playlist ID instead.
        #[usage(long)]
        id_only: bool,
        /// Print the long-form-videos-only feed (excludes shorts and live).
        /// Uses the undocumented auto-playlist: UULF + channel_id[2..].
        #[usage(long)]
        videos_only: bool,
        /// YouTube video or channel URL(s). Playlists rejected.
        /// Accepted: channel, @handle, watch, youtu.be, shorts, live.
        #[usage(required = true)]
        urls: Vec<String>,
    },
}
```

### Behavior

1. Resolve `yt-dlp` via existing `deps::resolve_yt_dlp()` (`$PATH` ->
   auto-provisioned `data_dir()/deps/` -> error). Same UX as `aud`/`vid`/`alb`.
2. Classify each input URL **before** invoking yt-dlp, using the already-present
   `url` crate (no new dependency). Only two kinds are accepted:
   - **video**: `youtube.com/watch?v=`, `youtu.be/<id>`, `/shorts/<id>`,
     `/live/<id>`, `/embed/<id>`, `/v/<id>`.
   - **channel**: `/channel/UC...`, `/@handle[/...]`, `/c/<name>[/...]`,
     `/user/<name>[/...]`.
   - Trailing slashes are ignored; `youtube-nocookie.com/embed/<id>` links are accepted as videos.
   Reject everything else **without** calling yt-dlp:
   - **playlist**: path is `/playlist` or query contains `list=`
     (covers `watch?v=...&list=...`, `playlist?list=...`, bare
     `?list=...`). Error: `playlist links are not supported: <URL> (give a video or channel link instead)`.
   - **non-YouTube host, bare `UC...`, bare `@handle`, bare video ID, empty**:
     error `unsupported YouTube link: <URL> (give a video or channel link instead)`.
   Playlist detection runs on the raw URL string + parsed query so a
   `watch` URL smuggling `&list=` is still rejected as a playlist.
3. For each accepted URL, run (captured, not inherited stdio):

   ```sh
   yt-dlp --no-download --no-warnings --no-progress \
     --flat-playlist -J --playlist-items 1 -- <URL>
   ```

   - One `-J --flat-playlist` fetch covers both kinds: a single video page
     for videos, a single channel-tab page for channels (no per-video
     extraction; `--playlist-items 1` keeps channel tabs to a single page).
   - Channel tabs report per-entry `channel_id` as `NA` in flat mode, so the
     authoritative top-level `channel_id` is what channel URLs resolve
     through (entries are still scanned, so a genuine multi-owner listing
     rejects instead of silently picking one).
4. Collect the top-level `channel_id` plus per-entry `channel_id`s, drop non-IDs (`null`/`NA`/missing), dedupe preserving order.
   - Exactly 1 distinct ID matching `^UC[A-Za-z0-9_-]{22}$` -> success.
   - 0 IDs -> error: `no channel_id found for <URL> (unsupported, private, or deleted?)`.
   - 2+ distinct IDs -> the link resolves to multiple owners/creators.
     Error: `video has multiple owners/creators for <URL> (found: <id1>, <id2>) — give a direct channel link instead`.
     Non-matching values (e.g. `NA`) are dropped before this check, so
     `NA` + one real ID still succeeds.
5. **Fail-fast** (decided): process URLs in input order, printing each success
   to stdout as it resolves. On the first failure, stop — do not resolve any
   later URLs — print the error to stderr and exit non-zero. Earlier successes
   stay on stdout; the exit code signals the run as failed so pipes are safe.
6. Output, one line per resolved URL in input order:
   - default: `https://www.youtube.com/feeds/videos.xml?channel_id={id}`
     (everything: videos + shorts + live).
   - `--videos-only`: `https://www.youtube.com/feeds/videos.xml?playlist_id=UULF{suffix}`
     where `UULF{suffix} = "UULF" + channel_id[2..]` (long-form videos only,
     no shorts/live). Pure string transform, no extra network call.
   - `--id-only`: prints the bare ID used in the feed — `{channel_id}` by
     default, `{playlist_id}` with `--videos-only`.

   Note: the UULF feed is undocumented (discovered by brute force, works as of
   2026) and its `<title>` is the generic string `Videos`, so readers need a
   manual rename. If YouTube ever drops it, `--videos-only` fails at the
   reader, not in lum — the default `channel_id` feed stays the stable path.
   Verified live for `UCOC7Er4tw8VhkdB2D0wz2_A`:
   `playlist_id=UULFOC7Er4tw8VhkdB2D0wz2_A` returns `<yt:playlistId>` with the
   same long-form entries and `<yt:channelId>` intact.
7. No network fetch of the feed itself, no feed XML parsing, no API key.
   Printing the URL is the contract; the reader validates it.

### Examples

```sh
# channel page -> feed
lum yt rss https://www.youtube.com/@phoboukaideimou/videos
# https://www.youtube.com/feeds/videos.xml?channel_id=UCOC7Er4tw8VhkdB2D0wz2_A

# any of these resolve to the same feed:
lum yt rss https://www.youtube.com/@phoboukaideimou
lum yt rss https://www.youtube.com/channel/UCOC7Er4tw8VhkdB2D0wz2_A
lum yt rss https://www.youtube.com/watch?v=_aCA6vWGTno
lum yt rss https://youtu.be/_aCA6vWGTno

# bare ID for scripting
lum yt rss --id-only https://www.youtube.com/@phoboukaideimou/videos
# UCOC7Er4tw8VhkdB2D0wz2_A

# long-form videos only — no shorts, no live
lum yt rss --videos-only https://www.youtube.com/@phoboukaideimou/videos
# https://www.youtube.com/feeds/videos.xml?playlist_id=UULFOC7Er4tw8VhkdB2D0wz2_A
lum yt rss --videos-only --id-only https://www.youtube.com/@phoboukaideimou/videos
# UULFOC7Er4tw8VhkdB2D0wz2_A

# multiples stay ordered, one line each
lum yt rss <URL1> <URL2>

# rejected: playlists (even watch URLs carrying &list=)
lum yt rss "https://www.youtube.com/watch?v=_aCA6vWGTno&list=PL123"
# error: playlist links are not supported (give a video or channel link instead)
lum yt rss "https://www.youtube.com/playlist?list=PL123"
# error: playlist links are not supported (give a video or channel link instead)

# rejected: video with two owners/creators
lum yt rss https://www.youtube.com/watch?v=<collab-video>
# error: video has multiple owners/creators ... — give a direct channel link instead
```

## Alternatives considered

- **Scrape channel HTML in Rust** (`reqwest` + regex for
  `channelId`/`externalId`/`browseId`): rejected. Duplicates what yt-dlp
already does, breaks on markup changes, needs per-URL-shape handling
  (`watch` vs `@handle` vs `youtu.be`), adds maintenance lum explicitly
  avoids (ADR-0005).
- **New HTML-scraper crate**: rejected per `AGENTS.md` (prefer existing
  dependency — yt-dlp is already the blessed YouTube resolver here).
- **YouTube Data API v3** (`channels.list`): rejected. Requires API key
  management, quota, and a new auth surface for a two-line derivation.
- **oEmbed** (`youtube.com/oembed?url=`): rejected. Returns author name/URL,
  not `channel_id`.
- **Standalone `lum rss` top-level command**: rejected for now. RSS need is
  YouTube-scoped today; `lum yt rss` keeps YouTube logic in `src/yt/`.
  Revisit if non-YouTube feeds appear.
- **Name `feed` instead of `rss`**: `rss` matches user vocabulary ("rss feed");
  `feed` is a viable alias. No alias proposed to keep the surface minimal.
- **Sibling flags `--shorts-only` / `--live-only`** (auto-playlists `UUSH` +
  suffix / `UULV` + suffix, same transform family): deferred. Cheap follow-up
  if wanted, but the asked-for option is videos-only; keep the surface to one
  flag until someone asks.

## Consequences

- One new file: `src/yt/rss.rs` (`feed_url(id)`, `validate(id)`, `resolve(url)`).
  `src/yt/mod.rs` gains the `Rss` dispatch arm. No ffmpeg check (no download).
- Zero new dependencies. `reqwest`/`serde` untouched; subprocess + stdout parse
  only. Linux/macOS/Windows safe (same as existing yt-dlp invocation).
- Help/completions gain `lum yt rss`; `src/yt/README.md` CLI shape updated.
- Existing `aud`/`vid`/`alb` paths unchanged.

## Testing

- Unit (`src/yt/rss.rs`): `feed_url` construction, `UC...` validation
  (accept 24-char, reject short/`NA`/empty), `playlist_id` transform
  (`UULF` + id[2..], byte-exact, preserves the 22-char suffix), `classify` table
  (video/channel accepted; playlist incl. `watch+&list=` rejected; bare
  IDs/handles and non-YouTube hosts rejected), dedupe-first logic, multi-ID
  rejection with the direct-channel-link message.
- CLI (`tests/yt_cli.rs`, mirroring existing fake-binary pattern):
  - `yt rss` with no URL fails with `required`.
  - fake `yt-dlp` script echoing `UCOC7Er4tw8VhkdB2D0wz2_A` -> stdout is exact
    feed URL, exit 0.
  - `--id-only` prints bare ID; `--videos-only` prints the `playlist_id` URL;
    both together print the bare playlist ID.
  - fake `yt-dlp` echoing nothing -> failure mentioning `channel_id`.
  - playlist URL fails without invoking `yt-dlp` (fake binary asserts it was
    never called) with `playlist links are not supported`.
  - fake `yt-dlp` echoing two distinct IDs fails with
    `multiple owners/creators` + `direct channel link`.
  - fail-fast: `yt rss <good> <bad-playlist> <good>` prints exactly one line
    and exits non-zero; the third URL is never resolved.
  - `yt` rejects unknown subcommand (existing test still passes).
- Manual: matrix of `@handle`, `/channel/UC...`, `/watch?v=`, `youtu.be`,
  `/shorts/`, `/live/` for one known channel all print the same feed URL;
  fetched feed contains `<yt:channelId>` equal to `--id-only` output.

## Non-goals

- Fetching/parsing feed contents, polling, OPML import/export.
- Playlist RSS (YouTube only offers per-channel feeds; playlist links are
  rejected inputs, not resolved).
- `--open`, clipboard copy, QR output.
- stdin (`-`) input — `urls: Vec<String>` positional only, like siblings.

## Decisions resolved

1. Fail-fast vs best-effort for multi-URL runs — **decided: fail-fast**,
   matching "one bad download fails the run" today. Stop at the first failure.
2. Input scope — **decided: only YouTube video or channel links.** Playlists
   rejected (even `watch?v=...&list=...`). Bare `UC...`, bare `@handle`,
   bare video IDs, and non-YouTube URLs rejected.
3. Multi-owner videos — **decided: reject.** 2+ distinct `channel_id`s for one
   URL errors with `video has multiple owners/creators ... — give a direct
   channel link instead`.
