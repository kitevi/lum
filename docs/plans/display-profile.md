# Plan: move the pre script's sRGB step into `lum display srgb`

Status: executed 2026-11-17; see Execution. The sections after it are the plan as approved.

## Execution (2026-11-17)

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace` pass. `cargo fmt` also reflowed one chain in `tests/env_cli.rs`, unformatted on HEAD. `cargo tree` for Linux and Windows doesn't pull `objc2-color-sync`.
- By the time it ran, the MP275Q E14 had no custom profile: the entry the read-only probe below found was gone. So Verification's row "2, 3" became row "1, 3": `sRGB profile set.`, then `sRGB profile already set.`, with `CustomProfiles` `1` reading back as `sRGB Profile.icc`. A Swift snippet reset it with `kCFNull` afterwards; `CustomProfiles` is empty again, as before the run.
- Pointing the custom profile at the factory file with `ColorSyncDeviceSetCustomProfiles` returned true but read back as no custom profile, and `srgb` then gave the monitor sRGB. Picking the profile macOS made doesn't reliably protect a monitor; ADR-0032 and the README say so. Criterion 2 rests on the unit tests; reading a custom profile back is exercised by `already set`.
- `lum display` and `lum display on` print `HiDPI on: looks like 2544x1431 at 144 Hz; everything is 0.6% bigger than at 2560x1440.`. `off` and criterion 4 were not run by hand: `off` switches the monitor, and no Retina external was at hand.
- `lum display --help` lists `off`, `on` and `srgb`; `lum display maybe` fails with `unrecognized subcommand 'maybe'`, exit 2.
- `pre`: lines 148-265 deleted; the grep finds nothing and `bash -n` passes. The original is at `$TMPDIR/pre.orig`.
- Afterwards, at the user's request, `srgb` asks before each monitor it would change (`inquire`, blessed.rs's prompt crate; ADR-0032). No, Enter alone and no terminal were run by hand and wrote nothing.

Deviations:

- `HidpiState` lives in `macos.rs`, the only module that reads it, not `mod.rs`.
- `objc2-core-foundation` gains `CFUUID` as well as `CFURL`, for the local declaration.
- No number or string fallbacks: the probe showed `CFString` IDs and `CFURL` profiles, and anything else is that monitor's error, with nothing written.
- The sRGB path is `modes::SRGB`, shared by the decision and `srgb.rs`, so `srgb_step` takes the other two paths.
- ColorSync asserts (`CGS_REQUIRE_INIT`) unless the process has connected to the window server. `srgb.rs` lists the monitors first, which connects, and says why.

## Changed from the first draft

- The subcommand is `srgb`, not `profile`.
- The monitors are the ones `lum display` already manages (ADR-0030), minus any whose color profile has been picked. The main-monitor rule, `--all` and its prompt are gone: a lone monitor is always the main one, so that rule skipped the very case this is for.
- The Retina skip stays, with ADR-0030's test rather than `pre`'s.
- The CLI uses an optional subcommand, which usage-rs 7.3 supports: no `status` subcommand, no default attribute.
- Cargo: no `CFNull` (not a feature of `objc2-core-foundation`), no `CGDisplayConfiguration` change (already enabled).
- `CGDisplayCreateUUIDFromDisplayID` is declared locally: the crate's wrapper panics when it returns NULL.
- The ADR is 0032 (0031 is the radio's). `tests/display_cli.rs` and the display tests in `cli.rs` are in the file list. `pre` is copied aside before editing, since `~/.bio` isn't under git.
- Corrected: the `24G2W1G4` `.icc` is a factory profile, not something `pre` made; `pre` is the setup script, not run at login; there is no Dell.

## Outcome

`~/.bio/install/pre`, the Mac setup script, writes the sRGB color profile to every display whose current mode isn't HiDPI. Whether the MP275Q E14 gets it depends on the moment: it runs HiDPI today (2544x1431, from `lum display on`), so a run now leaves it alone, but a run after `lum display off` replaces the profile picked for it. Monitors such as the AOC `24G2W1G4` want sRGB.

Intended behavior: the step leaves `pre`, and `lum display srgb` does it when run, on the monitors `lum display` manages, without replacing a profile someone picked. Acceptance criteria:

1. `lum display srgb` makes sRGB the custom default profile of every monitor `lum display` manages (external and not Retina, by ADR-0030) that has no custom profile yet, and prints a line per monitor saying what it did.
2. A monitor whose custom profile is anything but sRGB keeps it, and its line names the file. The MP275Q E14 is this case today.
3. A second run writes nothing: each line says sRGB is set already, or names the profile kept.
4. With only Retina screens connected it says it is disabled and fails, as `lum display` does.
5. `lum display`, `on` and `off` work as today.
6. No `ColorSync`, `sRGB` or `swift` is left in `pre`, and `bash -n` passes on it.

## Approach

One new subcommand on the loop `on` and `off` already use: `Monitor::managed()`, a line per monitor labelled by `Monitor::labels`, and one monitor failing without stopping the others. No Swift at run time.

### CLI (`src/cli.rs`)

```rust
/// HiDPI on and off, and sRGB color, for monitors that aren't Retina. (macOS)
///
/// Without a command, shows whether HiDPI is on.
Display {
    #[usage(subcommand)]
    command: Option<DisplayCommand>,
},

#[derive(Debug, usage_rs::Subcommands)]
pub enum DisplayCommand {
    /// Sharp HiDPI at about the monitor's own size.
    On,
    /// The monitor's plain resolution until you restart or log out: least GPU work, for games.
    Off,
    /// The sRGB color profile, for monitors still on the one macOS made for them.
    Srgb,
}
```

- An `Option` subcommand field may be left out; usage-derive 7.3 documents exactly this shape (`usage-derive-7.3.0/src/lib.rs:57`). `None` shows the status, as `lum display` does today.
- Subcommands rather than a third `[STATE]` value: `srgb` isn't a HiDPI state, and each subcommand gets its own help line.
- `HidpiState` loses its `ValueEnum` derive and moves to `src/display/mod.rs`, whose `run` maps `On` and `Off` onto it, so `macos::run` keeps its body.

### Which monitors get sRGB (`src/display/modes.rs`, every platform)

A pure function decides from two paths, the monitor's custom default profile and its factory default profile:

| Custom profile | Factory profile | Result |
|---|---|---|
| none | not sRGB | set sRGB |
| none | sRGB | already sRGB |
| sRGB | any | already sRGB |
| anything else | any | keep it, and name its file |

A custom profile counts as picked whatever it points at: the MP275Q E14's points at its own factory file, and that is what has to stay. The function sits in `modes.rs` beside `retina()` and `labels()`, apart from the macOS calls, so CI's `cargo test` on Linux covers it; the module's doc line gains "and which monitors get sRGB".

### macOS side (`src/display/srgb.rs`, new, macOS only)

`pub(super) fn run() -> Result<()>`, shaped like `macos::run`:

- `Monitor::managed()`. With none, fail: `lum display srgb is disabled: every connected screen is Retina, and lum leaves their color alone`.
- The sRGB file is `/System/Library/ColorSync/Profiles/sRGB Profile.icc`; if it's missing, fail naming the path before touching any monitor.
- For each monitor:
  1. Its ColorSync ID, from `CGDisplayCreateUUIDFromDisplayID` (declared locally, below).
  2. `ColorSyncDeviceCopyDeviceInfo(kColorSyncDisplayDeviceClass, id)`. `FactoryProfiles[DeviceDefaultProfileID]` is the default profile ID, `FactoryProfiles[ID][DeviceProfileURL]` the factory file, `CustomProfiles[ID]` the custom one. IDs are `CFString`s such as `"1"`, as `ColorSyncDevice.h` tells devices to register them; also try them as `CFNumber`, and read URLs given as `CFURL` or as a string, as the Swift block's `dictionaryValue` and `normalizedProfilePath` do. No device info, or no default profile ID, is that monitor's error.
  3. The decision above. To set: `ColorSyncDeviceSetCustomProfiles` with `{ID: sRGB file URL}`, keyed by the ID object just read, so the write lands where the next read looks (the header accepts a registered ID or `kColorSyncDeviceDefaultProfileID`). Then read the device info again and check the custom profile is sRGB, as `switch` reads a mode back.
- Scope stays at ColorSync's default, current user and current host, as with the Swift block.
- Lines. With several monitors each starts with the monitor's name; with one there is no name, so each reads alone:
  - `sRGB profile set.`
  - `sRGB profile already set.`
  - `Keeps the profile picked for it, MP275Q E14-BCE23874-1F59-4CB7-992A-A4E494B86262.icc.` (the file name)
- One monitor failing prints its error after its name, and the command fails at the end, as in `macos::run`; with one monitor its error is the command's.

`Monitor`, its `id`, `Monitor::managed` and `Monitor::labels` in `macos.rs` become `pub(super)`.

### ColorSync bindings

- `CGDisplayCreateUUIDFromDisplayID` is declared in ColorSync.framework's `ColorSyncDevice.h:270`, not in CoreGraphics, which is why `objc2-core-graphics` has no binding for it. `objc2-color-sync 0.3.2`, from the same objc2 family as lum's other `objc2-*` crates, binds it with `ColorSyncDeviceCopyDeviceInfo`, `ColorSyncDeviceSetCustomProfiles` and the `kColorSync*` keys under its `ColorSyncDevice` feature, which pulls only `CFDictionary` and `CFUUID` from `objc2-core-foundation`.
- The crate's wrapper for `CGDisplayCreateUUIDFromDisplayID` is marked non-null and panics when macOS returns NULL, and release builds have `panic = "abort"`. `srgb.rs` declares the function itself, with the crate's own inner signature (`fn(u32) -> Option<NonNull<CFUUID>>`), so a monitor unplugged mid-run is that monitor's error instead of an abort.

`Cargo.toml`, under `[target.'cfg(target_os = "macos")'.dependencies]`:

```toml
# `lum display srgb` reads and sets monitors' color profiles (see ADR-0032).
objc2-color-sync = { version = "0.3", default-features = false, features = ["std", "ColorSyncDevice"] }
```

plus `"CFURL"` in `objc2-core-foundation`'s features, for the sRGB URL (`CFURL::from_file_path`) and reading profile URLs back as paths (`to_file_path`). Nothing else: `CGDisplayConfiguration` is already enabled, and without a reset nothing needs `kCFNull`.

## The screen's current state (measured 2026-11-17, read-only)

From `/tmp/probe_colorsync.swift` and a device-info dump:

- One display online: `MP275Q E14`, external, 2544x1431 points at 5088x2862 pixels (HiDPI on). `pre`'s test calls it Retina right now.
- `FactoryProfiles`: `DeviceDefaultProfileID = 1`; `1` is `/Library/ColorSync/Profiles/Displays/MP275Q E14-BCE23874-1F59-4CB7-992A-A4E494B86262.icc`.
- `CustomProfiles`: `1` is the same file. Most likely picked in System Settings after `pre` had set sRGB; either way it is what criterion 2 protects.
- `/Library/ColorSync/Profiles/Displays/` also holds `24G2W1G4-566FE226-….icc`, from 2026-09-14: the factory profile macOS made from that monitor's EDID when it was connected. `pre` writes no `.icc` files; it points a custom entry at the system sRGB file.

## Decisions

| Question | Choice | Who / when | Evidence | Would flip it |
|---|---|---|---|---|
| Move the step into lum? | Yes, as a `lum display` subcommand, run by hand | User, first session | `lum display` already finds, labels and switches these monitors | A reason it belongs to machine setup |
| Name | `srgb` | Review, 2026-11-17; user to confirm | Reads with `on` and `off` as something you set; `profile` sounds like it shows one | The user prefers `profile` |
| Which monitors | Those `lum display` manages, minus any with a custom profile other than sRGB | User: leave the monitor in use alone. Review, 2026-11-17: say it as "keep a picked profile" | MP275Q E14's custom entry; a lone monitor is always main, and which one is main follows the arrangement | macOS adding custom entries by itself (below) |
| Retina | Skipped, by ADR-0030's test | Review, 2026-11-17 | `pre`'s current-mode test flips with `lum display on`/`off`; Apple's wide-color external displays would get sRGB | Wanting sRGB on a 4K monitor macOS runs at HiDPI |
| Override (`--all`, prompt) | None | Review, 2026-11-17 | System Settings > Displays > Color profile does the one-off; no lum command asks for confirmation | Needing sRGB forced onto several picked monitors at once |
| Reset | Out of scope | First draft, kept | System Settings does it; `ColorSyncDeviceSetCustomProfiles` takes `kCFNull` if lum ever needs it | The user wants it now |
| Bindings | `objc2-color-sync`, plus a local declaration of `CGDisplayCreateUUIDFromDisplayID` | Review, 2026-11-17 | The crate covers every call; its UUID wrapper panics on NULL | Accepting an abort on a monitor unplugged mid-run |
| Write key | The default profile ID as read from `FactoryProfiles` | Review, 2026-11-17 | `ColorSyncDevice.h` accepts registered IDs; the write lands where the read looks | — |

## Unverified assumptions

- macOS leaves `CustomProfiles` empty for a monitor until someone picks a profile. If it doesn't, `srgb` writes nothing and says it keeps the profile: safe, and visible. The first run on a monitor this Mac has never seen checks it: `Keeps the profile picked for it` there means macOS made the entry.
- ColorSync keeps a custom profile under the ID it was given. The read-back after each write checks it.

## Boundaries

- `lum display`, `on`, `off`, the tray and mode picking don't change.
- Built-in and Retina screens are never written.
- No reset, and nothing runs `srgb` by itself: not `pre`, not a monitor connecting.
- `DisplayResolutionEnabled` (`pre:144-147`) stays in `pre`: it isn't a profile, and it needs sudo where `pre` already asks.

## Files

- `src/cli.rs`: `Display` holds `Option<DisplayCommand>`; new `DisplayCommand`; `HidpiState` moves out. `parses_display_on` and `parses_display_without_a_state` follow the new field, and `parses_display_srgb` joins them; the `Commands::Display { .. }` arms need nothing.
- `src/main.rs`: `Commands::Display { command } => display::run(command)`.
- `src/display/mod.rs`: `run` takes `Option<DisplayCommand>`, sends `Srgb` to `srgb::run` and the rest to `macos::run`; `HidpiState` lives here; the module doc mentions sRGB.
- `src/display/srgb.rs` (new, macOS only): the ColorSync work.
- `src/display/macos.rs`: `pub(super)` as above; `HidpiState` imported from `super`.
- `src/display/modes.rs`: the decision, its unit tests, the doc line.
- `tests/display_cli.rs`: help lists `on`, `off` and `srgb`; an unknown command fails (assert the wording usage-rs gives); `srgb` refuses to run off macOS.
- `Cargo.toml`: the dependency and the `CFURL` feature above.
- `src/display/README.md`: intro, CLI shape, an sRGB section, Tests. `README.md`, Display section: the new line.
- `docs/adr/0032-display-srgb-keeps-picked-profiles.md`: why only managed monitors, why a picked profile is kept rather than the main monitor skipped, where the UUID function lives, and why the step left `pre`.
- `~/.bio/install/pre`: copy it aside first, then delete lines 148-265, from `# Set sRGB color profile only for non-Retina displays` through the `fi` that closes the `swift` check.
- `AGENTS.md`: no change.

## Verification

| Criterion | How | Expected |
|---|---|---|
| Build | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace` | Pass; CI's Linux and Windows release builds don't pull the new crate |
| 2, 3 | `cargo run -- display srgb` on today's desk, twice; the probe, or System Settings > Displays > Color profile, before and after | `Keeps the profile picked for it, MP275Q E14-….icc.` both times; `CustomProfiles` unchanged |
| 1, 3 | A monitor with no custom profile (one this Mac hasn't seen, or one whose entry a throwaway Swift snippet resets with `kCFNull`), run twice | First `sRGB profile set.` and System Settings shows sRGB; then `sRGB profile already set.` |
| 4 | By hand, when a Retina external is at hand; `retina()` is unit tested | The disabled message, exit 1 |
| 5 | `cargo run -- display`, `display on`, `display off` | As today |
| 6 | `grep -n -i -e colorsync -e srgb -e swift ~/.bio/install/pre`; `bash -n ~/.bio/install/pre` | No output; passes |
| CLI | `cargo run -- display --help` | Lists `on`, `off`, `srgb` |

On a one-monitor desk, criteria 1 and 4 wait for another monitor; until then the decision's unit tests pin the rule down.

## Recovery

- A monitor that got sRGB by mistake: System Settings > Displays > Color profile, pick its own profile. From then on lum keeps it.
- `pre`: put the copy back.
- Code: one commit, reverted.
