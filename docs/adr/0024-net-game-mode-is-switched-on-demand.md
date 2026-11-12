# Net Game Mode Is Switched On Demand

## Status

Accepted

## Context

On the line this was built for, GeForce NOW's network test reported about 2% packet loss at gigabit. The loss did not depend on the route (a VPN to another country gave the same) or on the stream bitrate (a 50 Mb/s cap gave the same), and CRC errors on the cable were far too rare to explain it. Forcing the link to 100 Mb/s brought the loss to 0%: with the link slower than the line, the router queues bursts that it otherwise drops. The same queue then fills during any large download, adding 250–400 ms of latency, so 100 Mb/s alone trades loss for lag.

Permanent fixes were rejected:

- A NetworkManager profile and dispatcher hook pinned at 100 Mb/s: Linux only, and every download is capped at 100 Mb/s all the time.
- A router running SQM between the ISP router and the computers: extra hardware.

The same cable is used by a Linux PC and a Mac.

## Decision

`lum net game` switches to game mode on demand, `lum net normal` switches back, and `lum net game <command>` does both around a command such as the GeForce NOW client.

- **100 Mb/s with autonegotiation still on.** Linux advertises only 100baseT/Full with ethtool. macOS sets the media with `ifconfig`. Forcing the speed with autonegotiation off would leave the router at half duplex.
- **Downloads are shaped to 90 Mbit/s on the computer**, so the queue builds where a shaper keeps it short. Linux redirects ingress to an IFB device running CAKE. macOS has no CAKE: a dummynet pipe with a 50-packet queue does the same main job, without per-flow fairness.
- **The network changes are runtime state only.** Nothing touches persistent network configuration (`networksetup -setMedia`, NetworkManager profiles, `/etc/pf.conf`), so a reboot always restores the network. On macOS the pipe rule goes into the `com.apple/lum` anchor that the stock `pf.conf` already evaluates, and pf is enabled through a `pfctl -E` reference released with `-X`. The main ruleset, which VPN clients hook into, is never reloaded.
- **Root through sudo, per command.** lum stays unprivileged and runs only the switching commands through `sudo`, with credentials refreshed while a wrapped command runs.
- **A record names what changed.** `net-game.json` holds the interface name and MAC before anything changes, so `normal` undoes it after a rename or replug, from another terminal, or after a crash. Whether game mode is still on is read from the system, so a record left behind by a reboot reads as off.
- **`lum net game gfn` launches GeForce NOW** from Flatpak in game mode, on Linux with GNOME only, with Dash to Panel disabled until no GeForce NOW instance is left. The extensions lum disabled go into the same record, so one `lum net normal` undoes everything.
- **`lum net test` measures instead of guessing.** It times TCP handshakes idle and under a three-server download and reads the interface's receive errors. The handshakes need no root on either system, unlike sub-second `ping` on macOS.

## Consequences

- Game mode is opt-in per session. Downloads run at full speed otherwise.
- `signal-hook` becomes a direct dependency on Unix so Ctrl+C, SIGTERM and SIGHUP cannot kill lum before it switches back. It was already in the tree through `shared_child`, so nothing new is downloaded.
- sysinfo's `network` feature supplies interface counters on both systems.
- Shaping on macOS depends on the stock `dummynet-anchor "com.apple/*"` line. Without it, shaping silently does nothing, and `lum net test` is how to notice.
- If the terminal closes while a wrapped command runs, lum still brings the desktop back. The network usually stays in game mode until `lum net normal`: sudo keeps its cached credentials per terminal by default, so switching back would have to ask for the password and has no terminal to ask on.
- `gfn` is a reserved word after `lum net game`, so a program named `gfn` can only be run by its path.
- Disabling a GNOME extension is saved in GNOME's settings, unlike the network changes. A record left by a killed session is therefore cleaned up rather than ignored, both by `lum net normal` and at the start of the next `game`.
- The macOS path has not been run on real hardware.
