# Apps Subcommand

`lum apps` installs a hardcoded catalog of desktop apps from their upstream GitHub releases on Linux, macOS and Windows, per user and without admin rights, and keeps the settings lum owns in each app's own config file.

## CLI

```sh
lum apps install <app> [--force]
lum apps list
lum apps status <app>
lum apps sync [--dry-run]
lum apps uninstall <app>
lum apps update <app> [--force]
```

`sync` is the all-apps operation: it installs missing apps, updates outdated ones and puts drifted settings back. `update` targets one app and reapplies its settings even when the app is up to date. There is no opt-in catalog behavior and no `--all` flag. `status` shows the installed version next to the latest release, which it looks up on GitHub.

## Language

**Build**:
One upstream release asset for one OS and set of CPU architectures, how it is packaged, and the settings lum owns for it. An app without a build for the current machine is unavailable there.

**Owned setting**:
One key in an app's config file that lum keeps at a fixed value. lum never owns a whole config file, and edits only the lines of the keys it owns.

**Drift**:
An owned setting whose value in the file is missing or not lum's.

**Dialect**:
The rules an app's own config parser follows, which lum uses to read and write the lines it owns.

**Launcher**:
The file lum writes so the desktop can start the app: a desktop entry on Linux, a Start Menu shortcut on Windows. macOS needs none.
_Avoid_: dotfiles, config sync

## Catalog

| App | Linux | macOS | Windows |
| --- | --- | --- | --- |
| `mgba` | `mGBA-<version>-appimage-{x64,arm64}.appimage` | `mGBA-<version>-macos.dmg` (universal) | `mGBA-<version>-win64.7z` |
| `snes9x` | `Snes9x-<version>-x86_64.AppImage` | `Snes9x-<version>-Mac.zip` (universal) | `snes9x-<version>-win32-x64.zip` |

Owned settings:

| App | OS | File | Section | Key | Value | Why |
| --- | --- | --- | --- | --- | --- | --- |
| `mgba` | all | `config.ini` | `ports.qt` | `updateAutoCheck` | `0` | lum installs updates; mGBA's updater would replace lum's files |
| `snes9x` | Windows | `snes9x.conf` | `Settings\Win\Files` | `Dir:SRAM`, `Dir:Savestates` | `~\ROMs` | its default, `.\Saves`, is inside the folder an update replaces |

Where the apps keep their config and saves:

| App | Linux | macOS | Windows |
| --- | --- | --- | --- |
| `mgba` config | `$XDG_CONFIG_HOME/mgba/config.ini` | `~/.config/mgba/config.ini` | `%APPDATA%\mGBA\config.ini` |
| `mgba` saves | beside each ROM | beside each ROM | beside each ROM |
| `snes9x` config | `$XDG_CONFIG_HOME/snes9x/snes9x.conf` | user defaults | `snes9x.conf` beside the program |
| `snes9x` saves | beside each ROM | `~/Library/Application Support/Snes9x` | `~\ROMs` (lum sets it) |

mGBA reads `config.ini` from the XDG config folder on macOS too, not from `~/Library` (`mCoreConfigDirectory` upstream). snes9x 1.63 for Mac keeps its preferences in user defaults rather than a file, and always saves to Application Support (`saveInROMFolder` is fixed at 2 upstream), so lum owns nothing there and `lum backup create roms` does not cover those saves.

Adding a setting is one `Setting { section, key, value }` in `catalog.rs`. A value is fixed text or the ROMs folder (`~/ROMs`), which `lum backup create roms` backs up; lum creates that folder when it sets it. Settings are hardcoded, like env aliases and the tools catalog, so one lum binary configures every machine the same way.

## Dialects

lum reads an owned value the way the app's own parser would, and writes it the way the app does:

- **inih** (mGBA): `key=value`, split at the first `=` or `:`.
- **snes9x** (`conffile.cpp`): `key = value`, split at the first `=` only, so `Dir:SRAM` keeps its colon. `#` starts a trailing comment and `##` is a literal `#`; quotes keep outer spaces; a trailing backslash would continue the line, so lum quotes such values.

Lines are compared after trimming; `[section]` headers, and `#` or `;` comment lines, are recognized in both. When a key appears more than once, lum sets every copy, and the last one is what both apps read. A missing key goes after the last entry of its section, or into a new section at the end of the file. The file keeps its line endings.

## Storage

- state: `config_dir()/apps-state.json` (XDG-aware, same policy as `tools-state.json`)
- install root: `$LUM_APPS_DIR` if set, else
  - Linux: `$XDG_DATA_HOME/lum/apps` (`~/.local/share/lum/apps`)
  - macOS: `~/Applications/lum`
  - Windows: `%LOCALAPPDATA%\Programs\lum`
- launchers:
  - Linux: `$XDG_DATA_HOME/applications/lum-<app>.desktop`
  - Windows: `%APPDATA%\Microsoft\Windows\Start Menu\Programs\lum\<Title>.lnk`

## Packages

A build's package names the app's layout; the asset's extension says how it is packed.

- **AppImage (Linux)**: lum unpacks the SquashFS image that follows the ELF runtime into `<root>/<app>` with `backhand`. It never mounts the image or runs the runtime, so the app starts without FUSE. Both upstream AppImages use the legacy runtime, which needs `libfuse.so.2`; Arch and other current distributions install only FUSE 3. The desktop entry is the AppImage's own, with `Exec` pointed at `AppRun` and `Icon` at the image's icon, so its file associations keep working. lum refreshes the MIME cache with `update-desktop-database` when it is installed.
- **Bundle (macOS)**, from a `.dmg` or a `.zip`: a disk image is attached with `hdiutil`, and `ditto` copies the bundle out, keeping its symlinks and attributes; no maintained Rust crate reads HFS+. A zip holding the bundle at its root is extracted with the `zip` crate, which restores the bundle's symlinks and modes. Files lum downloads never get `com.apple.quarantine`.
- **Program folder (Windows)**, from a `.zip` or `.7z`: the archive's only folder, or its root when it holds the program's files directly, goes to `<root>/<app>`. `remove` drops files after unpacking: lum deletes mGBA's `portable.ini`, which would keep mGBA's settings beside `mGBA.exe`. `keep` lists what the program writes beside itself; an update carries those over from the previous install. For snes9x that is `snes9x.conf` and the folders its `Dir:` settings default to. Windows' own `WScript.Shell` writes the Start Menu shortcut, through PowerShell, as Scoop does.

See [ADR 0025](../../docs/adr/0025-apps-install-upstream-builds-per-platform.md), [ADR 0026](../../docs/adr/0026-apps-own-settings-not-config-files.md), [ADR 0027](../../docs/adr/0027-app-settings-are-edited-line-by-line.md) and [ADR 0028](../../docs/adr/0028-program-folders-keep-what-the-program-writes.md).

## Notes

A build may carry a note that lum prints after installing it, for what lum cannot do itself. Snes9x's AppImage leaves glibmm out along with GLib, which the host provides, so it needs glibmm 2.4 (`libglibmm-2.4.so.1`) from the system; on Arch, `sudo pacman -S glibmm`.

## Safety

- `install` errors when the app is already managed unless `--force` is passed.
- An unmanaged folder or bundle at the install path is protected: `install` and `sync` never modify or delete it. `--force` takes it over.
- **Running apps**: lum refuses to install, update or uninstall an app, or write its settings, while its process runs, matching the executable's file name (an AppImage started through its `AppRun` symlink is listed as `AppRun`). mGBA rewrites `config.ini` on every settings change and ROM load, and Windows cannot replace a running program's files.
- **Swap**: a new build is unpacked into a staging folder inside the install root and renamed into place. The previous install is moved aside first; its kept entries move into the new one; if anything fails, they move back and the previous install returns.
- **Foreign copies (macOS)**: before any install, update or sync, lum scans `/Applications` and `~/Applications` (override with `$LUM_APPS_FOREIGN_SCAN_DIRS`) for an unmanaged bundle with the same name and fails fast if one exists. With two copies of one bundle, Launch Services may open either.
- **Checksums**: downloading the same release asset again must match the recorded SHA-256.
- **Unpacking**: AppImage entries that would leave the app folder or pass through a symlink are refused; the `zip` and `sevenz-rust2` crates refuse traversal themselves, and `zip` recreates only symlinks that stay inside the destination.
- `apps-state.json` is written only after the app is in place; a failed install leaves no managed state.
- Settings are written to a temporary file that replaces the config with one rename, keeping its permissions. A config file that is not UTF-8 is refused rather than rewritten.
- `uninstall` removes the app folder whole. Settings kept elsewhere stay; a program folder's own config (snes9x on Windows) goes with it, and lum says so.

## Implementation

Integration tests live in `tests/apps_cli.rs` and run on Linux without network access. `LUM_APPS_TEST_ARTIFACT_<NAME>` points at a local artifact (synthetic AppImages for both apps, built by the test), `$LUM_APPS_DIR` and the XDG variables keep everything in a temporary folder, and `LUM_APPS_TEST_PROCESS_<NAME>` replaces the process name lum looks for, so an app running on the test machine cannot interfere. The unpackers, both dialects, the kept-file swap and the desktop entry rewriting have unit tests. The disk image and Start Menu paths only run on their own OS.

OpenEmu left the catalog when apps went cross-platform: it is macOS-only, Intel-only and has had no release since 2023. `lum apps` ignores an `OpenEmu.app` it installed earlier, and its state entry; delete them by hand. `lum backup openemu` went with it.
