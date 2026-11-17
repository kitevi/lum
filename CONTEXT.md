# Lum

Lum is an opinionated CLI toolbox for automating personal developer workflows.

Subsystem language lives with each subcommand's local README when it grows beyond project-wide vocabulary.

## Language

**Radio playback**:
Internet radio playback owned by `lum radio`, implemented by controlling an external `ffplay` process and storing lightweight state. While a station is loaded, its hidden station runner (a subprocess of `lum` itself, not a daemon) also shows a system tray with a Stop/Play toggle, the station list and Quit, and on macOS the display tray's lines while one runs.
_Avoid_: Radio daemon, service, server, custom Radio player process

**Display tray**:
The tray icon `lum display off` starts on macOS to show which monitors have HiDPI off. It is a hidden `lum` subprocess that lives only while HiDPI is off, hides while a station's tray shows the same lines, and exits by itself once HiDPI is back on.
_Avoid_: Display daemon, display service
