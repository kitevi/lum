# YouTube Radio Uses yt-dlp and ffplay

Status: Superseded by ADR-0002; the YouTube radio stations were removed by ADR-0031; the YouTube radio stations were removed by ADR-0031

YouTube-backed `lum radio` stations are treated as normal built-in radio stations at the CLI layer. Current radio playback resolves YouTube station page URLs with `yt-dlp -g --no-playlist`, then gives the resolved media URL to `ffplay`.

This ADR previously described a mixed path where YouTube radio used `yt-dlp` plus `ffmpeg` to emit PCM into lum's native CPAL/Symphonia audio implementation. That path is no longer the product direction. ADR-0002 supersedes it: `lum radio` controls an external `ffplay` process and does not own audio decoding, buffering, or audio output.
