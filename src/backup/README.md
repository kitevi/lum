# Backup

`lum backup` backs up and restores hardcoded personal data targets:

- `bio`: `~/.bio` on macOS and Linux
- `roms`: `~/ROMs` on macOS, Linux and Windows. Keep ROMs there: mGBA, and snes9x on Linux, write battery saves and save states beside each ROM, and on Windows `lum apps` points snes9x's saves at `~\ROMs`, so one backup carries games and saves. Snes9x on macOS saves to `~/Library/Application Support/Snes9x` instead, which this target does not cover. Quit the emulator first, so its saves are on disk. The Windows path has not run on real hardware.

Running `lum backup create <target>` creates a `.tar.gz` archive, uploads it to the hardcoded x0.at file host, downloads it back, and verifies that the uploaded file is a valid archive containing the target path.

Running `lum backup restore <target> <code>` downloads `https://x0.at/<code>` (the code is the exact file name the host returned, e.g. `jtPb.gz`), verifies it, moves any existing target directory to a timestamped local backup, and extracts the archive into the home directory. Restore keeps the three newest local backups per target.

The command is intentionally not backward-compatible with the old zzk `.tar.xz` backup codes.
