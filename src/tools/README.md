# Tools Subcommand

`lum tools` manages a hardcoded catalog of developer tools installed into the same directory printed by `lum env bin-dir`.

## CLI Shape

```sh
lum tools install <tool> [--force]
lum tools list
lum tools status <tool>
lum tools sync [--dry-run]
lum tools uninstall <tool>
lum tools update <tool> [--force]
```

There is no opt-in catalog behavior and no `--all` flag. `sync` is the all-tools operation: it inspects every hardcoded tool, installs missing tools, updates outdated tools, and skips up-to-date tools. `update` targets one named tool only. `status` shows the installed version next to the latest release, which it looks up on GitHub. `uninstall` removes a tool lum installed and forgets it; it refuses a binary lum does not manage.

## Storage

Use lum's centralized platform directory policy (`directories::ProjectDirs::from("dev", "ppowo", "lum")`):

- state: `config_dir()/tools-state.json`
- entrypoints/install path: `data_dir()/bin`, matching `lum env bin-dir`

No vex state is read or migrated. If moving from vex, reinstall or sync tools manually with `lum tools`.

## Catalog

The managed tool catalog is currently empty. Add entries to `CATALOG` in `catalog.rs`.

Do not resurrect stale README-only tools from vex.

## Safety

`install` errors when a tool is already managed/installed unless `--force` is passed.

Existing unmanaged files in the install path are protected by default. `--force` may overwrite/take over an unmanaged file and record ownership in `tools-state.json`.

## Implementation

Use a clean Rust implementation. Keep the behavior and resolver logic from vex where useful, but do not port the Go structure directly. Avoid shelling out to `curl`, `tar`, or `unzip`; downloads, archive extraction, checksums, and installation should be pure Rust and cross-platform.

## Version Ordering

Versions use SemVer precedence: prereleases precede stable releases, and build metadata does not affect ordering. A leading `v` is accepted, and omitted minor or patch components default to zero.

Identical version strings are unchanged. Differing versions that cannot be parsed report an error instead of silently treating invalid components as zero. `tools update --force` bypasses version ordering.

See [ADR 0016](../../docs/adr/0016-tool-version-ordering.md) for the format and error-handling decisions.
