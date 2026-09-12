# lum

`lum` is a small opinionated CLI toolbox for setting up shell environment variables, installing a curated set of developer tools and macOS apps, listening to a few internet radio stations, checking local Git repositories, and managing folder-based Git identities.

The most useful commands for new users are:

- `lum env` — manage API-key-style environment variables and add lum's managed tool directory to your shell `PATH`.
- `lum tools` — install and update curated CLI tools like `scc` and `universal-ctags`.
- `lum apps` — install and update macOS apps like OpenEmu that Homebrew disabled (macOS only).

## Install

Download the latest binary for your platform from the GitHub Releases page, then put it somewhere on your `PATH`.

### What is `PATH`?

Your `PATH` is the list of folders your terminal searches when you type a command. If the `lum` binary is inside one of those folders, you can run it from anywhere by typing:

```sh
lum --help
```

### macOS / Linux

1. Download the Linux or macOS binary from Releases.
2. Rename it to `lum` if needed.
3. Move it into `~/.local/bin`:

```sh
mkdir -p ~/.local/bin
mv ~/Downloads/lum-* ~/.local/bin/lum
chmod +x ~/.local/bin/lum
```

4. Make sure `~/.local/bin` is on your `PATH`.

For bash, add this to `~/.bashrc`:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

For zsh, add this to `~/.zshrc`:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Then restart your terminal, or run:

```sh
source ~/.bashrc   # bash
# or
source ~/.zshrc    # zsh
```

Check that it works:

```sh
lum --help
```

### Windows PowerShell

1. Download the Windows binary from Releases.
2. Rename it to `lum.exe` if needed.
3. Create a tools folder, for example:

```powershell
New-Item -ItemType Directory -Force "$HOME\bin"
Move-Item "$HOME\Downloads\lum-windows-x86_64.exe" "$HOME\bin\lum.exe"
```

4. Add that folder to your user `PATH`:

```powershell
[Environment]::SetEnvironmentVariable(
  "Path",
  [Environment]::GetEnvironmentVariable("Path", "User") + ";$HOME\bin",
  "User"
)
```

Restart PowerShell, then check:

```powershell
lum --help
```

## Shell setup with `lum env`

`lum env init` prints shell code that:

- adds lum's managed binary directory to `PATH`
- exports environment variables you set with `lum env set`
- applies lum's built-in default environment variables

Run this once in your shell startup file.

### macOS / Linux

For bash, add this to `~/.bashrc`:

```sh
eval "$(lum env init --shell posix)"
```

For zsh, add this to `~/.zshrc`:

```sh
eval "$(lum env init --shell posix)"
```

For fish, add this to `~/.config/fish/config.fish`:

```fish
lum env init --shell fish | source
```

Then restart your terminal.

### Windows PowerShell

Add this to your PowerShell profile:

```powershell
lum env init --shell powershell | Invoke-Expression
```

If you do not know where your profile is, run:

```powershell
$PROFILE
```

Create it if it does not exist:

```powershell
New-Item -ItemType File -Force $PROFILE
notepad $PROFILE
```

## Environment variables

`lum env` stores common secrets behind short aliases.

Available aliases:

| Alias | Environment variable |
| --- | --- |
| `deepseek` | `DEEPSEEK_API_KEY` |
| `exa` | `EXA_API_KEY` |
| `hypercharm` | `HYPERCHARM_API_KEY` |
| `neuralwatt` | `NEURALWATT_API_KEY` |
| `opencode` | `OPENCODE_API_KEY` |
| `openrouter` | `OPENROUTER_API_KEY` |
| `synthetic` | `SYNTHETIC_API_KEY` |
| `zro` | `ZRO_API_KEY` |

Examples:

```sh
lum env aliases
lum env set openrouter sk-or-...
lum env list
lum env unset openrouter
lum env path
```

After setting or unsetting a value, restart your terminal or re-run your shell init command so the environment updates in the current shell.

## Managed tools

`lum tools` installs curated developer tools into lum's managed binary directory. If you ran `lum env init` from your shell startup file, that directory is automatically on your `PATH`.

Available tools:

| Tool | Binary | Description |
| --- | --- | --- |
| `scc` | `scc` | Code counter with complexity |
| `universal-ctags` | `ctags` | Source code indexer |

Common commands:

```sh
lum tools ls
lum tools install scc
lum tools status scc
lum tools version scc
lum tools update scc
lum tools sync
lum tools sync --dry-run
```

## macOS apps

`lum apps` installs and updates macOS applications that Homebrew can no longer distribute (Homebrew 5.0 requires signed and notarized casks). Apps are installed per-user into `~/Applications/lum` — no admin rights needed — and appear in Launchpad and Spotlight like any other app.

Available apps:

| App | Bundle | Description |
| --- | --- | --- |
| `openemu` | `OpenEmu.app` | Open source video game emulation for macOS |

Common commands:

```sh
lum apps ls
lum apps install openemu
lum apps status openemu
lum apps version openemu
lum apps update openemu
lum apps sync
lum apps sync --dry-run
```

Notes:

- Apps are unsigned; files downloaded by lum never get the `com.apple.quarantine` attribute, so they open normally.
- If an unmanaged copy of the app already exists (for example an old Homebrew cask install in `/Applications`), lum refuses to install alongside it. Remove the old copy first, e.g. `brew uninstall --cask openemu`, or see `src/apps/README.md` for the exact rules.
- OpenEmu is currently an Intel-only build; on Apple Silicon lum prints the one-time Rosetta 2 install command after installing.

Pair `lum apps` with `lum backup openemu` to also back up emulator save states and configs.

## Other commands

### Radio

List stations:

```sh
lum radio
```

Play a station by code:

```sh
lum radio <station>
```

### Repositories

Scan a directory tree for Git repository status:

```sh
lum repos scan <directory>
```

By default this fetches each current branch's upstream remote before reporting ahead/behind status. Use `--offline` to compare against cached remote refs only.

Mirror configured repositories:

```sh
lum repos mirror init
lum repos mirror config-path
lum repos mirror sync
lum repos mirror status
```

### Git identities

Manage folder-based Git author identities with SSH or declarative HTTP Basic authentication:

```sh
lum git-id init
lum git-id config-path
lum git-id sync
lum git-id status
lum git-id where
lum git-id info <identity>
lum git-id pubkey <identity>
lum git-id paths
```

See `src/git_id/README.md` for config shape, plaintext-credential warnings, ownership markers, generated files, and routing behavior.

## Build from source

You need Rust installed.

```sh
git clone https://github.com/ppowo/lum.git
cd lum
cargo build --release
```

For local development installs in this repository, use:

```sh
cargo local-install
```

## License

See [`LICENSE`](LICENSE).
