# Tool Version Ordering

## Status

Accepted

## Context

`scc` releases use SemVer. The managed `universal-ctags` nightly catalog uses date tags such as `2026.10.08+<commit>`, which can contain leading zeros and are not SemVer. The numeric comparator discarded prerelease and build information and silently converted malformed components to zero.

## Decision

- Use `semver::Version::cmp_precedence` for `scc`, accepting a leading `v` and filling omitted minor or patch components with zero.
- Use the existing `chrono::NaiveDate` parser for ctags nightly dates. Commit metadata after `+` does not affect ordering.
- Treat identical version strings as unchanged, including opaque tags. Report errors for differing versions that cannot be parsed. `tools update --force` bypasses ordering.

`semver` is the standard Cargo-ecosystem parser, works across the supported platforms, and is already present transitively in the lockfile. It is an established fallback where blessed.rs does not cover version comparison.

## Consequences

Prereleases precede stable releases, and build metadata does not trigger upgrades. Zero-padded nightly dates remain valid without pretending they are SemVer. Unsupported differing tags require an explicit forced update rather than a guessed numeric ordering.

CLI arguments and stored version strings remain unchanged. Consumer-facing behavior is documented in [the tools README](../../src/tools/README.md).
