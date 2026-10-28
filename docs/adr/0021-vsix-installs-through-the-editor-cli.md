# Vsix Installs Through the Editor CLI

## Status

Accepted

## Context

Some VS Code extensions lum should manage, starting with Lathe for Java, ship only as `.vsix` files attached to GitHub releases. They have no Marketplace or Open VSX listing, so the editor never updates them.

`lum tools` and `lum apps` install artifacts in pure Rust without shelling out. A `.vsix` is a zip archive, so lum could unpack it into the extensions directory itself. But VS Code, Cursor, and VSCodium each own an extensions directory plus an `extensions.json` registry whose format and location are internal details that vary by editor and platform. Writing them directly would break when an editor changes them.

## Decision

`lum vsix` installs and inspects extensions only through the editor's own CLI:

- `<editor> --install-extension <file> --force` installs a downloaded `.vsix`.
- `<editor> --list-extensions --show-versions` is the source of truth for what is installed. lum keeps no state file.

lum still owns release resolution, download, and integrity: it verifies the download against the SHA-256 digest GitHub publishes for each release asset before handing it to the editor, then checks that the editor reports the released version.

Requirements such as Java are checked before resolving the editor or touching the network.

## Consequences

- This is a deliberate exception to the pure-Rust, no-shelling-out rule used by `tools` and `apps`, in the same spirit as ADR 0002 (radio uses `ffplay`).
- An editor CLI must be available: on `PATH`, in a known default install location, or chosen with `--editor`. On Windows the CLI is a `.cmd` script, which Rust runs through `cmd.exe`.
- Without a state file there is no checksum pinning across runs; integrity relies on GitHub's per-asset digest. Older releases without a digest install unverified.
- Removing an extension through the editor UI is reflected immediately, because lum asks the editor every time.
