# Display Skips Retina Monitors

## Status

Accepted. Narrows ADR-0029 from every external monitor to the ones that aren't Retina. How `off` shows in the tray is in ADR-0022.

## Context

ADR-0029 switches every external monitor. A Retina one, such as a 4K or 5K monitor macOS runs at HiDPI by itself, has sharp text already: `on` would move it to a bigger HiDPI size, which makes everything smaller, and `off` would make it tiny. With such a monitor next to a 1440p one, `lum display` would switch both.

## Decision

- **Retina.** A monitor is Retina when the mode macOS picks for it by itself, the one IOKit flags `kDisplayModeDefaultFlag`, is HiDPI. Built-in screens always are. `lum display` leaves Retina monitors alone, and with only Retina screens connected it says it is disabled and fails. The flag is Apple's own call, so lum needs no density threshold, and physical sizes from EDID are often missing or wrong. Override files don't move it: on the 2560x1440 monitor lum was built on, the default stays the 1x mode with Crisp's file loaded, so lum keeps managing the monitor after `on`. Going by the current mode instead would lose every monitor `on` switched.
- **Several monitors.** Every monitor that isn't Retina is switched, each on its own. Each line starts with the monitor's name as System Settings shows it, numbered when two share a name. A monitor that fails doesn't stop the others: its error is printed after its name, and the command fails at the end.

## Consequences

- `lum display` no longer touches a 4K or 5K monitor, even for a game.
- NSScreen's display ID property needs macOS 26, so the names are matched through its device description (`NSScreenNumber`). Reading them adds `objc2`, `objc2-app-kit` and `objc2-foundation` as macOS dependencies, which tray-icon and winit build already.
