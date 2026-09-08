# Env Subcommand

`lum env` manages a small, hardcoded set of shell environment aliases and adds lum's bin directory to `PATH`.

## CLI Shape

```sh
eval "$(lum env init)"
lum env set openrouter sk-or-v1-xxxx
lum env unset openrouter
lum env list
lum env aliases
lum env path
```

`lum env init` emits shell code. On Unix shells, put `eval "$(lum env init)"` in `.bashrc`, `.zshrc`, or another POSIX-ish shell startup file. The emitted wrapper makes `lum env set` and `lum env unset` affect the current shell immediately without re-sourcing the startup file.

Scope: the new value applies to the current shell immediately and to new shells (which replay state via `init`). Already-running processes (editors, agents like `pi`, daemons, other terminals) keep the environment they were started with — Unix copies env at spawn. Restart just that program from an updated shell to pick up the change; no reboot needed.

The same init output also wires shell completions for bash and zsh by evaluating `lum __completions <shell>` for the active shell. Generated scripts call back into `lum` at completion time for candidates.

On Windows, PowerShell is the supported shell. Add this to `$PROFILE`:

```powershell
Invoke-Expression (& lum env init)
```

For testing or explicit selection, `lum env init`, `lum env set`, and `lum env unset` accept `--shell posix` or `--shell powershell`. The default is PowerShell on Windows and POSIX elsewhere.

`lum __completions <shell>` is a hidden internal command backed by `usage-rs`; it supports `bash`, `zsh`, `fish`, `elvish`, and `powershell`.

## Storage

Use lum's centralized platform directory policy (`directories::ProjectDirs::from("dev", "ppowo", "lum")`):

- state: `config_dir()/env-state.json`
- bin directory: `data_dir()/bin`

`lum env path` only prints the bin directory. `lum env init` creates it and prepends it to `PATH` with deduplication.

No vex state is read or migrated. If moving from vex, set values again manually with `lum env set`.

## Aliases

Aliases are hardcoded in source by design. Unknown aliases are rejected to avoid typo-created persistent environment variables.

Current aliases:

| Alias | Variable |
|-------|----------|
| baro | BARO_DB_URL |
| deepseek | DEEPSEEK_API_KEY |
| exa | EXA_API_KEY |
| hypercharm | HYPERCHARM_API_KEY |
| neuralwatt | NEURALWATT_API_KEY |
| opencode | OPENCODE_API_KEY |
| openrouter | OPENROUTER_API_KEY |
| synthetic | SYNTHETIC_API_KEY |
| terr | TERR_DB_URL |
| zro | ZRO_API_KEY |

## Shell Output

Commands intended for eval write shell statements to stdout and human messages to stderr.

POSIX exports use single-quote escaping, including embedded single quotes:

```sh
export OPENROUTER_API_KEY='abc'\''def'
```

PowerShell exports use PowerShell single-quote escaping:

```powershell
$env:OPENROUTER_API_KEY = 'abc''def'
```

## Forced Defaults

`lum env init` also emits these forced defaults. They are lum-managed, not user-configurable env-state entries:

| Variable | Value |
|----------|-------|
| npm_config_ignore_scripts | true |

## Listing

`lum env list` shows every variable lum manages: alias-backed variables plus forced defaults. Persisted alias values are masked by default because they are usually secrets.
