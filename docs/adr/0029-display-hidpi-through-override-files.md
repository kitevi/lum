# Display HiDPI Through Override Files

## Status

Accepted. ADR-0030 narrows it to monitors that aren't Retina; ADR-0022 shows `off` in the tray.

## Context

macOS draws text sharply only in HiDPI modes, which use 2x2 pixels for every point. It offers them only for monitors it treats as high-density, so a 27-inch 2560x1440 monitor gets none and its text looks soft. Crisp, Acuity, BetterDisplay and one-key-hidpi add them. Some tools do it with a virtual display that mirrors the real one, which keeps an app running and costs GPU time. Crisp and one-key-hidpi write an override file instead, which macOS reads when the monitor connects and then offers the sizes as ordinary modes.

The HiDPI size worth having is the one closest to the panel's own, so things look as big as without HiDPI, only sharper. System Settings and Crisp's size picker hide it, because it shares its size with a 1x mode. HiDPI also draws four times the pixels, which games feel, and turning it off and back on meant going through System Settings each time.

## Decision

- `lum display` is macOS only and has two states for every external monitor. `on` switches to the biggest HiDPI size no bigger than the panel: the panel's own size where macOS offers it at 2x, else the closest size under it (2544x1431 on a 2560x1440 panel). `off` switches to the panel's own resolution without HiDPI. lum offers no other sizes.
- When `on` finds no such size, it writes the override file, as Crisp does: no virtual display, and nothing keeps running after the command ends. The file format and the 16-point size list follow Crisp (MIT). A file already there, Crisp's included, is kept.
- Modes, the hidden ones included, are listed and switched through CoreGraphics with `objc2-core-graphics`, which the tray already pulls in, and the file is read and written with `plist`. Both are macOS-only target dependencies, so Linux and Windows builds never compile them.
- The override folder belongs to root. lum runs `sudo install` for the file instead of asking to run as root, so the password prompt is the only privileged step.
- `on` is saved for good, so it survives restarts. `off` lasts only until the user logs out or restarts, so forgetting to turn HiDPI back on after a game costs nothing. `off` keeps the file, so `on` needs no password or restart afterwards.

## Consequences

- No app has to stay installed or running for HiDPI. Crisp can go.
- On Apple silicon `on` can't match the panel's own size exactly, so everything looks a little bigger than with HiDPI off: 0.6% on a 2560x1440 monitor and 0.8% on a 1920x1080 one. That is small enough to accept, and `lum display status` shows it.
- After lum installs the file, macOS needs a restart or a replug before `on` can switch, since it reads the file only when the monitor connects.
- Reinstalling or resetting macOS can drop `/Library/Displays`. Run `lum display on` again and restart.
- Built-in screens and monitors Apple ships an override for are not touched.
- Other HiDPI sizes are left to System Settings.
