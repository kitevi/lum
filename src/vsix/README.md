# Vsix Subcommand

`lum vsix` manages a hardcoded catalog of VS Code extensions that are distributed only as `.vsix` files attached to GitHub releases (no Marketplace or Open VSX listing, so no editor auto-updates). It works on Linux, macOS, and Windows, with VS Code, Cursor, or VSCodium.

## CLI

```sh
lum vsix ls [--editor <editor>]
lum vsix install <extension> [--editor <editor>] [--force]
lum vsix status <extension> [--editor <editor>]
lum vsix update <extension> [--editor <editor>] [--force]
lum vsix sync [--editor <editor>] [--dry-run]
lum vsix version <extension> [--editor <editor>]
```

`<editor>` is `code`, `cursor`, or `codium`. Without `--editor`, lum uses the first of those found on `PATH`, then in default install locations (macOS: `/Applications` and `~/Applications` app bundles; Windows: `%LOCALAPPDATA%\Programs` and `%ProgramFiles%`). Each command targets one editor; run `sync --editor cursor` separately for a second editor.

`sync` is the all-extensions operation: it installs missing extensions, updates outdated ones, and skips extensions whose requirements are missing. `update` targets one named extension only.

## Catalog

- `lathe` → `ag-libs.lathe` (GitHub `ag-libs/lathe`, release asset `lathe-<version>.vsix`). Requires Java 17+; conflicts with `redhat.java`.

## Requirements

Requirements are checked first, before the editor CLI is resolved and before any network request, so a machine without them fails fast with nothing downloaded. `sync` reports such extensions as skipped instead of failing.

Java is found at `$LATHE_JAVA_HOME/bin/java`, then `$JAVA_HOME/bin/java`, then `java` on `PATH`. On macOS, the `/usr/bin/java` stub only counts when `/usr/libexec/java_home` finds a real JDK. Only presence is checked: the Lathe extension runs each project's JDK from `.lathe/java-home`, so the default `java` version is not what Lathe uses.

## State

lum keeps no state file. The editor owns its extensions, and `<editor> --list-extensions --show-versions` is the source of truth for what is installed, so removing an extension from the editor UI is reflected immediately. Outdated means the installed version differs from the latest release.

## Install Flow

1. Resolve the latest GitHub release and its `<prefix>-<version>.vsix` asset.
2. Download it to a temporary directory and verify its SHA-256 against the asset digest GitHub publishes (`sha256:<hex>`). Assets without a digest install unverified.
3. Run `<editor> --install-extension <file> --force`.
4. Re-list extensions and fail unless the editor now reports the released version.

After installing, lum prints a note for each installed conflicting extension (for lathe: disable Red Hat's `redhat.java` in Lathe workspaces). lum only prints; it never disables extensions.

## Implementation

See [ADR 0021](../../docs/adr/0021-vsix-installs-through-the-editor-cli.md) for why lum drives the editor CLI instead of unpacking `.vsix` files itself.

Integration tests live in `tests/vsix_cli.rs` and run network-free via `LUM_VSIX_TEST_ARTIFACT_<NAME>` (a local `<prefix>-<version>.vsix`; the version comes from the file name) and `LUM_VSIX_EDITOR` (path to a fake editor CLI that overrides editor resolution).
