# Tool Version Ordering

## Status

Accepted. The managed tool catalog is empty now: `scc` and `universal-ctags`, the entries that made this decision, were removed. The ordering stays for whatever is added next.

## Context

`lum tools update` has to tell whether a tool's latest release is newer than the one installed. Release tags are mostly SemVer, often with a leading `v`, but a catalog entry can use other tags: the `universal-ctags` nightlies used date tags such as `2026.10.08+<commit>`, which can contain leading zeros and are not SemVer. The numeric comparator first used discarded prerelease and build information and silently converted malformed components to zero.

## Decision

- Order versions with `semver::Version::cmp_precedence`, accepting a leading `v` and filling omitted minor or patch components with zero.
- Treat identical version strings as unchanged, including opaque tags. Report errors for differing versions that cannot be parsed. `tools update --force` bypasses ordering.

`semver` is the standard Cargo-ecosystem parser, works across the supported platforms, and is already present transitively in the lockfile. It is an established fallback where blessed.rs does not cover version comparison.

## Consequences

Prereleases precede stable releases, and build metadata does not trigger upgrades. Unsupported differing tags require an explicit forced update rather than a guessed numeric ordering. A tool whose tags are not SemVer, such as dates, needs an ordering of its own; `universal-ctags` had one, by date, until it was removed.

CLI arguments and stored version strings remain unchanged. Consumer-facing behavior is documented in [the tools README](../../src/tools/README.md).
