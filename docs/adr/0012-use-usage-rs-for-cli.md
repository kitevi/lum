# Use usage-rs for the CLI

## Status

Accepted

## Context

Lum's CLI was declared with clap's derive API plus clap_complete for shell
completions. [usage-rs](https://usage.jdx.dev) offers the same typed derive
model and additionally emits a portable usage spec (KDL) from the same
declaration, which powers generated docs, manpages, and dynamic shell
completions that call back into the binary.

## Decision

Parse and generate completions with usage-rs. clap and clap_complete are
removed as direct dependencies. The CLI remains declared as typed Rust structs
and enums in `src/cli.rs`; dispatch stays an explicit match in `src/main.rs`
rather than usage-rs's generated `Run` dispatch, because lum's runtime policy
(credential helper before logging, mixed sync/async handlers, blocking tool
work) stays local and inspectable there.

Both `-V` and `--version` keep printing the full build-metadata version.
The hidden `__completions` command is preserved so existing shell startup
files keep working; it accepts the same five shells as before
(bash, elvish, fish, powershell, zsh).

## Consequences

- This is a deliberate deviation from the blessed.rs soft rule, which lists
  clap for argument parsing. Justification: usage-rs's spec/docs/completion
  tooling is the capability lum wants, and its derive API keeps lum's CLI
  declaration a mechanical rename away from clap's.
- usage-rs requires Rust 1.91. lum's `rust-version` is the highest minimum
  among its dependencies, which sysinfo 0.40 raised to 1.95.
- Help layout and completion script contents change; accepted argv, exit
  codes, and output channels do not.
- Lum gains the `__usage_spec__` endpoint that prints its usage spec (KDL),
  validated in tests by round-tripping `Cli::to_kdl()` through usage-lib.
- Completion scripts now call back into `lum` for candidates instead of
  embedding static word lists.
