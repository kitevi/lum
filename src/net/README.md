# Net Subcommand

`lum net` switches the wired connection between normal and game mode. Linux and macOS only; on Windows every command refuses to run. ADR-0024 has the reasoning.

## Why Game Mode Exists

On the fiber line this was built for, game streams lost about 2% of their packets at gigabit, over any route (a VPN did not help) and at any stream bitrate (a 50 Mb/s cap did not help). At 100 Mb/s loss was 0%: with the link slower than the line, the router queues bursts instead of dropping them. That same queue then fills during big downloads (+250 ms of latency), so game mode also shapes downloads just under the link speed, which keeps the router's queue empty.

## CLI Shape

```sh
lum net game [--iface <name>] [[--] <command>...]
lum net game [--iface <name>] gfn
lum net normal
lum net status
lum net test [--iface <name>] [--seconds <5-120>]
```

- `game` forces the link to 100 Mb/s and shapes downloads to 90 Mbit/s.
- `game <command>` also runs the command, then switches back when it exits, e.g. `lum net game flatpak run com.nvidia.geforcenow` or `lum net game open -W -a "GeForce NOW"`. The command runs as the user; only the switches use root. The `--` is optional: the first word that is not a flag starts the command. The command is looked up before anything changes, so a mistyped name does not cost a switch on and back off.
- With game mode already on, `game` leaves it as it is, and `--iface` naming another interface is an error: there is one `lumifb0` and one record, so `lum net normal` comes first.
- `game gfn` runs GeForce NOW in game mode with Dash to Panel off; see below. `gfn` is a reserved word, matched before the command catch-all, like `lum radio rand`.
- `normal` undoes whatever the last `game` recorded, Dash to Panel included. Without a record it says so and changes nothing.
- `status` prints whether game mode is on, then each wired interface with its link and the receive errors it has counted since boot.
- `test` measures latency for 5 s idle and while downloading from three public test servers, and counts receive errors meanwhile. It needs no root and changes nothing.

## What Game Mode Changes

| | Linux | macOS |
|---|---|---|
| 100 Mb/s | `ethtool -s <if> speed 100 duplex full autoneg on advertise 0x008` | `ifconfig <if> media 100baseTX mediaopt full-duplex` |
| Back | `ethtool -s <if> autoneg on` | `ifconfig <if> media autoselect` |
| Shaping | the interface's ingress is redirected to the `lumifb0` IFB device, whose root qdisc is `cake bandwidth 90mbit besteffort ingress` | `dnctl` pipe 64242 (`bw 90Mbit/s queue 50`), fed by `dummynet in quick on <if> all pipe 64242` in the `com.apple/lum` pf anchor, with pf enabled through a `pfctl -E` reference |
| Unshaping | delete the ingress qdisc and `lumifb0` | load an empty ruleset into the anchor, delete the pipe, `pfctl -X <token>` |

Everything in this table is runtime state, so a reboot is also a way back to normal. That is why macOS uses `ifconfig` and not `networksetup -setMedia`, which saves the media in the network preferences.

- The 100 Mb/s mode keeps autonegotiation on and advertises only 100baseT/Full. Forcing the speed with autonegotiation off would make the router fall back to half duplex.
- Shaping goes on after the link is back at 100 Mb/s: on macOS a link change can reload pf, which would drop an anchor loaded before it. A link that is not at 100 Mb/s within 15 s is an error, and lum switches back. Switching off removes shaping first, so a failure part-way leaves a slow link. Either way a shaper never sits on a gigabit link.
- On Linux, switching off deletes the ingress qdisc before `lumifb0`. An interface that still redirects to a deleted `lumifb0` has everything it receives dropped by the kernel.
- `lumifb0` is a fixed name: `ifb4<if>` would exceed the 15-character limit for USB adapters, whose names are already 15 characters long (`enx` plus the MAC).
- macOS has no CAKE and its dummynet has no fq_codel, so the short 50-packet queue stands in for fair queueing. It keeps the router's queue empty all the same.
- lum never edits `/etc/pf.conf` or reloads the main ruleset, which VPN clients and other tools hook into. The stock `pf.conf` already evaluates `dummynet-anchor "com.apple/*"`. If someone removed that line, shaping silently does nothing, and `lum net test` shows it.

## Choosing The Interface

Interface names are read fresh on every run, never stored as the identity: macOS numbers adapters in the order it first saw them, and a USB adapter on Linux is named after its MAC.

- Linux reads `/sys/class/net`: an interface is wired Ethernet when it has a `device` (physical), no `wireless` or `phy80211` (not Wi-Fi), and `type` 1 (Ethernet framing). `carrier` and `speed` give the link.
- macOS reads `ifconfig -a`: an interface is wired when its media names an Ethernet type such as `1000baseT`; Wi-Fi only shows `autoselect`. `status: active` is the link.
- With one wired link, that is the one. With several, the one carrying the default route (`/proc/net/route`, `route -n get default`), else lum stops and asks for `--iface`. Without a wired link lum stops: game mode is for wired connections only.

## GeForce NOW

`lum net game gfn` is Linux with GNOME only. Before anything changes it checks, and exits with a reason if any fails:

1. the system is Linux;
2. `XDG_CURRENT_DESKTOP` names GNOME (it is a colon-separated list such as `ubuntu:GNOME`) and `gnome-extensions` is on `PATH`;
3. `flatpak info com.nvidia.geforcenow` succeeds;
4. GeForce NOW is not already running (`flatpak ps --columns=application`).

Then game mode goes on, Dash to Panel goes off (`gnome-extensions disable dash-to-panel@jderose9.github.com`), and `flatpak run com.nvidia.geforcenow` runs. When it returns, lum also waits until `flatpak ps` lists no GeForce NOW instance, since `flatpak run` can return while the app lives on. Then Dash to Panel comes back first, because it needs no root and is what you see, and game mode goes off.

- Dash to Panel is touched only if it was on. Disabling an extension is saved in GNOME's settings and survives a logout or reboot, so lum records it in `net-game.json` before disabling it. `lum net normal`, or the cleanup at the start of the next `game`, switches it back on if lum was killed first.
- If game mode was already on, GeForce NOW runs with Dash to Panel off, Dash to Panel comes back afterwards, and game mode stays on.
- The extensions lum hides live in `GAME_EXTENSIONS` in `desktop.rs`; there is just Dash to Panel so far.

## Root

lum itself never runs as root. It runs each root command through `sudo`, after one `sudo -v` up front so the password prompt comes before anything changes. While a wrapped command runs, a thread refreshes sudo's cached credentials every minute (`sudo -n -v`), so switching back hours later does not stop at a prompt. sudo needs a terminal to ask for the password, so run `lum net` from one.

## Record And Recovery

`game` writes `net-game.json` in lum's state directory before changing anything: the interface name, its MAC, the start time, on macOS the pf token, and the GNOME extensions it switched off. `normal` switches those extensions back on, finds the interface again by MAC, so a renamed or replugged adapter is still found, undoes the rest and deletes the record.

- Only a wired interface can be the recorded one. A bridge, bond or VLAN on top of it has the same MAC and can sort first, and undoing game mode on that would leave the real interface redirecting to a deleted `lumifb0`. Where two wired interfaces share a MAC, as the two halves of a bond do, the recorded name decides.
- Whether game mode is still on comes from the system, not from the record: on Linux, `lumifb0` exists; on macOS, the recorded interface's media is set by hand. A reboot resets both, so `status` reports a leftover record as off.
- The next `game` undoes a leftover record in full first, as `normal` does. A reboot is not the only way to get one: a lum killed while switching on, after the speed change and before shaping, leaves the link forced with no `lumifb0` to show for it.
- If switching on fails part-way, lum switches back at once. If switching back fails, the record stays so `lum net normal` can retry. An extension that cannot be switched back on stays in the record, and the error gives the `gnome-extensions enable` command for it.

## Wrapped Commands

- Ctrl+C, SIGTERM and SIGHUP do not kill lum while it wraps a command. They set a flag, lum passes SIGTERM to the command, waits for it to exit and switches back. Ctrl+C from the terminal already reaches both. A command still running 10 s after the SIGTERM is killed, so one that ignores it cannot keep game mode on.
- lum exits with an error when the command exits unsuccessfully, unless lum was interrupted.
- If game mode was already on, the command just runs, and game mode stays on afterwards.
- Closing the terminal sends SIGHUP and leaves lum nowhere to print. `println!` panics then, which would end lum before it switches back, so everything that switches prints through `say!` and `say_err!`, which drop the line instead.
- After the terminal is gone, Dash to Panel still comes back, because that needs no root. The network usually does not: sudo keeps its cached credentials per terminal by default (`timestamp_type=tty`), so it has to ask for the password again and has no terminal to ask on. Game mode then stays on until `lum net normal`.

## Test Measurements

- Latency is the time of a TCP handshake with `1.1.1.1:443`, every 250 ms. A handshake needs no root, unlike macOS `ping` below one-second intervals, and the reply queues behind the download like any other packet. A probe without an answer within a second counts as lost.
- Load comes from three public 10 GB test files on separate hosts (two Hetzner, one OVH) for `--seconds`. Latency is probed under load after a 2 s ramp-up.
- The end of `--seconds` also bounds each download request, so a server that never answers or stalls cannot hold up the result. The download rate is the bytes received over `--seconds`.
- Bufferbloat grades the latency a full download adds: under 30 ms low, under 100 ms moderate, else high.
- Receive errors are the interface's receive error counter (via `sysinfo`), mostly packets damaged on the wire. Some drivers count each one twice. Under 0.1% is harmless for game streaming.

## Tests

Parsers, interface choice and command lists are unit tested with fixtures, macOS ones included, so they run on Linux. `tests/net_cli.rs` covers the CLI surface without root or network.

`tests/net_game_cli.rs` runs `lum net game`, bare, with a command and with `gfn`, and `lum net normal` end to end against fake `sudo`, `flatpak` and `gnome-extensions` scripts on `PATH` and a fake `/sys/class/net` (`$LUM_NET_SYSFS`, a test hook). The fakes log every command instead of running it, and the tests compare the log with the exact sequence lum must run. The fake sudo also moves the fake link speed as ethtool would.

- `$LUM_NET_LINK_TIMEOUT_MS` is a second test hook: it shortens the 15 s wait for the link, for the test of a link that never reaches 100 Mb/s.
- Two tests take the terminal away from a lum that is wrapping a command: they close its output pipes, so that printing fails as it does on a closed terminal, and send it SIGHUP. In one of them the fake sudo then refuses, as the real one does without a terminal.

On real hardware, switching needs root and a real link, so it is tested by hand:

```sh
lum net status
lum net test
lum net game
lum net test        # expect 100 Mb/s and low bufferbloat
lum net normal
```

The macOS path has not been run on real hardware yet.
