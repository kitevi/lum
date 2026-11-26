# ffmpeg Auto-Provisioning

Status: Superseded by ADR-0036: lum no longer downloads ffmpeg or ffplay.

Lum resolves ffmpeg through the same broad dependency flow as yt-dlp: prefer a system binary on `$PATH`, fall back to a managed copy in `data_dir()/deps/`, then fail with install guidance.

The managed copy is supported on Linux, Windows and macOS, for x86_64 and aarch64. Linux and Windows builds come from BtbN/FFmpeg-Builds using GPL, non-shared/static assets:

- `ffmpeg-master-latest-linux64-gpl.tar.xz`
- `ffmpeg-master-latest-linuxarm64-gpl.tar.xz`
- `ffmpeg-master-latest-win64-gpl.zip`
- `ffmpeg-master-latest-winarm64-gpl.zip`

macOS was first left PATH-only, because the evaluated public binary providers either did not publish Apple Silicon builds or published outdated ffmpeg versions. It now takes the latest snapshot builds from ffmpeg.martin-riedl.de, which publishes both Apple Silicon (`arm64`) and Intel (`amd64`), as a zip with the `ffmpeg` binary in it. Lum installs `ffplay` from an archive only when the archive holds one, so where the archive has none, radio playback needs a user-installed ffplay (ADR-0002).

BtbN publishes under a rolling `latest` release tag, and martin-riedl's link always points at the latest snapshot, so lum does not compare versions. Instead, `data_dir()/deps/ffmpeg.json` records the last successful download timestamp. A managed copy older than 14 days is considered stale and is refreshed. If refresh fails and a cached binary exists, lum logs a warning and keeps using the cached copy.

For minimal parity with yt-dlp provisioning, downloads are trusted through HTTPS and are not checked against a checksum, including BtbN's `checksums.sha256`. An escape hatch, `LUM_FFMPEG_DISABLE_AUTO_PROVISION`, disables automatic provisioning and preserves PATH-only behavior. Tests can use `LUM_FFMPEG_TEST_ARTIFACT` to install a local fake executable without network access.
