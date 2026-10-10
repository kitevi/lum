# Backup Restore Extracts Archives Into a Staging Directory

Status: Accepted

## Context

`lum backup <target> <code>` restores by downloading the archive behind the
code from a public, unauthenticated file host and unpacking it into the home
directory. The download was checked by `verify_tar_gz`, but that check only
required the archive to *contain* the target directory; any other entries
(`.zshrc`, `.ssh/authorized_keys`, ...) were also extracted, and the tar
crate's unpack defaults to overwriting existing files. A code supplied by
anyone could therefore overwrite arbitrary files inside the home directory.

The tar crate confines unpacked writes to the destination directory, but the
destination was the home directory itself, so that guarantee protected
nothing: symlink entries under the target directory can redirect later
writes to any other location inside the home directory as well.

## Decision

Restore treats the downloaded archive as untrusted input:

- `verify_tar_gz` rejects any entry that is not the target directory itself
  or below it, including entries containing `..` components. `./`-prefixed
  entry paths are normalised before the check.
- Extraction happens into a fresh staging directory inside the home
  directory. The tar crate's confinement then guarantees every write —
  including writes redirected through archive-supplied symlinks — lands
  inside staging.
- The staged entry for the target must be a real directory, and only that
  directory is renamed into place; the staging directory is removed
  afterwards.
- The existing target directory is only moved aside after the archive has
  been extracted and validated in staging.

## Consequences

Restoring a code that points at an archive carrying entries outside the
target directory fails with an explicit error instead of writing those
entries into the home directory.

The restore code path never writes anywhere in the home directory except the
target directory and its own `lum-restore-*` staging directory.

Backups created by lum itself round-trip unchanged: their entries are all
below the target directory. Archives from other tar implementations verify
as long as their entry paths resolve below the target directory.

A restore code remains an unauthenticated public filename; encrypting or
signing backup archives would be a separate decision.
