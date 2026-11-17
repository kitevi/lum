# Radio drops YouTube handling

Status: executed (uncommitted). Verified with `cargo fmt`, `cargo clippy --all-targets --
-D warnings`, `cargo test --workspace`, and `LUM_RADIO_PLAYBACK_TESTS=1 SDL_AUDIODRIVER=dummy
cargo test radio::` (50 passed, including the real-ffplay respawn and pause probes).
Also confirmed by hand: `lum radio list` shows 15 stations, `lum radio ytlf` exits 1 with the
station reference, `lum __radio_playlist_runner aphx` exits 2 as an unknown subcommand.

## Outcome

`lum radio` carries a second playback path for YouTube-backed stations: yt-dlp resolves a
channel or video page URL before every ffplay attempt, a separate playlist runner exists only
to advance through per-track resolutions, and two catalog entries (`ytlf`, `aphx`) depend on
it. The request: remove YouTube handling from `lum radio` and every station that depended on
it.

Intended behavior: every station is a direct URL `ffplay` can play. Radio never invokes
yt-dlp, has one runner, and the catalog is the remaining 15 direct stations.

Acceptance criteria:

1. `lum radio list` prints 15 rows; `ytlf` and `aphx` are gone.
2. `lum radio ytlf` exits non-zero with `station not found: ytlf` plus the station reference.
3. No `yt-dlp`, `YouTube`, `StationKind`, or playlist-runner reference remains in `src/radio`.
4. The hidden `__radio_playlist_runner` subcommand no longer exists; `__radio_direct_runner` does.
5. `lum radio rand` draws uniformly from the remaining catalog and still skips the remembered code.
6. `lum yt` is untouched: yt-dlp resolution, download args and RSS tests still pass.
7. Unchanged radio behavior: supervisor respawn/backoff, give-up after 5 short attempts, tray
   pause/resume and `paused` state, `playing|paused|stopped` output.
8. A stale state file written by the old playlist runner still parses, so `status` reports
   `stopped` and `stop` succeeds instead of erroring on an unknown state kind.

## Approach

One vertical slice per concern, code first because the docs describe it:

1. Delete the catalog dependency: drop `ytlf` and `aphx` from `src/radio/stations.rs`, and with
   them `StationKind`, the `kind` field on `Station`, and `playlist_urls()`. `Direct` was the only
   value left, and `rand` already ignored kind, so the field carried no information.
2. Delete the code only reachable from those stations in `src/radio/mod.rs`: `playable_url`,
   `resolve_youtube_stream_url`, `yt_dlp_reason`, `youtube_stream_url_args`, the playlist runner
   (`run_playlist_runner`, `run_playlist_loop`, `randomized_playlist_urls`,
   `playlist_failure_message`), the kind dispatch in `start`, and the playlist guard in
   `run_station_stream_loop`. `play_station_once` passes `station.url` straight to ffplay.
3. Collapse the player layer in `src/radio/player.rs` to the one command that is left: delete
   `start_playlist`, `playlist_runner_command`, the reconnect-free `play_until_exit`/`ffplay_command`
   pair, and make the live command (reconnect + `-infbuf` + `delay`, ADR-0020) the only
   `ffplay_command`.
4. Delete the hidden subcommand: `Commands::RadioPlaylistRunner` in `src/cli.rs`, its arm in
   `src/main.rs`, and `pub(crate) use deps::resolve_yt_dlp;` in `src/yt/mod.rs` (radio was its only
   external consumer; `yt` calls `deps::resolve_yt_dlp` directly).
5. Update `src/radio/README.md` and the ADRs that describe current radio behavior; add an ADR
   recording "direct streams only".

`RadioProcessKind::PlaylistRunner` stays as a legacy-only state variant (see Decisions).

## Decisions

| Question | Choice | Evidence | What would flip it |
|---|---|---|---|
| Remove `StationKind` instead of leaving a single-variant enum? | Remove enum and `kind` field | `src/radio/stations.rs:2-15`; `src/radio/mod.rs:146,198-200,405`; `station.kind` is only matched where the removed kinds were handled; random selection already ignored it (`src/radio/README.md:28`) | A second station kind is planned next; re-adding the field is a 10-line change |
| Keep `RadioProcessKind::PlaylistRunner`? | Keep, documented as legacy | `src/radio/mod.rs:73-91,537-553`; `read_state` hard-errors on an unknown enum value (`src/radio/mod.rs:559-568`), so removing it makes `status`/`stop` fail for anyone whose last station was `aphx` until the file is deleted by hand. Precedent: retired codes and `Ffplay` state are already tolerated | A versioned state file or a "state that fails to parse is absent" rule is added |
| Delete the reconnect-free ffplay path? | Delete | Only `run_playlist_loop` used it (`src/radio/player.rs:74-75,127-128`); it existed so playlist tracks could reach EOF | A non-live, finite-length station is added |
| New ADR? | Yes, `docs/adr/0031` | ADR-0002 and ADR-0008 record the yt-dlp path as a live decision; deleting it silently would leave them claiming current behavior | The user prefers no ADR ceremony; drop step 5's ADR and keep the README + ADR-0002/0014/0022 edits |
| Motive for the removal | Unverified | The request states the change, not a reason. The ADR records the decision and its consequences, not an invented motive | The user supplies a reason; add it to the ADR Context |

Assumptions: no external consumer of the hidden `__radio_playlist_runner` command exists (it is
hidden, undocumented outside the radio README, and only spawned by lum itself); retired codes
need no state migration, matching the existing retired `atma` precedent.

## Boundaries

- `lum yt` keeps yt-dlp (downloads, albums, RSS). Only radio's use of it goes.
- The ffmpeg/ffplay provisioning path, tray, display tray lines, and ffplay recovery arguments
  are unchanged.
- Legacy `Ffplay` state handling and the retired-code tolerance stay as they are.

## Verification

| Criterion | Check | Expected |
|---|---|---|
| 1 | `cargo test --test cli_contract radio_list_shows_station_tags_then_shows_the_random_selector` | passes; output has `conc  RadCap Musique Concrète`, no `ytlf`/`aphx` |
| 2 | new `radio_rejects_a_removed_youtube_station` in `tests/cli_contract.rs` | exit 1, stderr `station not found: ytlf`, listing follows |
| 3 | `grep -rn 'yt-dlp\|YouTube\|StationKind\|playlist' src/radio` | no match |
| 4 | `cargo test --test cli_contract help_prints_public_commands_and_hides_internal_ones`; `cargo run -- __radio_playlist_runner aphx` | help hides `__radio_direct_runner`; the removed command exits 2 with `unexpected argument` |
| 5 | `cargo test radio::` (`random_station_draws_*`, `eligible_stations_*`) | 200 draws stay inside the catalog and never equal the excluded code |
| 6 | `cargo test --test yt_cli` | passes unchanged |
| 7 | `cargo test --workspace`; optional `LUM_RADIO_PLAYBACK_TESTS=1 cargo test radio::` | unit tests pass; probes still respawn and pause without giving up |
| 8 | new unit test parsing `{"process_kind":"PlaylistRunner",...}` | parses, `status` path (`process_is_alive`/`stop_process`) routes to the generic runner branch |

## Docs

- `src/radio/README.md`: catalog fields, random pool, playback stack, supported streams, runtime
  semantics, tests (drop yt-dlp/playlist/youtube wording).
- `docs/adr/0002-radio-playback-uses-ffplay.md`: remove the yt-dlp bullet from "Lum owns".
- `docs/adr/0014-radio-stations-self-heal-stream-failures.md`: drop the YouTube re-resolution and
  playlist-runner sentences.
- `docs/adr/0022-the-tray.md`: drop the playlist/yt-dlp sentence in the switch consequences.
- `docs/adr/0008-youtube-radio-uses-yt-dlp-and-ffmpeg.md`: note removal alongside its superseded status.
- New `docs/adr/0031-radio-plays-direct-streams-only.md`.
- Root `README.md` and `CONTEXT.md` need no change (neither names YouTube stations).

## Recovery

No migration. Rollback is `git revert`. Stale state files naming `ytlf`, `aphx`, or a playlist
runner keep parsing; `status` prints `stopped` once that process is gone and removes the file.
