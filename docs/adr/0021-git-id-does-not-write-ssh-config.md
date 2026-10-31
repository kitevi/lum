# Git Identity Does Not Write SSH Config

## Status

Accepted. Supersedes the "default domain identity" consequence of ADR-0007.

## Context

`lum git-id sync` wrote a `Host <domain>` block to `~/.ssh/config`, naming the first SSH identity's key for each domain, so that direct SSH commands had a default identity. That block reached much further than those commands:

- OpenSSH adds every `IdentityFile` line to the list of identities to try, and `IdentitiesOnly` only filters out extra agent keys. The per-folder `ssh -i <key>` therefore also offered the block's key, so a rejected key could fall through to another identity. Hosting services identify the account by key, so that is a login as the wrong account.
- Outside managed folders, every SSH connection to the domain offered only that key, dropping default and agent keys.
- `~/.ssh/config` is parsed for every host, so one bad generated line stopped `ssh` for the whole machine.

## Decision

Sync no longer writes `~/.ssh/config`, except to remove the section older versions left there. Folder routing stays `core.sshCommand` with `ssh -i <key> -o IdentitiesOnly=yes`, the command GitHub documents for this purpose. Anyone who wants a default key for a host writes the three-line `Host` block themselves.

## Considered Options

- Keep the block: leaves all three problems in place.
- Make the block opt-in per identity: adds a config field and validation to save writing three lines, and keeps lum writing a file that can break `ssh`.
- Add `-F none` to `core.sshCommand`: GitLab documents the equivalent `-F /dev/null`, and it would also stop fall-through to the user's own `IdentityFile` lines, but it ignores all of the user's SSH settings. That breaks every host that needs one, such as `ProxyJump`, with no way around it.
- Keep the block and exclude managed folders with `Match tag`: needs OpenSSH 9.4, and older clients treat the unknown attribute as a fatal configuration error for every host, which `IgnoreUnknown` does not cover.
- SSH host aliases plus URL rewriting: already rejected by ADR-0007, and still writes `~/.ssh/config`.

## Consequences

- `ssh -T git@github.com` no longer tests a lum key. Pass the key explicitly with `-i`.
- Tools that run Git outside the managed folder, such as Cargo with `net.git-fetch-with-cli`, use the user's own SSH configuration rather than a lum key.
- If the user's own config names an `IdentityFile` for a host, a folder whose key is rejected can still fall through to it. Lum documents this rather than overriding the user's SSH settings.
- The first sync after upgrading changes SSH behaviour outside managed folders for anyone who relied on the old block, and says so when it removes the section.
