# Plan: move the pre script's sRGB color profile step into `lum display profile`

Status: draft for approval. No code written yet.

## Outcome

`~/.bio/install/pre` still writes the sRGB color profile to every non-Retina monitor, including the one in daily use. The panel in use (`MP275Q E14`, mode 2544x1431 HiDPI) shows with its factory profile, which suits it; the other panels (`24G2W1G4`, and whatever an Apple Silicon Mac is plugged into) want sRGB.

Intended behavior: the profile step leaves `pre`, and `lum display profile` does it on demand, never touching the monitor in use. Acceptance criteria:

1. `lum display profile` writes the sRGB profile as the custom default for every connected, non-builtin monitor that is **not** the main monitor, and prints one line per monitor saying what it did.
2. It leaves the main monitor's profile as it is, and says so in its line.
3. Running it twice changes nothing the second time and needs no second confirmation.
4. `--all` includes the main monitor; the confirmation prompt names it before anything is written.
5. No occurrence of `ColorSync`, `sRGB` or `swift` remains in `pre`.

## Approach

A thin vertical slice through the existing `display` subsystem: one new subcommand, one new macOS module, no new runtime dependency, no Swift at run time.

### CLI (`src/cli.rs`)

```rust
/// Sharp HiDPI text on monitors that aren't Retina, or off for games. (macOS)
Display {
    /// Omit to show whether HiDPI is on.
    #[usage(subcommand, default = "status")]
    command: Option<DisplayCommand>,
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum DisplayCommand {
    /// Show whether HiDPI is on.
    Status,
    /// HiDPI at about the monitor's own size.
    On,
    /// The monitor's plain resolution.
    Off,
    /// Set the sRGB color profile on monitors other than the main one.
    Profile {
        /// Also set it on the main monitor, the one in use.
        #[usage(long)]
        all: bool,
    },
}
```

- `default = "status"` keeps `lum display` reading as today. Whether `usage-rs 7.3` accepts a default subcommand on an `Option` field is **unverified**; if it needs a required field, make the field required and let the default supply it, or fall back to the plain `Option<HidpiState>` + manual `"profile"` match. Verify by running `lum display`, `lum display --help` and the `tests/display_cli.rs` cases.
- `Subcommands` rather than `ValueEnum`, since `ValueEnum` cannot express `--all` on one variant.
- `HidpiState` stays as the value type `display::run` takes; `DisplayCommand::On` / `Off` map onto it. This keeps `macos::run` untouched.

### macOS side: `src/display/profile.rs` (new)

- `pub(super) fn run(all: bool) -> Result<()>`:
  - Enumerate online displays (`external_ids()` in `macos.rs` already drops built-ins), keep each monitor's label using `Monitor::labels`, and mark the ones `CGDisplayIsMain` returns true for.
  - Refuse the main monitor unless `all`: print `<label>in use: left as it is` and continue with the rest.
  - With `all` and a main monitor, print a confirmation line naming it and read one line from stdin before writing; anything but `y`/`yes` skips it. Skipping is not an error.
  - For each remaining monitor, resolve the ColorSync UUID and read `ColorSyncDeviceCopyDeviceInfo`; skip with a line when the factory default profile ID or the custom entry is missing, mirroring the Swift block's skip messages.
  - Write `kColorSyncDeviceDefaultProfileID` → sRGB `CFURL` through `ColorSyncDeviceSetCustomProfiles`. Already sRGB in either the custom or factory path → skip with a line.
  - Return an error only when a write fails, in the usual lum style (`ensure!`), after printing the other monitors' lines.
- Paths: sRGB is `/System/Library/ColorSync/Profiles/sRGB Profile.icc`; its absence on some future macOS is an error with the path in the message.

### Why `color-sync` rather than "`CGDisplayCreateUUIDFromDisplayID` is missing from the bindings"

`CGDisplayCreateUUIDFromDisplayID` lives in **ColorSync.framework's** `ColorSyncDevice.h` (`ColorSync.tbd` exports it) `ColorSyncDevice.h:270`, not in CoreGraphics, which is why `objc2-core-graphics 0.3.2` has no binding for it. `objc2-color-sync 0.3.2` (madsmtm's objc2 family, same as the `objc2-*` crates lum already uses) covers it: `kColorSyncDisplayDeviceClass`, `ColorSyncDeviceCopyDeviceInfo`, `ColorSyncDeviceSetCustomProfiles`, `CGDisplayGetDisplayIDFromUUID`, and the `kColorSync*` keys. `ColorSyncDevice` is the only feature needed, and it pulls only `objc2-core-foundation/CFDictionary` and `/CFUUID`.

New `[target.'cfg(target_os = "macos")'.dependencies]` entries:

- `objc2-color-sync = { version = "0.3", default-features = false, features = ["ColorSyncDevice"] }`
- add `CFUUID`, `CFURL`, `CFNull` to the existing `objc2-core-foundation` feature list (already `CFArray`, `CFDictionary`, `CFString`, `CFNumber`)
- add `CGDisplayConfiguration` to the existing `objc2-core-graphics` features for `CGDisplayIsMain` (`CGDirectDisplay` is already enabled)

Both crates are macOS-only target deps, so Linux and Windows builds are untouched (AGENTS.md dependency rules).

### The screen's current state (measured, `/tmp/probe_colorsync.swift`)

- One display, `MP275Q E14`, builtin=false, 2544x1431 points at 5088x2862 pixels (HiDPI), factory default profile ID `1`, `CustomProfiles[1]` already pointing at `/Library/ColorSync/Profiles/Displays/MP275Q E14-….icc`.
- `24G2W1G4` has an `.icc` beside it in `/Library/ColorSync/Profiles/Displays/` but is not connected now; the pre run that made it is the case this feature is for.
- That custom entry is why the panel shows as it does today, and exactly what `--all` would replace.

## Decisions

| Question | Choice | Who / when | Evidence | Would flip it |
|---|---|---|---|---|
| Ship the step as `lum display profile`? | Yes: a display concern, one macOS-only code path, runs on demand without a login-time script | Proposed by the user, this session | `lum display` already lists and switches every monitor; `pre` writes profiles with an inline Swift program on each run | A reason the profile belongs to machine setup rather than a display action |
| Subcommand name | `profile` | Proposed, this session | Subsystem vocabulary: "profile" is ColorSync's word for it, and "color" adds nothing | A name that reads better with `on`/`off` |
| Main monitor | Never touched unless `--all`, which asks first | User instruction, this session | Panel in use is fine with its factory profile | — |
| Retina filter | Dropped: no `retina()` check | Inferred | `pre` skips Retina monitors because sRGB is not needed there; HiDPI has nothing to do with the profile, and a 4K panel *can* need sRGB | Evidence a Retina panel must never get sRGB |
| Implementation | `objc2-color-sync`, drop the Swift snippet | Inferred | Generated bindings for every API the snippet calls; no toolchain or `pre`-time compile | A binding gap the plan missed, or a decision to keep `pre` Swift-only |
| Reset path | Out of scope | Inferred | `pre` never reset anything; `ColorSyncDeviceSetCustomProfiles` accepts `kCFNull` if a later release wants `lum display profile --reset` | The user wants a reset now |

Unverified assumptions: `usage-rs` default-subcommand behaviour on an `Option` field (checked by test, not by reading the crate in full); `CGDisplayIsMain` being the right "monitor in use" test on a mirrored setup — mirrored displays both report main, and the plan only ever *includes* fewer monitors that way, never more.

## Boundaries

- No changes to `lum display on/off` behavior, to the tray, or to `modes.rs`.
- `DisplayResolutionEnabled` (the other display line in `pre`, `pre:144-145`) stays where it is; it is not a profile and needs sudo at a point where `pre` already asks.
- No profile for built-in screens, ever.
- A reset-to-factory path is left to a later change.

## Files

- `src/cli.rs`: `Display` variant becomes a subcommand; new `DisplayCommand`; fix the `Commands::Display { .. }` arms in the existing unit tests.
- `src/main.rs`: route `DisplayCommand::Profile` to `display::run_profile(all)`, keep `On`/`Off`/`Status` on `display::run`.
- `src/display/mod.rs`: `run` takes the narrower state enum; add `run_profile` with the same `cfg` split as `run` and the shared `unsupported()` error.
- `src/display/profile.rs` (new, `#[cfg(target_os = "macos")]`): the ColorSync work, reusing `macos::external_ids` and `Monitor::labels` (both need `pub(super)`).
- `Cargo.toml`: one new macOS dep, two feature-list edits.
- `~/.bio/install/pre`: delete `# Set sRGB color profile…` through the closing `fi` of the Swift block, and renumber nothing else.
- `src/display/README.md`, `README.md` (Display section) get the new subcommand.
- `docs/adr/0031-display-profile-skips-the-main-monitor.md`: the skip rationale, the ColorSync-vs-CoreGraphics API location, and why the step left `pre`.
- `AGENTS.md` needs no change (policy-level only).

## Verification

| Criterion | How | Expected |
|---|---|---|
| 1 | `cargo run -- display profile` with the Dell plugged in | One line per external monitor; a line saying the main monitor is left as it is; Dell's custom profile becomes sRGB |
| 2 | `swift /tmp/probe_colorsync.swift` before and after | `MP275Q E14` custom entry unchanged (`…/MP275Q E14-….icc`) |
| 3 | Run command twice | Second run: every line reads "already sRGB", no write |
| 4 | `cargo run -- display profile --all`, answer `n` then `y` | Prompt names `MP275Q E14`; `n` skips it, `y` writes whatever the factory makes reachable |
| 5 | `grep -n -i 'colorsync\|srgb\|swift' ~/.bio/install/pre` | No output; `bash -n ~/.bio/install/pre` passes |
| CLI | `cargo run -- display` (no arg), `display on`, `display off`, `cargo test --test display_cli` | `display` alone still prints the status line; `--help` lists `status on off profile` |
| Build | `cargo build` and `cargo build --target x86_64-unknown-linux-gnu` if a target is installed | macOS compiles; the new deps never appear on other targets |

The prompt path (criterion 4) and a real write (criteria 1, 3) need a second monitor that is not the main one; on a one-monitor desk this is checked by hand later, and the plan's unit-testable surface is the label/main-monitor selection, which lives in `profile.rs` behind a small pure function taking `(id, is_main, custom_path, factory_path)`.

## Recovery

- The main monitor is never written without `--all`, so a mistake cannot wreck the display in use.
- A profile written by mistake is undone by setting that monitor's profile back to the factory default in System Settings > Displays > Color Profile, or by `ColorSyncDeviceSetCustomProfiles` with `kCFNull` for that ID.
- `pre` keeps its other display settings and loses only the profile step; `git diff` on `pre` shows nothing else.
