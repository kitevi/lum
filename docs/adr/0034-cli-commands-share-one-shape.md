# CLI Commands Share One Shape

## Status

Accepted. Builds on ADR-0012, which moved the CLI to usage-rs.

## Context

lum's commands grew one subsystem at a time, and each picked its own shape. `lum vol 40` set the volume and `lum vol` showed it, `lum radio <code>` played a station and `lum radio rand` a random one, `lum backup bio` made a backup, and a bare `lum display` printed the monitors. yt had `aud`, `vid` and `alb`, git-id had `paths`, and `ls` stood in for `list` in places. `lum net game` and `lum net normal` named why one switches, not what switches. Each shape had to be learned on its own, and a word could be a command, a name or a number depending on where it stood.

## Decision

- **A group only holds commands.** Run bare, it prints its help and exits 2. It never reads its first word as a name or a number; those are arguments of a command: `lum radio play <code>`, `lum vol set <n>`, `lum backup create <target>`.
- **One spelling per command.** Whole words (`audio`, `video`, `album`, `random`, `list`) and no aliases. A rename replaces the old name instead of keeping it as an alias, so help lists each command once. Unknown subcommands and flags are errors.
- **A shared name means the same thing in every group.** `list` shows a catalog, `status` shows state without changing it, `install`, `update`, `uninstall` and `sync` manage what lum installs, and `on` and `off` switch something lum can switch back. A command that prints a path is named for it: `config-path`, `dir`, `bin-dir`, `files`.
- **Named for what it does, not why.** `lum net on` and `off`, not `game` and `normal` (ADR-0024).
- **Names complete.** An argument that takes a station, tool, app, font, env alias, identity, mirror tag or interface completes from lum's catalogs, the user's config files or the system, through usage-rs completers that the shell calls back into lum for.
- **Every command declares its effect** in the usage spec: `read`, `write` or `destructive`, the last for the uninstalls, `lum backup restore` and `lum env unset`. A test fails when a command has none. Nothing in lum reads it; a shell wrapper or an agent's allowlist can.

## Consequences

- Habits and scripts that used the old forms stop working, and they fail with "unrecognized subcommand" and exit 2 instead of doing something else. `lum env init`, which shell startup runs, kept its name.
- Some commands are a word longer: `lum vol set 40` for `lum vol 40`.
- Shaping switched on by a lum from before the rename is recorded in `net-game.json`, which `lum net off` no longer reads, so that lum has to switch it off.
- A new command picks its name from the ones already in use, declares its effect and, if it takes a name, gets a completer.
