# Lum

Lum is an opinionated CLI toolbox for automating personal developer workflows.

Subsystem language lives with each subcommand's local README when it grows beyond project-wide vocabulary.

## Language

**Radio playback**:
Internet radio playback owned by `lum radio`, implemented by controlling an external `ffplay` process and storing lightweight state. While a station is loaded, its hidden station runner (a subprocess of `lum` itself, not a daemon) also shows a system tray with a Stop/Play toggle.
_Avoid_: Radio daemon, service, server, custom Radio player process
