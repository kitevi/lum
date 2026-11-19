# Vol Subcommand

`lum vol` reads and sets the volume of the default output device, through the `volumecontrol` crate.

## CLI Shape

```sh
lum vol status
lum vol set <level>
lum vol reset
```

- `status` prints `Volume: <level>`.
- `set` takes a whole number from 0 to 100. Anything else is a usage error and exits 2.
- `reset` sets the default level: 17 on macOS, 25 on Linux and Windows.

`set` and `reset` print the new level and, when the device reports it, the one before: `Volume set to 40 (was 25)`.
