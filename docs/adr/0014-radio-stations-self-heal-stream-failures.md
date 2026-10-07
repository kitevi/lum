# Radio Stations Self-Heal Stream Failures

Status: Accepted

## Context

`lum radio` spawned ffplay fully detached with all stdio nulled and never supervised it (ADR-0002 kept that surface minimal). Diagnosis with a local fixture server driving the real spawn path showed the resulting failure mode: any stream interruption — a server-side drop or a stalled connection — leaves ffplay alive but silent forever. Without `-autoexit`, ffplay waits at end of stream instead of exiting; without a read timeout, it blocks indefinitely on a connection that stopped sending. Because `status` only checks that the remembered PID is a live ffplay process, it kept reporting `playing` while no audio flowed.

Users experienced this as "sometimes audio stops", with manual restart of the station as the only recovery. The trigger is upstream/server-side interruption of long-lived streams, not sleep/wake or network switching.

## Decision

Layer three recovery mechanisms, all inside the external-player adapter, per ADR-0002's preference to extend the adapter before adding new playback machinery:

1. Every ffplay invocation gets `-rw_timeout 15000000`: a connection that delivers no data for 15 seconds errors out instead of blocking forever.
2. Live stations additionally run with `-reconnect 1 -reconnect_streamed 1 -reconnect_delay_max 5`. The HTTP layer then re-establishes dropped or timed-out connections transparently; probing confirmed ffplay sustains playback across repeated server-side force-closes with no audible gap and no process exit.
3. A hidden `__radio_direct_runner` supervisor loops ffplay for direct and YouTube stations. `-autoexit` makes each attempt observable; on exit the runner respawns with escalating backoff (2s, 5s, 15s, then 30s capped), re-resolving YouTube stream URLs through yt-dlp on every attempt so expired URLs heal themselves; a failed resolution (e.g. a transient yt-dlp error) counts as a failed attempt and backs off rather than killing the runner. A session lasting at least 60 seconds resets the failure streak. After 5 consecutive short-lived failures the runner sends a desktop notification, logs the failure to the rotating tracing log, and exits, so `status` truthfully reports `stopped`.

Playlist stations keep the separate `__radio_playlist_runner` without the reconnect flags: playlist advancement depends on observing track EOF, and `-reconnect_streamed` would swallow EOF for sources without a known length.

## Consequences

- `lum radio status` reports `playing` while the runner process is alive, meaning playback is active or being retried with backoff. Audible gaps are now limited to the respawn window (roughly the backoff delay plus ffplay startup).
- The pathological case of a server that accepts connections but never sends data loops reconnect attempts inside ffplay indefinitely. Real servers either send data or close the connection; permanent exits are handled by the runner.
- ffplay stderr stays nulled. Stream failure details now reach the rotating `lum.log` through the runner's tracing output (initialized before command dispatch) instead of being lost.
- The playback-recovery probes need ffplay, ffmpeg, and an audio output device, so they are gated behind `LUM_RADIO_PLAYBACK_TESTS=1` and excluded from default test runs and CI, which has no audio device.
- State files written before runners existed remember an ffplay PID directly and keep working; new state remembers the runner PID.
