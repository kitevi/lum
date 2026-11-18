# Net Shaping Lights The Tray's Screen

## Status

Accepted. Extends the tray of ADR-0022 to the net shaping of ADR-0024.

## Context

ADR-0022's tray shows the states lum leaves running after a command returns: a radio, playing or paused, and a monitor with HiDPI off. `lum net on` (ADR-0024) is another. It holds the wired link at 100 Mb/s until `lum net off` or a reboot, and without a command it returns at once, so nothing on screen says it is still on.

`lum display off` is mostly used for gaming too, so the two usually come together, and a gamer is as likely to forget one as the other. One icon has to say "shaping is on", on any panel at 16 px, without costing the radio's or the display's states anything.

Options considered:

- The radio inside the monitor's screen: at 16 px it is a smudge in a box, and its antenna cue is lost.
- A game controller glyph: another shape to tell apart from the monitor and the radio at 16 px.
- A green screen: green already means "radio playing", so it would read as a playing radio.
- A second tray icon for shaping: more clutter on a bar, and some hosts hide extra icons.

## Decision

- **A lit screen is shaping.** The monitor's screen stays hollow for HiDPI off and is filled in blue while shaping is on. Fill against outline is the difference that survives at 16 px. Shaping draws the monitor whether or not HiDPI is off, so with both in force the screen is lit, and the menu and the tooltip, which have a line for each state, say what is on.
- **Blue is for the lit screen, green stays for "radio playing".** The blue is a mid-tone held to the same 3:1 contrast floor as the other fills. On macOS, where the menu bar tints a template, the menu bar ink draws it in Apple's system blue, as it draws the playing radio in green.
- **The radio stays as it was:** full size alone, or the badge in the monitor's bottom-right corner with its antenna, never inside the screen. A playing radio's antenna gap cuts into a lit screen as it does into a hollow one.
- **The display tray becomes the status tray.** The process ADR-0022 made for HiDPI now serves every state that no station owns: `__status_tray` and `status-tray.lock` replace `__display_tray` and `display-tray.lock`. `lum net on` starts it, as `lum display off` does, also when shaping was on already, and it runs on Linux as well as macOS. It exits once HiDPI is back on everywhere and shaping is off, and a station's tray shows its lines while it runs.
- **Shaping is read as `lum net status` reads it:** the record exists and the system agrees (ADR-0024). The tray only reports it. Switching back needs sudo, and so a terminal, which a tray item cannot give.

## Consequences

- Linux gets a tray process of its own, where before only a station's runner had a tray. The StatusNotifierItem backend can only mark an icon passive, which a host may ignore, so a hidden status tray takes its icon down there and builds it again when it shows.
- The status tray looks for the record every 2 seconds and asks the system about it, an `ifconfig -a` on macOS, at most every 10, so a tray that is already up for `lum display off` can show the shaping line that late. A new tray reads at once.
- The tray's id changes from `lum-display` to `lum-status`, so a host that remembers the user hiding the old icon does not apply that to the new one. While lum is being upgraded, an old display tray and a new status tray can both show for a moment.
- A lit screen says only that something is on. The menu and the tooltip say what.
- A tray host is still needed. Without one the icon is invisible and shaping is unaffected (ADR-0022).
- The tray cannot be exercised in CI, and the lit screen, the blue in the macOS menu bar and the status tray on Linux have not been seen on real hardware.
