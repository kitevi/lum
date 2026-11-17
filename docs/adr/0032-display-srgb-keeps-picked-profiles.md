# Display sRGB Keeps Picked Profiles

## Status

Accepted. Moves the sRGB step of the Mac setup script, `~/.bio/install/pre`, into `lum display srgb`. The monitors are ADR-0030's.

## Context

`pre` gave the sRGB profile to every display whose current mode wasn't HiDPI, through a Swift snippet. Monitors that aren't Retina, such as the AOC 24G2W1G4, want sRGB rather than the profile macOS makes from their EDID. But `pre`'s test follows the current mode, which `lum display on` and `off` change: the MP275Q E14 was skipped while HiDPI was on and written after `off`. Each run also replaced whatever profile had been picked for a monitor. An earlier draft of this command spared the main monitor instead, which skips the case it is for: a lone monitor is always the main one.

## Decision

- **Command.** `lum display srgb`, run by hand. Nothing runs it by itself: not `pre`, not a monitor connecting.
- **Monitors.** The ones `lum display` manages (ADR-0030): external and not Retina. Retina screens are never written, and with only Retina screens connected it says it is disabled and fails.
- **Picked profiles.** A monitor gets sRGB only while it has no custom profile, using the one macOS made for it. Any custom profile ColorSync lists, the one macOS made included, was picked in System Settings or by an app and is kept; the line names its file. There is no override: System Settings > Displays > Color profile does the one-off.
- **Question.** lum asks before it gives each monitor sRGB, no by default, with `inquire`, blessed.rs's prompt crate. Without a terminal to ask in, it changes nothing. A monitor still on the profile macOS made may be on it by choice, and ColorSync doesn't keep that choice (below), so the question is what protects it.
- **Write.** `ColorSyncDeviceSetCustomProfiles` maps the default profile ID, as read from the factory profiles, to `/System/Library/ColorSync/Profiles/sRGB Profile.icc`, at ColorSync's default scope: the current user on this Mac. lum reads it back to confirm, as `on` and `off` read a mode back.
- **Bindings.** `objc2-color-sync`, from the objc2 family lum already uses. `CGDisplayCreateUUIDFromDisplayID` is declared in ColorSync.framework's `ColorSyncDevice.h`, not in CoreGraphics. The crate binds it but panics when it returns NULL, and release builds abort on a panic, so lum declares it itself: a monitor unplugged mid-run is only that monitor's error.
- **No reset.** System Settings puts a monitor's own profile back.

## Consequences

- Setup no longer needs Swift, and whether a monitor gets sRGB no longer depends on whether HiDPI is on.
- A monitor with a picked profile keeps it. To give it sRGB anyway, pick sRGB in System Settings.
- Picking the profile macOS made doesn't reliably keep a monitor off sRGB: set through ColorSync's API, a custom entry pointing at it reads back as none, and one listed for the MP275Q E14 on 2026-11-17 was gone later that day. Answering no does, and so does picking any other profile.
- ColorSync asserts unless the process has connected to the window server, which listing the monitors through CoreGraphics does first.
- `objc2-color-sync`, `inquire` with its crossterm backend, and `objc2-core-foundation`'s `CFURL` and `CFUUID` features are new macOS dependencies.
