# lum

`lum` is a small opinionated CLI toolbox for setting up shell environment variables, installing a curated set of developer tools and desktop apps, listening to internet radio stations or letting lum pick a random one, checking local Git repositories, and managing folder-based Git identities.

The most useful commands for new users are:

- `lum env` — manage API-key-style environment variables and add lum's managed tool directory to your shell `PATH`.
- `lum apps` — install and update desktop apps like mGBA on Linux, macOS and Windows, and keep their settings.
- `lum radio random` — play a random built-in station, never the one already playing.

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
eval "$(lum env init --shell bash)"
```

For zsh, add this to `~/.zshrc`:

```sh
eval "$(lum env init --shell zsh)"
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
| `baro` | `BARO_DB_URL` |
| `coralbricks` | `CORALBRICKS_API_KEY` |
| `deploy` | `DEPLOY_API_KEY` |
| `keenable` | `KEENABLE_API_KEY` |
| `opencode` | `OPENCODE_API_KEY` |
| `opendesign` | `OPENDESIGN_API_KEY` |
| `openrouter` | `OPENROUTER_API_KEY` |
| `terr` | `TERR_DB_URL` |

Examples:

```sh
lum env list
lum env set openrouter sk-or-...
lum env unset openrouter
lum env bin-dir
```

With the shell setup above, `lum env set` and `lum env unset` update the current shell at once. Programs that are already running keep the environment they started with.

## Managed tools

`lum tools` installs curated developer tools into lum's managed binary directory. The tool catalog is currently empty, so there is nothing to install yet. If you ran `lum env init` from your shell startup file, that directory is automatically on your `PATH`.

## Desktop apps

`lum apps` installs desktop apps from their upstream releases on Linux, macOS and Windows, per user and without admin rights, and keeps a few of their settings at lum's values.

Available apps:

| App | Description | Settings lum keeps |
| --- | --- | --- |
| `mgba` | Game Boy Advance emulator | `updateAutoCheck=0`, since lum does the updating |
| `snes9x` | Super Nintendo emulator | On Windows, saves go to `~\ROMs` instead of the program folder |

Where things go:

| | Linux | macOS | Windows |
| --- | --- | --- | --- |
| App | `~/.local/share/lum/apps/<app>` | `~/Applications/lum/<App>.app` | `%LOCALAPPDATA%\Programs\lum\<app>` |
| Launcher | `~/.local/share/applications/lum-<app>.desktop` | Launchpad and Spotlight | Start Menu, `lum` folder |

Common commands:

```sh
lum apps list
lum apps install mgba
lum apps status snes9x        # installed and latest version, settings
lum apps update mgba
lum apps sync
lum apps sync --dry-run
lum apps uninstall snes9x
```

Notes:

- lum sets only the settings it lists, and touches only those lines of the config file. Everything else (controls, BIOS paths, recent games) stays yours. `lum apps sync` puts a changed lum setting back.
- lum does not update an app, or change its settings, while it is running. Quit it first.
- On Linux the AppImage is unpacked, so it runs without FUSE. Snes9x's AppImage still needs glibmm 2.4 from the system; on Arch, `sudo pacman -S glibmm`.
- `lum apps uninstall` removes the app and its launcher, and keeps its saves. Its settings stay too, except snes9x's on Windows, which live in its program folder.
- Keep ROMs in `~/ROMs`: `lum backup create roms` backs them up with their saves, and `lum backup restore roms <code>` restores them on another machine. mGBA saves beside each ROM everywhere, and so does snes9x on Linux; on Windows lum points snes9x's saves at `~\ROMs`. Snes9x on macOS always saves to `~/Library/Application Support/Snes9x`, which that backup does not cover. Quit the emulator first.

## Other commands

### Radio

List stations:

```sh
lum radio list
```

Play a station by code:

```sh
lum radio play <station>
```

Play a random station, never the one already playing:

```sh
lum radio random
```

While a station plays, a system tray icon offers a Stop/Play toggle. It needs a tray host: Windows, macOS, KDE and most Linux bars have one, and stock GNOME needs the AppIndicator extension. Without one the icon is simply absent and playback is unaffected. `lum radio status` reports `playing`, `paused` or `stopped`, and `lum radio stop` ends playback and removes the icon.

### Repositories

Scan a directory tree (the current one by default) for Git repository status:

```sh
lum repos scan [path]
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
lum git-id status [identity]
lum git-id where
lum git-id public-key <identity>
lum git-id files
```

See `src/git_id/README.md` for config shape, plaintext-credential warnings, ownership markers, generated files, and routing behavior.

### Network shaping

Linux and macOS only. `lum net on` forces the wired link to 100 Mb/s and shapes downloads just under that, for game streaming and anything else that suffers on a line whose router drops bursts at gigabit:

```sh
lum net on -- flatpak run com.nvidia.geforcenow       # Linux: any command; switch back when it exits
lum net on -- open -W -a "GeForce NOW"                # macOS
lum net on                                            # or switch on and off by hand
lum net off
lum net status
lum net test                                          # latency idle and under load, receive errors
```

Switching asks for your sudo password, so run it from a terminal. The network changes do not persist, so a reboot also switches them back. While shaping is on, a tray icon says so: a monitor with a blue screen. See `src/net/README.md`.

### Display (macOS)

Sharp text on monitors macOS doesn't treat as Retina, such as 27-inch 2560x1440 ones, and back to plain resolution for games. Retina screens are left alone. No helper app keeps running for HiDPI; while it is off, a tray icon of a monitor says so.

```sh
lum display on     # HiDPI at about the monitor's own size; the first time it asks for your password and a restart
lum display off    # plain resolution until you restart or log out: least GPU work, for games
lum display status # show which one is on
lum display srgb   # the sRGB color profile where none was picked; asks first
```

On Apple silicon, macOS won't do HiDPI at exactly the monitor's own size, so `on` uses the closest size under it: 2544x1431 on a 2560x1440 monitor and 1904x1071 on a 1920x1080 one. Everything looks 0.6% or 0.8% bigger than with HiDPI off, and `lum display status` shows the difference.

`lum display srgb` gives the same monitors the sRGB color profile, unless one was picked for them in System Settings > Displays > Color profile. It asks before it changes each one.

See `src/display/README.md`.

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
