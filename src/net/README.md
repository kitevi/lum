# Net Subcommand

`lum net test` measures the wired connection: latency idle and while downloading (bufferbloat), and the receive errors the interface counts meanwhile. It needs no root and changes nothing. Linux and macOS only; on Windows it refuses to run.

## CLI Shape

```sh
lum net test [--iface <name>] [--seconds <5-120>]
```

- `--seconds` is how long the link is loaded, 20 by default.
- `--iface` picks the wired interface instead of detecting it.

## Choosing The Interface

Interface names are read fresh on every run.

- Linux reads `/sys/class/net`: an interface is wired Ethernet when it has a `device` (physical), no `wireless` or `phy80211` (not Wi-Fi), and `type` 1 (Ethernet framing). `carrier` and `speed` give the link.
- macOS reads `ifconfig -a`: an interface is wired when its media names an Ethernet type such as `1000baseT`; Wi-Fi only shows `autoselect`. `status: active` is the link.
- With one wired link, that is the one. With several, the one carrying the default route (`/proc/net/route`, `route -n get default`), else lum stops and asks for `--iface`. Without a wired link lum stops: the test is for wired connections only.

## Measurements

- Latency is the time of a TCP handshake with `1.1.1.1:443`, every 250 ms. A handshake needs no root, unlike macOS `ping` below one-second intervals, and the reply queues behind the download like any other packet. A probe without an answer within a second counts as lost.
- Load comes from three public 10 GB test files on separate hosts (two Hetzner, one OVH) for `--seconds`. Latency is probed under load after a 2 s ramp-up.
- The end of `--seconds` also bounds each download request, so a server that never answers or stalls cannot hold up the result. The download rate is the bytes received over `--seconds`.
- Bufferbloat grades the latency a full download adds: under 30 ms low, under 100 ms moderate, else high.
- Receive errors are the interface's receive error counter (via `sysinfo`), mostly packets damaged on the wire. Some drivers count each one twice. Under 0.1% is harmless for game streaming.
- On macOS it also points to `networkQuality -s`, Apple's own measurement.

## Tests

Parsers, interface choice and grading are unit tested with fixtures, macOS ones included, so they run on Linux. `tests/net_cli.rs` covers the CLI surface without root or network.

The measurement itself needs a real link and the public test servers, so it is run by hand: `lum net test`. The macOS path has not been run on real hardware yet.
