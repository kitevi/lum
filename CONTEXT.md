# Lum

Lum is an opinionated CLI toolbox for automating personal developer workflows.

Subsystem language lives with each subcommand's local README when it grows beyond project-wide vocabulary.

## Language

**Radio playback**:
Internet radio playback owned by `lum radio`, implemented by controlling an external `ffplay` process and storing lightweight state. While a station is loaded, its hidden station runner (a subprocess of `lum` itself, not a daemon) also shows a system tray with a Stop/Play toggle, the station list and Quit, and the status tray's lines while it runs.
_Avoid_: Radio daemon, service, server, custom Radio player process

**Status tray**:
The tray icon `lum display off` (macOS) and `lum net game` start to show what is in force: which monitors have HiDPI off, and that game mode is on. It is a hidden `lum` subprocess that lives only while one of them is, hides while a station's tray shows the same lines, and exits by itself once both are over. Its monitor's screen is hollow for HiDPI off and filled in blue for game mode.
_Avoid_: Display tray, display daemon, display service, status daemon
