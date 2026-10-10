# Archive Fixtures

`executables.tar.xz` is a GNU tar archive compressed with the system `xz` encoder using CRC64. Its headers have zero UID, GID, and modification times. It contains textual placeholders at `fixture/bin/ffmpeg`, `fixture/bin/ffplay`, and `fixture/bin/ctags`, plus an unrelated README. The placeholders are never executed.

Extraction tests embed the fixture; running the tests does not require `xz` or a native XZ library.
