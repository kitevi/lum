# Blocking HTTP With ureq

## Status

Accepted

## Context

lum ran every command under a multi-threaded Tokio runtime, but no command did concurrent I/O: async code only awaited single `reqwest` calls, and `spawn_blocking` wrappers existed only to leave the runtime again. Part of the code already used `reqwest::blocking`, so lum carried both the async and blocking HTTP stacks. GitHub release lookup was implemented three times (`github.rs`, `ffmpeg.rs`, `yt/deps.rs`) and file downloads twice (`github.rs`, `font/install.rs`).

## Decision

Make lum fully synchronous and use `ureq`, the blocking HTTP client listed by blessed.rs. Drop `tokio` and `reqwest`.

All HTTP goes through `crate::github`: one agent (`lum/<version>` user agent, 30s connect timeout, 60s read-stall timeout, no total-time cap), `latest_release`, `GitHubRelease::asset_url`, streaming `download_asset`, and `download_bytes`. Downloads read through `into_reader()` because ureq's buffered helpers cap bodies at 10 MB.

ureq's `gzip` feature stays off: transparent decoding would decompress a `.tar.gz` served with `Content-Encoding: gzip`, breaking backup verification.

The backup upload uses `ureq::unversioned::multipart`, which ureq exempts from semver. Pin ureq's minor version when upgrading and re-check the upload if that module changes.

## Consequences

Tokio, hyper, h2, tower, and reqwest leave the dependency tree, and async/`spawn_blocking` plumbing leaves the code. Downloads and uploads stream from and to disk instead of buffering whole archives.

The radio respawn test runs the station runner on a thread, which cannot be aborted; it relies on the stopped fixture making later attempts fail fast until the test process exits.
