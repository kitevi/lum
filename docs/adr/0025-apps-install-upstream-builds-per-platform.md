# Apps Install Upstream Builds Per Platform

## Status

Accepted

## Context

`lum apps` began as a macOS-only installer for OpenEmu, after Homebrew stopped shipping casks that fail Gatekeeper (the `openemu` cask was disabled on 2026-09-01). It now has to install apps on Linux, macOS and Windows, starting with mGBA. The package managers lum could delegate to do not cover it: Homebrew's `mgba` formula is deprecated and there is no cask, and the Flathub build moves mGBA's settings into `~/.var/app` and brings in the KDE runtime.

mGBA ships one asset per platform, and each has a trap:

- The Linux AppImage uses the legacy AppImageKit runtime, which loads `libfuse.so.2`. Arch and other current distributions install only FUSE 3, so the AppImage does not start.
- The macOS build is a `.dmg` holding an HFS+ volume. No maintained Rust crate reads HFS+.
- The Windows `.7z` contains an empty `portable.ini`, which makes mGBA keep its settings beside `mGBA.exe`, inside the folder an update replaces.

## Decision

Each catalog app lists **builds**: an upstream release asset per OS and CPU architecture, and how it is packaged. lum installs per user, without admin rights:

- **AppImage**: lum unpacks the SquashFS image that follows the ELF runtime (where the runtime's section header table ends) with `backhand`, into `~/.local/share/lum/apps/<app>`. It never mounts the image or runs the runtime. A desktop entry in `~/.local/share/applications/lum-<app>.desktop`, made from the AppImage's own, starts `AppRun` and keeps the app's file associations.
- **Disk image**: `hdiutil attach`, `ditto` the bundle into `~/Applications/lum`, `hdiutil detach`. These two system tools are the exception to lum's pure-Rust installs.
- **7z**: `sevenz-rust2` unpacks the single top-level folder into `%LOCALAPPDATA%\Programs\lum\<app>`, without `portable.ini`. Windows' own `WScript.Shell` writes a Start Menu shortcut through PowerShell, instead of lum hand-encoding the `.lnk` format.

Installs unpack into a staging folder inside the install root and rename into place, keeping the previous install until then. lum refuses to change an app while it runs.

OpenEmu leaves lum, both the catalog entry and `lum backup openemu`: it is macOS-only, Intel-only and has had no release since 2023.

## Consequences

- An unpacked AppImage takes about 2.5 times the space of the file (62 MB instead of 25 MB for mGBA 0.10.5) and starts without FUSE or a mount.
- `backhand` (gzip only) and `sevenz-rust2` (no default features; it shares `lzma-rust2` with lum) are new dependencies. Both are pure Rust and compiled on every OS, so Linux CI tests both unpackers.
- The disk image and Start Menu paths run only on their own OS; CI does not exercise them.
- A new app is a catalog entry if its upstream ships one of these three package kinds. Another kind, such as an installer or a zipped bundle, needs a new `Package` variant.
- An `OpenEmu.app` installed by lum stays on disk, unmanaged.
- A backup made by `lum backup openemu` is a plain `.tar.gz` of `Library/Application Support/OpenEmu`. Restoring one now means extracting it into the home folder by hand.
