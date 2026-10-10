# Repository Worker Pools Use Rayon

## Status

Accepted

## Context

Repository scanning, mirror sync, and mirror status each implemented chunked thread spawning and joining. A slow job blocked scheduling of the next chunk even when another worker was idle. The three copies also owned separate result and error aggregation loops.

## Decision

Use a dedicated Rayon pool per command. Its size is bounded by both `max(1, jobs)` and the number of repositories; empty commands return before creating a pool. Do not use Rayon's global pool, whose size cannot honor each invocation's `-j` setting.

Use indexed parallel collection for scan results so output remains in path order. Use a full reduction for mirror sync and collect every status outcome before deciding the exit code. Do not use short-circuiting parallel predicates that could skip repositories after an error.

Rayon is listed by blessed.rs and supports Linux, macOS, and Windows. Its Rust 1.80 minimum is below lum's Rust 1.91 minimum.

## Consequences

Worker reuse removes chunk barriers and repeated manual spawn/join code. Mirror commands still attempt every configured repository, and status errors retain precedence over behind results.

Scanner results are buffered until inspections finish, so the first result appears later than with chunk-by-chunk printing. Inspections are independent and can complete before an earlier ordered result reports an error. Unexpected worker panics propagate through Rayon instead of being converted into ordinary mirror-operation errors in debug builds; release builds already abort on panic.

A local Git handshake test verifies that a free worker starts the next repository while another job is blocked, without relying on a throughput timing threshold. Pool-sizing and CLI tests cover job limits, sorted scan results, continued mirror work after errors, and exit-code precedence.
