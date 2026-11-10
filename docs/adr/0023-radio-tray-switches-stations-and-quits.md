# The Radio Tray Switches Stations and Quits

Status: Accepted. Supersedes the single-item menu and the "no Quit item" rule of ADR-0022.

## Context

ADR-0022 gave the station runner a tray with one Stop/Play item and deliberately no Quit. In use, the two things that still send you back to a terminal are changing station and getting rid of a paused tray. We want both in the tray menu, still without a daemon or IPC.

A runner is bound to one station: its kind picks the runner command (direct or playlist), and `radio-player.json` names its PID and station.

Options considered for switching:

- Retune the live runner in place: the supervisor, the state file and the tray would all need to change station mid-flight, and direct and playlist stations would have to share one runner.
- Spawn `lum radio <code>` from the runner: that command stops the remembered runner tree first, which would include the very process it was spawned from.
- The runner starts its successor and exits: the same steps as `lum radio <code>`, in the opposite order.

## Decision

The menu is `Stop/Play <station>`, a `Stations` submenu, `Random station` and `Quit radio`.

- **Switching hands over to a new runner.** The runner starts the chosen station's runner, writes the state file for it, then silences itself and exits. The successor shows its own tray. Starting the successor first means a failed start leaves the current station playing. `Random station` uses the `lum radio rand` rules.
- **Quit is `lum radio stop` from the tray.** The runner removes the state file only if it still names its own PID, kills its ffplay and exits.
- **Exit without joining the supervisor.** `Playback::halt` kills ffplay and parks the supervisor as a pause would, without notifying the pause observer, and the runner returns from `main`. The supervisor loops keep a single way to end.
- **The station list is a submenu** of plain names in catalog order, with the current station checked and disabled. The catalog is too long for the top level, and tags would make the menu wide.

## Consequences

- No new crates: submenus, check items and separators come with tray-icon's menu types, and the `ksni` backend maps all three.
- The tray icon disappears and reappears on a switch, because it belongs to a new process. On Linux the gap is tens of milliseconds.
- For an instant during a switch both runners exist; the old ffplay is killed right after the new runner is spawned, and a live station's 5s of leading silence (ADR-0020) covers the overlap.
- A playlist runner that is resolving a track with yt-dlp when it exits leaves that yt-dlp to finish by itself.
- Quit and switch leave through the normal return path, so on Windows the icon is removed properly, unlike `lum radio stop` (ADR-0022's TerminateProcess caveat).
- As with ADR-0022, the macOS and Windows paths have not been run.
