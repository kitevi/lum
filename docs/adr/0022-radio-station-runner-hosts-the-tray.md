# The Radio Station Runner Hosts a Tray Toggle

Status: Accepted. The single-item menu and "no Quit item" are superseded by ADR-0023. Reverses the "no paused state" part of ADR-0010 and builds on the runners of ADR-0014.

## Context

`lum radio <code>` starts a station and returns to the shell, so nothing on screen shows that radio is playing, and stopping or restarting it means going back to a terminal. We want a system tray icon that appears while a station plays, with a single Stop/Play toggle, on Linux, macOS and Windows, and without a separate daemon (`CONTEXT.md` avoids "Radio daemon").

A long-lived process already exists: ADR-0014's hidden `__radio_direct_runner` and `__radio_playlist_runner` supervisors. A runner lives exactly as long as a station is loaded, is the same `lum` binary, and already owns the whole ffplay lifecycle.

Options considered:

- A separate tray process or daemon: a second long-lived program to start, find and clean up, plus an IPC protocol that ADR-0002 declined to own.
- tray-icon's default Linux backend (GTK 3 and libappindicator): the binary hard-links GTK and about 70 shared libraries in total, so every lum command, `git-id`, `env` and `backup` included, would refuse to start on a Linux machine without GTK 3, and CI would need GTK development packages.
- The runner hosting the tray: no new process and no IPC.

## Decision

The station runner hosts the tray.

- **Threads.** The runner's main thread owns the tray, because macOS requires the main thread. The supervisor loop runs on a worker thread that starts first, so a tray can never delay or gate playback. The two share only an in-process `Playback` handle: the pause flag and the running ffplay. All tray and menu objects stay on the main thread.
- **Menu.** One item, `Stop <station>` or `Play <station>`. Stop kills ffplay, through `shared_child` so it can be killed while the supervisor is blocked waiting on it. Play respawns it. A pause is not a stream failure: it does not back off or count toward giving up, backoff sleeps wake early on it, and resuming starts a fresh failure streak. The playlist runner pauses the same way and replays the current track on resume.
- **The tray stays while paused.** The runner, and so the tray, outlives a pause; that is how Play works. `lum radio stop`, starting another station, or the runner giving up removes it. There is deliberately no Quit item.
- **State.** `radio-player.json` gains `paused`, written by the runner itself, only when the file names its own PID, and replaced atomically. `lum radio status` prints `paused <code> <name>` for a paused station. No CLI commands are added.
- **Crates.** `tray-icon` with its `ksni` backend: StatusNotifierItem over D-Bus, pure Rust, no GTK. zbus is already in the tree through `notify-rust`. On Windows and macOS, `winit` (the blessed.rs default) supplies the native event loop the tray needs; the Linux backend runs its own thread, so Linux needs none. The default GTK backend stays off.
- **Optional by construction.** If the tray cannot be created (no D-Bus session, no tray host, no window server), the runner logs a warning and keeps playing.

## Consequences

- Cost on Linux: 21 more crates and about 690 KB more binary (5.63 to 6.32 MB, +12%), with no new shared libraries and no extra CI packages. Windows adds 14 crates and macOS 33.
- ADR-0010's two-state lifecycle no longer holds: a loaded station is playing or paused. ADR-0010's objection was a paused flag that the CLI had to emulate by stopping and restarting ffplay. Here the runner owns the flag and nothing is added to the CLI.
- ADR-0002 still holds. ffplay plays the audio. The runner gained a UI surface and an in-process handle, not a protocol or a player.
- Resume rejoins the live stream, so ADR-0020's 5s of leading silence returns each time.
- The icon is only visible with a tray host. Stock GNOME has none (the AppIndicator extension provides one). Without a host the icon is invisible and playback is unaffected.
- Clicking the icon itself is not a portable gesture across tray hosts, so the toggle is a menu item.
- Windows: `lum radio stop` ends the runner with TerminateProcess, and Windows can leave the icon of a killed process in the notification area until the mouse passes over it. A graceful stop path would fix that if it matters.
- Release builds use `panic = "abort"`, and macOS menus panic off the main thread, so a panic in tray code would take playback down. This is why all menu work stays on one thread.
- The macOS and Windows paths are type-checked on Linux only; the release workflow compiles them only when cutting a release. They have not been run.
- The tray cannot be exercised in CI (no display or tray host). Pause semantics are unit-tested without it, and the ffplay-level pause probe is opt-in with the other playback probes.
