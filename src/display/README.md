# Display Subcommand

`lum display` turns HiDPI on and off for external monitors: on for sharp text, off for games. It does the job of Crisp or Acuity without an app. macOS only; elsewhere it refuses to run. Why it works this way is in ADR-0029.

## CLI Shape

```sh
lum display        # show whether HiDPI is on
lum display on     # HiDPI at about the monitor's own size
lum display off    # the monitor's plain resolution
```

Every external monitor is switched; with several, each line names its monitor. Built-in screens are HiDPI already and are left alone.

## On and Off

- The panel's own resolution is what the modes IOKit flags native (`kDisplayModeNativeFlag`) draw, else the biggest 1x mode.
- `on` picks the biggest HiDPI size no wider than the panel and wider than half of it, at the panel's aspect ratio and the fastest refresh rate. macOS on Apple silicon doesn't offer a panel's own size at 2x, so a 2560x1440 panel gets 2544x1431 and a 1920x1080 one 1904x1071, and everything looks 0.6% and 0.8% bigger than without HiDPI. Where macOS does offer it, `on` takes it. Half the panel's size, which macOS offers by itself, makes everything twice as big, so it doesn't count.
- `off` picks the panel's own resolution without HiDPI, at the fastest refresh rate. It draws about a quarter of the pixels `on` does, which leaves the GPU to games. It lasts only until the user logs out or restarts; then macOS goes back to the last mode saved for good, which `on` sets.
- Each line shows the mode and, when it isn't the panel's own size, how much bigger or smaller everything looks than at the panel's own resolution without HiDPI: the panel's width over the mode's, to a tenth of a percent.
- Modes come from `CGDisplayCopyAllDisplayModes` with `kCGDisplayShowDuplicateLowResolutionModes`. Without it a HiDPI mode that shares its size with a 1x mode is hidden, which is why System Settings and Crisp don't list sizes like 2544x1431. Modes the desktop can't use are dropped.
- A switch goes through `CGConfigureDisplayWithDisplayMode`, and lum reads the mode back to confirm it. `on` completes it with `kCGConfigurePermanently`, which saves it across restarts, and `off` with `kCGConfigureForSession`, which saves nothing. A monitor already in the mode is not touched.

## HiDPI Sizes

macOS only offers HiDPI sizes for monitors it treats as high-density. For the others the sizes come from an override file, which macOS reads when the monitor connects. When `on` finds no HiDPI size, it installs the file and asks for a restart or a replug.

- The file is `/Library/Displays/Contents/Resources/Overrides/DisplayVendorID-<vendor>/DisplayProductID-<product>`, with the IDs from `CGDisplayVendorNumber` and `CGDisplayModelNumber` in lowercase hex.
- `scale-resolutions` holds 8-byte entries: the size drawn at 2x, width then height, as big-endian 32-bit numbers. `DisplayVendorID` and `DisplayProductID` repeat the IDs. Other keys already in the file are kept.
- The sizes run from the panel's width down to half of it in 16-point steps, at the panel's aspect ratio. A 2560x1440 panel gets 81, the list Crisp writes. macOS leaves out the ones it can't drive.
- A file that already has `scale-resolutions`, Crisp's included, is left alone; lum only says macOS hasn't loaded it yet.
- The file belongs to root. lum stages it in a temporary file and puts it in place with `sudo install` (root:wheel, mode 644).
- Monitors Apple ships an override for under `/System/Library/Displays` are left alone.
- `off` only switches the mode and keeps the file, so `on` needs no password or restart afterwards.

## Tests

Picking the modes, the size difference, the size list and the file entries are unit tested on every platform in `modes.rs`, with the modes a 1440p and a 1080p monitor offer. `tests/display_cli.rs` covers the CLI surface without touching a display.

`lum display`, `off` and `on` have been run by hand on an M1 Mac with a 2560x1440 monitor that had Crisp's file loaded. Installing the file needs a password and has not been run yet: on a monitor without the file, run `lum display on`, restart, then run it again. A restart after `off` has not been tried yet either.
