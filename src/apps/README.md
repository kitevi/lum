# Apps Subcommand

`lum apps` installs a hardcoded catalog of desktop apps from their upstream GitHub releases on Linux, macOS and Windows, per user and without admin rights, and keeps the settings lum owns in each app's own config file.

## CLI

```sh
lum apps ls
lum apps install <app> [--force]
lum apps status <app>
lum apps update <app> [--force]
lum apps sync [--dry-run]
lum apps version <app>
lum apps uninstall <app>
```

`sync` is the all-apps operation: it installs missing apps, updates outdated ones and puts drifted settings back. `update` targets one app and reapplies its settings even when the app is up to date. There is no opt-in catalog behavior and no `--all` flag.

## Language

**Build**:
One upstream release asset for one OS and set of CPU architectures, and how it is packaged. An app without a build for the current machine is unavailable there.

**Owned setting**:
One key in an app's config file that lum keeps at a fixed value. lum never owns a whole config file.

**Drift**:
An owned setting whose value in the file is missing or not lum's.

**Launcher**:
The file lum writes so the desktop can start the app: a desktop entry on Linux, a Start Menu shortcut on Windows. macOS needs none.
_Avoid_: dotfiles, config sync

## Catalog

| App | Linux | macOS | Windows |
| --- | --- | --- | --- |
| `mgba` | `mGBA-<version>-appimage-{x64,arm64}.appimage` | `mGBA-<version>-macos.dmg` (universal) | `mGBA-<version>-win64.7z` |

Owned settings:

| App | Section | Key | Value | Why |
| --- | --- | --- | --- | --- |
| `mgba` | `ports.qt` | `updateAutoCheck` | `0` | lum installs updates; mGBA's updater would replace lum's files |

mGBA reads `config.ini` from `%APPDATA%\mGBA` on Windows and from `$XDG_CONFIG_HOME/mgba` (else `~/.config/mgba`) on Linux **and macOS**, not from `~/Library` (`mCoreConfigDirectory` upstream).

Adding a setting is one `Setting { section, key, value }` in `catalog.rs`. Settings are hardcoded, like env aliases and the tools catalog, so one lum binary configures every machine the same way.

## Storage

- state: `config_dir()/apps-state.json` (XDG-aware, same policy as `tools-state.json`)
- install root: `$LUM_APPS_DIR` if set, else
  - Linux: `$XDG_DATA_HOME/lum/apps` (`~/.local/share/lum/apps`)
  - macOS: `~/Applications/lum`
  - Windows: `%LOCALAPPDATA%\Programs\lum`
- launchers:
  - Linux: `$XDG_DATA_HOME/applications/lum-<app>.desktop`
  - Windows: `%APPDATA%\Microsoft\Windows\Start Menu\Programs\lum\<program>.lnk`

## Packages

- **AppImage (Linux)**: lum unpacks the SquashFS image that follows the ELF runtime into `<root>/<app>` with `backhand`. It never mounts the image or runs the runtime, so the app starts without FUSE. mGBA's AppImage uses the legacy runtime, which needs `libfuse.so.2`; Arch and other current distributions install only FUSE 3. The desktop entry is the AppImage's own, with `Exec` pointed at `AppRun` and `Icon` at the image's icon, so its file associations keep working. lum refreshes the MIME cache with `update-desktop-database` when it is installed.
- **Disk image (macOS)**: `hdiutil attach`, `ditto` the bundle, `hdiutil detach`. No maintained Rust crate reads HFS+, and `ditto` keeps the bundle's symlinks and attributes. Files lum downloads never get `com.apple.quarantine`.
- **7z (Windows)**: `sevenz-rust2` unpacks the archive's single top-level folder into `<root>/<app>`. lum deletes mGBA's `portable.ini`: with it, mGBA keeps its settings beside `mGBA.exe`, where every update would replace them. Windows' own `WScript.Shell` writes the Start Menu shortcut, through PowerShell, as Scoop does.

See [ADR 0025](../../docs/adr/0025-apps-install-upstream-builds-per-platform.md) and [ADR 0026](../../docs/adr/0026-apps-own-settings-not-config-files.md).

## Safety

- `install` errors when the app is already managed unless `--force` is passed.
- An unmanaged folder or bundle at the install path is protected: `install` and `sync` never modify or delete it. `--force` takes it over.
- **Running apps**: lum refuses to install, update or uninstall an app, or write its settings, while its process runs. mGBA rewrites `config.ini` on every settings change and ROM load, and Windows cannot replace a running program's files.
- **Swap**: a new build is unpacked into a staging folder inside the install root and renamed into place. The previous install is moved aside first and moved back if the rename fails.
- **Foreign copies (macOS)**: before any install, update or sync, lum scans `/Applications` and `~/Applications` (override with `$LUM_APPS_FOREIGN_SCAN_DIRS`) for an unmanaged bundle with the same name and fails fast if one exists. With two copies of one bundle, Launch Services may open either.
- **Checksums**: downloading the same release asset again must match the recorded SHA-256.
- **Unpacking**: AppImage entries that would leave the app folder or pass through a symlink are refused; `sevenz-rust2` refuses traversal itself.
- `apps-state.json` is written only after the app is in place; a failed install leaves no managed state.
- Settings are written to a temporary file that replaces the config with one rename, keeping its permissions. Values are read and written literally, without quote or escape processing, so Windows paths survive.

## Implementation

Integration tests live in `tests/apps_cli.rs` and run on Linux without network access. `LUM_APPS_TEST_ARTIFACT_<NAME>` points at a local artifact (a synthetic AppImage built by the test), `$LUM_APPS_DIR` and the XDG variables keep everything in a temporary folder, and `LUM_APPS_TEST_PROCESS_<NAME>` replaces the process name lum looks for, so an app running on the test machine cannot interfere. The 7z and AppImage unpackers and the desktop entry rewriting have unit tests. The disk image and Start Menu paths only run on their own OS.

OpenEmu left the catalog when apps went cross-platform: it is macOS-only, Intel-only and has had no release since 2023. `lum apps` ignores an `OpenEmu.app` it installed earlier, and its state entry; delete them by hand. `lum backup openemu` went with it.
