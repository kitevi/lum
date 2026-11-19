# Backup Runs brdump From PATH Before Archiving bio

## Status

Accepted.

## Context

`lum backup create bio` archives `~/.bio` as it stands. `~/.bio/install/Brewfile` is only rewritten by `brdump`, which runs `brew bundle dump` and was a zsh function in the user's dotfiles. A backup taken after installing or removing Homebrew packages therefore carried a package list describing a machine that no longer existed, and nothing said so. That is worse than out of date: `breset` uninstalls whatever the Brewfile does not list, so resetting from such a backup removes the packages installed since.

As a shell function, `brdump` was invisible to child processes: the OS looks for executables, not functions. lum could source the user's shell startup (`zsh -ic brdump`) or run `brew bundle dump` with its own copy of the flags. The first drags interactive-shell side effects and startup cost into a backup; the second duplicates a flag list that belongs to the user's dotfiles.

## Decision

- **PATH is the opt-in.** lum looks the command up with `which`, the way it already resolves `ffmpeg` and `yt-dlp` (ADR-0005, ADR-0009). A machine that has `brdump` as an executable on `PATH` — the dotfiles keep it in `~/.bio/bin`, next to `lum` — has it run; a machine that does not backs up exactly as before, with no flag to set.
- **A target declares its own.** The step is a `pre_command` field in `src/backup/target.rs`, so `roms` says "nothing to prepare" by leaving it empty instead of lum branching on the target's name.
- **Output is held back until failure.** Captured stdout and stderr are printed only when the command exits nonzero, followed by why the backup stopped. A step that worked adds one line; a broken step is loud and nothing is uploaded.

## Consequences

- `brdump` and `breset` moved out of `.zshrc` into scripts in `~/.bio/bin`, so they travel with every `bio` backup and restore.
- A failing `brdump` aborts a backup that would otherwise have worked. It fails when `brew` is not on `PATH`, so a machine that has `~/.bio` but no Homebrew cannot back up `bio` as is. Fixing the command, or taking it off `PATH`, is the way through.
- The command has no timeout and runs in lum's working directory; it is treated like any other external tool lum calls.
