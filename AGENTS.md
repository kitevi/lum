# Agent Instructions

## Dependency Selection

1. **Check [blessed.rs](https://blessed.rs) first.** If a crate is listed for the category you need, prefer it. This is a soft rule — deviations are fine with a brief justification.
2. **Fallback: well-known Rust crates.** If blessed.rs doesn't cover the need, pick widely-adopted, well-maintained crates (e.g. `serde`, `tokio`). Avoid niche or unmaintained options.
3. **Cross-platform required.** Every dependency must compile and run on Linux, macOS, and Windows. Do not introduce crates that break on any of the big three.
4. **Prefer dependencies over own code.** Always take a well-maintained dependency instead of writing it yourself. The goal is as few lines to maintain in lum as possible.

## Local Documentation

Before changing a subsystem, read its local README:

- `src/radio/README.md` for the `lum radio` subcommand.
- `xtask/README.md` for repository maintenance tasks and local installation.

Keep `AGENTS.md` policy-level. Put subsystem implementation details in the subsystem README and durable trade-off decisions in ADRs under `docs/adr/`.

## Local Install Workflow

Use `cargo local-install` as the canonical local install command for lum. Do not customize or replace Cargo's built-in `cargo install`; treat it only as a standard Rust fallback.
