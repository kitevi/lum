# Radio Playback Uses ffplay

Status: Accepted (YouTube station resolution removed by ADR-0031). ADR-0014 adds a runner that supervises ffplay, and ADR-0022 brings pause back through the tray.

## Context

`lum radio` reimplements the old `ruv` workflow: list built-in stations, play a station, stop playback, and check status. The command line's pause and resume were later removed by ADR-0010, and ADR-0022 brought pause back as the tray's Stop/Play toggle.

The first Rust implementation direction used a pure Rust foreground playback stack:

- `reqwest` for direct HTTPS stream fetching
- `symphonia` for decoding
- `cpal` for cross-platform audio output
- `crossterm` for terminal controls
- `ringbuf` / `rubato` / audio buffer crates for decode-to-output plumbing

A later direction added a custom background Radio player process controlled over IPC. That would have made lum own audio output, decoding, buffering, process lifecycle, IPC protocol, stale endpoint cleanup, and retry semantics.

That was too much implementation surface for the actual product need. Lum already provisions or locates ffmpeg for video workflows, and `ffplay` is commonly distributed with ffmpeg builds.

## Decision

Use `ffplay` as the playback adapter for `lum radio`.

Lum owns:

- station catalog and CLI routing
- starting/stopping an `ffplay` process
- storing lightweight playback state in the platform-native state directory

Lum does not own:

- audio decoding
- audio output device integration
- terminal-key foreground controls
- a custom radio IPC protocol
- a player process of its own: ffplay plays, and the runner of ADR-0014 only supervises it

`ffplay` is resolved from `$PATH` first. If unavailable, lum looks for a provisioned `ffplay` next to its managed ffmpeg binary and the ffmpeg provisioning path extracts `ffplay` when the downloaded archive includes it.

## Consequences

This greatly reduces implementation surface area and dependency surface for radio playback. The pure Rust playback stack dependencies should stay removed unless a future ADR reverses this decision.

`lum radio <code>` starts a hidden runner, detached, that supervises `ffplay` (ADR-0014), and records `radio-player.json` state containing the runner's process id, its start time when available, the station code and description and, since ADR-0022, whether playback is paused.

`stop` ends the remembered runner, which kills its `ffplay`, and clears state. The command line has no pause or resume (ADR-0010); the tray's Stop/Play toggle pauses (ADR-0022).

`status` is based on lum's remembered state plus a process identity/aliveness check. Lum verifies the remembered PID still looks like the runner (like ffplay, for state written before runners existed) and, when start time is known, that it is the same process instance. It is intentionally simpler than a full playback telemetry channel.

If a platform's managed ffmpeg archive does not include `ffplay`, radio playback requires a user-installed ffplay. Do not fall back to a custom Rust audio stack just to cover that case.

If future requirements need richer control, prefer extending the external-player adapter first. Reopening a custom internal Radio player or pure Rust playback stack requires a new ADR.
