# The Tray

## Status

Accepted. Reverses the "no paused state" part of ADR-0010 and builds on the runner of ADR-0014. ADR-0033 adds net shaping to the status tray and a lit screen to its icon.

## Context

`lum radio play <code>` starts a station and returns to the shell, and `lum display off` lasts until logout. Neither shows on screen, so stopping a station, changing it or noticing that HiDPI is still off means going back to a terminal. We want one tray icon for both, on Linux, macOS and Windows, without a daemon (`CONTEXT.md` avoids "Radio daemon").

A long-lived process already exists while a station is loaded: ADR-0014's hidden `__radio_direct_runner` supervisor, the same `lum` binary, owning the whole ffplay lifecycle.

Options considered:

- A separate tray daemon: a second program to start, find and clean up, plus an IPC protocol that ADR-0002 declined to own.
- tray-icon's default Linux backend (GTK 3 and libappindicator): the binary would hard-link GTK and about 70 shared libraries, so every lum command would refuse to start on a Linux machine without GTK 3, and CI would need GTK development packages.
- The station runner hosting the tray, with a small process of its own, the status tray, for the states no station owns: `lum display off` and, since ADR-0033, `lum net on`.

## Decision

**Hosts**

- While a station is loaded, its runner hosts the tray. Its main thread owns the tray and every menu object, because macOS requires it; the supervisor runs on a worker thread that starts first, so the tray never delays playback. They share only an in-process `Playback` handle: the pause flag and the running ffplay.
- Otherwise the status tray hosts it, the hidden `__status_tray` command. `lum display off` starts it on macOS and `lum net on` on Linux and macOS (ADR-0033), in a process group of its own so that the terminal that started it cannot take it down. A lock file keeps it to one. It hides while a station is loaded and exits once HiDPI is back on everywhere and shaping is off, however that happened, so like a runner it lives exactly as long as the state it shows.
- Nothing passes between the two: every 2 seconds each reads what it shows: the station from `radio-player.json`, and the status lines at most every 10 seconds, since reading every mode of every monitor, or asking `ifconfig` whether shaping is on, is costly for a state that changes a few times a day. A station's tray shows the lines only while the status tray runs, so a monitor merely left at its plain resolution shows nothing.
- The tray is optional: if it can't be created (no D-Bus session, no tray host, no window server), the runner logs a warning and keeps playing.

**Menu**

- While HiDPI is off (macOS) or shaping is on, the menu starts with a disabled line for each, a monitor's worded as `lum display` prints it, and a separator.
- The radio's part is `Stop/Play <station>`, a `Stations` submenu and `Random station`, then `Quit radio`.
- **Stop and Play.** Stop kills ffplay, through `shared_child` so it can be killed while the supervisor waits on it; Play respawns it. A pause is not a stream failure: it doesn't back off or count toward giving up, backoff sleeps wake early on it, and resuming starts a fresh failure streak. The runner, and so the tray, outlives a pause; `radio-player.json` gains `paused`, written only by the runner whose PID it names, and `lum radio status` prints `paused <code> <name>`.
- **Switching hands over to a new runner.** The runner starts the chosen station's runner, writes the state file for it, then silences itself and exits; the successor shows its own tray. Spawning `lum radio play <code>` from the runner would kill the process that spawned it, so the runner starts its successor itself. Starting the successor first means a failed start leaves the current station playing. `Random station` follows the `lum radio random` rules. The station list is a submenu of plain names in catalog order, the current one checked and disabled.
- **Quit is `lum radio stop` from the tray.** The runner removes the state file only if it still names its own PID, kills ffplay and returns from `main` without joining the supervisor, which `Playback::halt` parks as a pause would.
- Clicking the icon itself is not a portable gesture across tray hosts, so everything is a menu item.

**Icon**

- One square icon, drawn in code on a 16-unit grid: a radio while a station is loaded, a monitor while HiDPI is off or shaping is on, and with both, a smaller radio over the monitor's bottom-right corner, cut out of it with a gap the way SF Symbols draws its badges. Under the radio the monitor drops its stand, which the radio would cut in two. The monitor's screen is hollow for HiDPI off and filled in blue for shaping (ADR-0033).
- The radio's antenna is up while playing and folded while paused, so the state shows in the silhouette and not only in colour. A playing radio is green, and nothing else is.
- macOS gets a template image the menu bar tints. A template has no colour, so while the radio plays or the screen is lit lum draws the tint itself: it asks the icon's own button whether the menu bar is dark (its appearance follows the wallpaper as well as the system setting), draws the monitor and the paused parts black or white as the template would be, and the radio and the lit screen in Apple's system green and blue for that appearance. The watch asks again every 2 seconds and redraws when it changes.
- Elsewhere the host paints the icon over a panel lum can't see, so it is drawn in mid-tone colours, green while playing, blue for the lit screen and grey otherwise, all above 3:1 contrast on light and dark panels, with a faint white rim for mid-tone panels.
- Rejected: the monitor and the radio side by side, which is wide and reads as two icons; a monitor as a badge on the radio, a few pixels that read as a notification; one merged glyph, which reads as neither; a neutral icon with the states only in the menu.

**Crates**

`tray-icon` with its `ksni` backend on Linux: StatusNotifierItem over D-Bus, pure Rust, no GTK; zbus is already in the tree through `notify-rust`. On Windows and macOS, `winit` (the blessed.rs default) supplies the event loop. The default GTK backend stays off.

## Consequences

- Cost on Linux: 21 more crates and about 690 KB more binary, with no new shared libraries or CI packages. Windows adds 14 crates and macOS 33.
- ADR-0010's two-state lifecycle no longer holds: a loaded station is playing or paused. The runner owns the flag and nothing is added to the CLI. ADR-0002 still holds: ffplay plays the audio.
- Resume rejoins the live stream, so ADR-0020's 5s of leading silence returns each time.
- The icon needs a tray host. Stock GNOME has none without the AppIndicator extension; without a host the icon is invisible and playback is unaffected.
- A switch removes the icon and shows a new one, because it belongs to a new process. For an instant both runners exist; a station's 5s of leading silence covers it.
- For up to 2 seconds after a station starts or stops, both icons can show, or the status lines can be missing. A change of state shows within 10 seconds. A status tray that leaves because nothing is in force any more checks once more after releasing its lock and starts a new one if something is, so a `lum display off` or `lum net on` racing its exit still gets an icon. A status tray with shaping off whose monitors are all unplugged stays, hidden, until one returns or the session ends.
- The StatusNotifierItem backend can only mark a hidden icon passive, which a host may ignore, so on Linux a hidden status tray takes its icon down and builds it again when it shows.
- On macOS, after the menu bar turns light or dark while the radio plays or the screen is lit, the icon can keep the old tint for up to 2 seconds.
- Display lines exist only on macOS, and shaping's only on Linux and macOS, so on Windows the tray is the radio's alone.
- Windows: `lum radio stop` ends the runner with TerminateProcess, which can leave a dead icon in the notification area until the mouse passes over it. Quit and switch leave through the normal return path and remove it properly.
- Release builds use `panic = "abort"`, and macOS menus panic off the main thread, so all menu work stays on one thread.
- The tray can't be exercised in CI. The macOS and Windows radio paths have not been run.
