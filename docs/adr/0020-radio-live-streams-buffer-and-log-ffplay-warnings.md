# Radio Live Streams Buffer Ahead and Log ffplay Warnings

## Status

Accepted. Supersedes the "ffplay stderr stays nulled" consequence of ADR-0014.

## Context

RadCap stations (`rdrn`, `cavt`) played with frequent audible gaps while `status` reported `playing` and `lum.log` held nothing. Measurement showed two causes:

- The servers send 320 kbps AAC at exactly realtime (~40 KB/s) after a ~2s connect burst. ffplay caps its input queue at roughly one second for HTTP sources, so any network jitter longer than that drains the queue and the output underruns. A probe showed a steady `aq=40KB` queue.
- ffplay's stderr was nulled and its log level was `error`, so reconnects and stalls inside a single ffplay session never reached the log. The runner only logs whole-attempt exits.

## Decision

Live stations add `-infbuf -af adelay=delays=5000:all=1` to ffplay. `-infbuf` removes the input queue cap, and the 5s of leading silence lets the queue fill while nothing is audible. The same probe then settled at `aq≈260KB` (about 6.5s of cushion). Playlist playback keeps the base command.

All ffplay invocations run at `-loglevel warning`, and the runner pipes ffplay's stderr into the rotating log as `warn` events tagged with the stream URL. Healthy streams emit nothing at this level, so the log stays quiet unless something goes wrong.

## Consequences

- Live stations start with 5s of silence, and playback stays about 5s behind the live edge.
- The cushion is fixed at startup; a sustained throughput deficit still drains it. After a drain, ffplay rebuilds only from whatever surplus the server sends.
- Stream warnings such as reconnects are now recorded in `lum.log`, so "audio keeps dropping" reports can be checked against the log.
