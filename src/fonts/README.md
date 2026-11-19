# Fonts Subcommand

`lum fonts` installs fonts from a hardcoded catalog into the current user's font directory.

## CLI Shape

```sh
lum fonts list
lum fonts install <font> [--force]
lum fonts status <font>
lum fonts uninstall <font>
```

- `list` shows each catalog font as `installed` or `not installed`.
- `install` refuses a font that is already installed unless `--force` is passed.
- `status` prints the font's directory and whether it exists.
- `uninstall` removes the font's directory, and fails when it is not installed.
- A name that is not in the catalog fails with the names that are.

## Storage

Each font gets a directory of its own, named in the catalog, under the user font directory:

- macOS: `~/Library/Fonts`
- Linux: `$XDG_DATA_HOME/fonts`, or `~/.local/share/fonts`
- Windows: `%LOCALAPPDATA%\Microsoft\Windows\Fonts`

That directory existing is what makes a font installed; there is no state file.

## Install

`install` downloads the font's zip and extracts its `.ttf` files flat, skipping the catalog's `excluded_files`; an archive without any fails. The download and the files are staged in a temporary directory beside the destination and only replace an existing copy once both succeeded, so a failed reinstall keeps the old one. Install and uninstall then run `fc-cache` on Linux and macOS, best effort.

`LUM_FONT_TEST_ARTIFACT_<NAME>` (the font name upper-cased, `-` as `_`) replaces the download URL, for tests.

## Catalog

`CATALOG` in `catalog.rs` lists the fonts; today that is `dmca-sans-serif`. Its archive also carries Arial and Tahoma alias files, which `excluded_files` leaves out so that fontconfig does not prefer them over the real Windows fonts.
