# App Settings Are Edited Line By Line

## Status

Accepted. Changes how ADR-0026 writes owned settings.

## Context

ADR-0026 had lum parse an app's INI file with `rust-ini` and write the whole file back. snes9x breaks that. Its Windows build keeps keys such as `Dir:SRAM = .\Saves` in `snes9x.conf`, and `rust-ini`, like many INI parsers, splits a key at `:` as well as `=`: writing the file back would turn that line into `Dir=SRAM = .\Saves` and lose every `Dir:` setting. snes9x also has value rules of its own: `#` starts a trailing comment, `##` is a literal `#`, quotes keep outer spaces, and a trailing backslash continues the line. No well-maintained crate edits an INI file without rewriting all of it.

Owned settings also differ per platform now. snes9x's keys, file and format on Windows have nothing in common with mGBA's, and snes9x needs none on Linux, where it already saves beside each ROM, or on macOS, where it keeps its preferences in user defaults.

## Decision

- lum edits a config file line by line. It rewrites only the lines of the keys it owns, adds a missing key after the last entry of its section or as a new section at the end, and leaves every other byte as the app wrote it: comments, order, spacing and line endings.
- Each app's settings name a **dialect**, the rules its own parser reads a line by: inih for mGBA (split at the first `=` or `:`), and snes9x's ConfigFile (split at the first `=`, `#` comments, `##`, quotes). lum reads an owned value the way the app would and writes it the way the app does.
- Settings belong to a build, not to an app, so an app can own different keys in a different file on each OS, or none.
- A value is fixed text or the ROMs folder, `~/ROMs`, which `lum backup create roms` backs up. lum creates the folder when it sets such a value.
- Where a key appears more than once, lum sets every copy; both apps read the last.
- A config file that is not UTF-8 is refused rather than rewritten lossily.

## Consequences

- `rust-ini` and the seven crates it brought in leave the tree. lum maintains about a hundred lines of line editing, with tests for both dialects.
- A key with a trailing comment loses the comment when lum rewrites it; snes9x writes it back on its next save.
- An app whose parser follows neither dialect needs a third.
