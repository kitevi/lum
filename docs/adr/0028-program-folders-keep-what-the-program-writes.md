# Program Folders Keep What The Program Writes

## Status

Accepted. Extends the package kinds of ADR-0025.

## Context

snes9x for Windows is a portable program. It reads and writes `snes9x.conf` beside `snes9x-x64.exe`, always, and by default keeps battery saves, save states, cheats, screenshots and more in folders beside it. lum installs a program folder by swapping the whole folder, so an update would wipe all of that.

snes9x also ships zips where ADR-0025 expected other formats: a flat zip of the Windows program, with no top-level folder, and a zip holding the Mac bundle.

## Decision

- A package names the app's layout, and the asset's extension says how it is packed: an AppImage; a macOS **bundle** from a `.dmg` or a `.zip`; a Windows **program folder** from a `.zip` or `.7z`, which is the archive's only folder or else its root.
- A program folder lists what its program writes beside itself. An update moves those entries from the previous install into the new one before the swap, the program's copy winning over one the new build ships, and moves them back if the swap fails. For snes9x that is `snes9x.conf` and the folders its `Dir:` settings default to.
- lum points snes9x's battery saves and save states at `~\ROMs` on Windows (ADR-0027), so saves leave the program folder and sit with the games, as they do on Linux.

## Consequences

- Updating snes9x on Windows keeps its settings and anything it wrote beside itself.
- `lum apps uninstall` removes a program folder whole, its config included, and says so.
- Zips are extracted with the `zip` crate already in the tree. It restores a bundle's symlinks and modes and refuses entries that would land outside the destination.
- snes9x's Windows and macOS paths have not run on real hardware. The snes9x 1.63 Windows and Mac zips unpack as expected on Linux.
