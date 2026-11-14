# Backup

`lum backup` backs up and restores hardcoded personal data targets:

- `bio`: `~/.bio` on macOS and Linux
- `roms`: `~/ROMs` on macOS, Linux and Windows. Keep ROMs there: mGBA writes battery saves (`.sav`) and save states (`.ss0`–`.ss9`) beside each ROM, so one backup carries both. Quit mGBA first; it flushes saves to disk on exit. The Windows path has not run on real hardware.

Running `lum backup <target>` creates a `.tar.gz` archive, uploads it to the hardcoded x0.at file host, downloads it back, and verifies that the uploaded file is a valid archive containing the target path.

Running `lum backup <target> <code>` downloads `https://x0.at/<code>` (the code is the exact file name the host returned, e.g. `jtPb.gz`), verifies it, moves any existing target directory to a timestamped local backup, and extracts the archive into the home directory. Restore keeps the three newest local backups per target.

The command is intentionally not backward-compatible with the old zzk `.tar.xz` backup codes.
