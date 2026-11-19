# CLI Commands Share One Shape

## Status

Accepted. Builds on ADR-0012, which moved the CLI to usage-rs.

## Context

lum's commands grew one subsystem at a time, and each picked its own shape. `lum vol 40` set the volume and `lum vol` showed it, `lum radio <code>` played a station and `lum radio rand` a random one, `lum backup bio` made a backup, and a bare `lum display` printed the monitors. yt had `aud`, `vid` and `alb`, git-id had `paths`, and `ls` stood in for `list` in places. `lum net game` and `lum net normal` named why one switches, not what switches. Each shape had to be learned on its own, and a word could be a command, a name or a number depending on where it stood.

## Decision

- **A group only holds commands.** Run bare, it prints its help and exits 2. It never reads its first word as a name or a number; those are arguments of a command: `lum radio play <code>`, `lum vol set <n>`, `lum backup create <target>`.
- **One spelling per command.** Whole words (`audio`, `video`, `album`, `random`, `list`, `public-key`, `--interface`) and no aliases. A group that manages a set of things is named in the plural: `apps`, `tools`, `fonts`, `repos`. A rename replaces the old name instead of keeping it as an alias, so help lists each command once. Unknown subcommands and flags are errors.
- **A shared name means the same thing in every group.** `list` shows a catalog, `status` shows state without changing it (given a name, that one in detail), `install`, `update`, `uninstall` and `sync` manage what lum installs, and `on` and `off` switch something lum can switch back. A command that prints a path is named for it: `config-path`, `dir`, `bin-dir`, `files`. `--dry-run` shows what `tools sync` and `apps sync`, which install releases, would do. `git-id sync` and `repos mirror sync` have none: `status` shows which identities or mirrors they would change.
- **Named for what it does, not why.** `lum net on` and `off`, not `game` and `normal` (ADR-0024).
- **Names complete.** An argument that takes a station, tool, app, font, env alias, identity, mirror tag or interface completes from lum's catalogs, the user's config files or the system, through usage-rs completers that the shell calls back into lum for. Other values complete nothing rather than file names: numbers, URLs, backup codes and secrets. A directory argument completes directories.
- **Output reads the same in every group.** `status` prints `Key: value` lines (`Volume: 40`, `Radio: stopped`). A state is the same word in every list: `installed`, `not installed`, `unmanaged`, `missing`, `unavailable`. A hint quotes the command to run in backticks (``Run `lum git-id init` to create it.``), and no message starts with a `[lum …]` tag.
- **Every command declares its effect** in the usage spec: `read`, `write` or `destructive`, the last for the uninstalls, `lum backup restore`, `lum env unset` and `lum git-id sync`, which deletes the keys of identities removed from its config. A side effect counts as much as the point of the command: `lum repos scan` is `write` because it fetches. A test fails when a command has none. Nothing in lum reads it; a shell wrapper or an agent's allowlist can.
- **The exit code says whether the command worked, not what it found.** 2 for anything the parser rejects: an unknown command or flag, a missing argument, a value of the wrong type or out of range (`lum vol set 101`). 1 for a failure while running, a name lum does not know included, whose message lists the names there are. Otherwise 0: `status` exits 0 whatever it reports, a mirror behind or missing included.

## Consequences

- Habits and scripts that used the old forms stop working, and they fail with "unrecognized subcommand" and exit 2 instead of doing something else. `lum env init`, which shell startup runs, kept its name.
- Later renames follow the same rule: `lum font` became `lum fonts`, `git-id info <identity>` became `git-id status <identity>`, `git-id pubkey` became `public-key` and `--iface` became `--interface`; the old forms fail with exit 2. `lum radio status` now prints `Radio: ` before the state, which a script that compared its output has to allow for.
- Some commands are a word longer: `lum vol set 40` for `lum vol 40`.
- Shaping switched on by a lum from before the rename is recorded in `net-game.json`, which `lum net off` no longer reads, so that lum has to switch it off.
- A script that read `lum repos mirror status`'s exit code to learn whether a mirror is behind reads its output instead. Exit 2 now only ever means a usage error.
- A new command picks its name from the ones already in use, declares its effect and, if it takes a name, gets a completer and reports a name it does not know through `unknown_name` in `src/cli.rs`.
