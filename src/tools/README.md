# Tools Subcommand

`lum tools` manages a hardcoded catalog of developer tools installed into the same directory printed by `lum env path`.

## CLI Shape

```sh
lum tools install <tool> [--force]
lum tools ls
lum tools status <tool>
lum tools sync [--dry-run]
lum tools update <tool> [--force]
lum tools version <tool>
```

There is no opt-in catalog behavior and no `--all` flag. `sync` is the all-tools operation: it inspects every hardcoded tool, installs missing tools, updates outdated tools, and skips up-to-date tools. `update` targets one named tool only.

## Storage

Use lum's centralized platform directory policy (`directories::ProjectDirs::from("dev", "ppowo", "lum")`):

- state: `config_dir()/tools-state.json`
- entrypoints/install path: `data_dir()/bin`, matching `lum env path`

No vex state is read or migrated. If moving from vex, reinstall or sync tools manually with `lum tools`.

## Catalog

The managed tool catalog contains only:

- `scc` → `scc`
- `universal-ctags` → `ctags`

Do not resurrect stale README-only tools from vex.

## Safety

`install` errors when a tool is already managed/installed unless `--force` is passed.

Existing unmanaged files in the install path are protected by default. `--force` may overwrite/take over an unmanaged file and record ownership in `tools-state.json`.

## Implementation

Use a clean Rust implementation. Keep the behavior and resolver logic from vex where useful, but do not port the Go structure directly. Avoid shelling out to `curl`, `tar`, or `unzip`; downloads, archive extraction, checksums, and installation should be pure Rust and cross-platform.

## Version Ordering

`scc` versions use SemVer precedence: prereleases precede stable releases, and build metadata does not affect ordering. A leading `v` is accepted, and omitted minor or patch components default to zero.

`universal-ctags` nightly versions use calendar dates (`YYYY.MM.DD`); commit metadata after `+` does not affect ordering.

Identical version strings are unchanged. Differing versions that cannot be parsed report an error instead of silently treating invalid components as zero. `tools update --force` bypasses version ordering.

See [ADR 0016](../../docs/adr/0016-tool-version-ordering.md) for the format and error-handling decisions.
