# xtask

`xtask` contains repository maintenance tasks that should not live in the main `lum` binary.

## Local Install

Canonical command:

```sh
cargo local-install
```

The repo-local Cargo alias runs:

```sh
cargo run --package xtask -- install
```

The install task:

1. builds `lum` with `cargo build --release`
2. selects a user-owned install directory from `PATH`
3. stages `target/release/lum` or `target/release/lum.exe` in a unique temporary file in that directory
4. sets executable permissions on Unix, then replaces the installed binary
5. prints the installed path

## Install Directory Policy

Without `PREFIX`, choose the first preferred directory that appears in `PATH`:

1. `$HOME/.bio/bin`
2. `$HOME/.local/bin`
3. `$HOME/bin`

If none are present in `PATH`, fall back to the first existing user directory among:

1. `$HOME/Desktop`
2. `$HOME/Downloads`
3. `$HOME`

Fallback installs warn because the installed command may not be directly runnable.

With `PREFIX`, install to `PREFIX/bin`. `PREFIX` must be absolute, inside the user's home directory, and `PREFIX/bin` must already appear in `PATH`.

## Windows Behavior

On Windows, the same preferred user-bin names are checked under `%USERPROFILE%`:

1. `%USERPROFILE%\.bio\bin`
2. `%USERPROFILE%\.local\bin`
3. `%USERPROFILE%\bin`

These are usually not present in `PATH` on a default Windows machine. In that case, `cargo local-install` intentionally falls back to Desktop, Downloads, or the home directory and prints warnings that the command may not be directly runnable.

Do not change the installer to use `Program Files`, require administrator privileges, or edit the user's `PATH` automatically.

## Rules

- Do not use sudo or system install directories.
- Do not replace Cargo's built-in `cargo install`.
- Keep this installer cross-platform in Rust rather than shell.
