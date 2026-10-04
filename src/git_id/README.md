# Git ID Subcommand

`lum git-id` manages folder-based Git identities. The user edits one JSON config, then `lum git-id sync` converges signing keys and folder-scoped Git authentication from that source of truth.

## CLI Shape

```sh
lum git-id config-path
lum git-id init
lum git-id sync
lum git-id status
lum git-id where
lum git-id info <identity>
lum git-id pubkey <identity>
lum git-id paths
```

## Config

The config path is resolved through lum's centralized platform directory policy and printed by `lum git-id config-path`.

```json
{
  "identities": [
    {
      "name": "github-work",
      "author_name": "Jane Doe",
      "email": "jane@company.com",
      "domain": "github.com",
      "folders": ["~/Work/Github"]
    }
  ]
}
```

HTTP Basic authentication is optional per identity. The `password` value may be an account password or a provider access token:

```json
{
  "identities": [
    {
      "name": "internal-work",
      "author_name": "Jane Doe",
      "email": "jane@company.com",
      "domain": "gitlab.internal.example",
      "folders": ["~/Work/Internal"],
      "authentication": {
        "type": "http-basic",
        "scheme": "http",
        "username": "jane",
        "password": "replace-me",
        "allow_insecure_http": true
      }
    }
  ]
}
```

Omitting `authentication` keeps the existing SSH behavior. For HTTPS, set `scheme` to `https` and omit `allow_insecure_http`. Plain HTTP transmits the username and password/token without encryption, so lum rejects it unless `allow_insecure_http` is explicitly `true`.

- `name` is the stable identity ID. Renaming creates a new identity and orphans old marked artifacts.
- `author_name` is Git's `user.name`; it is not a hosting-service username.
- Duplicate identity names are rejected.
- Duplicate exact managed folders are rejected.
- Duplicate `email + domain` is rejected.
- Duplicate `author_name + domain` is rejected.
- Same email or author name across different domains is allowed.
- `authentication.username` is the hosting-service login; it is separate from `author_name`.
- `authentication.password` is stored as plaintext in this config. Prefer a narrowly scoped access token when the provider supports one.
- A domain is a host with an optional port, without a URL scheme or repository path.

## Language

**Git identity**: A named profile that represents one Git commit author and authentication setup for repositories under one or more managed folders.

**Author name**: The human-readable Git commit author name written to `user.name`. Avoid using `user` or `username` for this field.

**Managed folder**: A directory tree where repositories should automatically use a specific Git identity. When managed folders overlap, the most specific matching folder wins.

**Default domain identity**: The deterministic fallback identity used for a hosting domain when no managed folder context exists, such as direct SSH checks. It is not folder-aware.

## Sync Behavior

`sync` is declarative: the JSON config is the source of truth.

For each configured identity, sync:

1. Restricts the JSON config to owner-only permissions on Unix.
2. Creates managed folders.
3. Generates a namespaced Ed25519 SSH signing key via `ssh-keygen` when missing.
4. Writes a per-identity Git config with either SSH routing or an HTTP credential helper.
5. Updates lum-marked sections in global `~/.gitconfig`.
6. Updates lum-marked sections in `~/.ssh/config` for SSH-authenticated identities.
7. Updates lum-marked sections in `~/.ssh/allowed_signers` for every identity.
8. Backs up and removes orphaned lum-marked artifacts.
9. Deletes backups older than 30 days.

## Ownership and Safety

Git ID uses marker-only ownership, not a state file. Lum may mutate or delete an artifact only when it carries a valid `lum:git-id` marker.

Markers:

```text
# lum:git-id:managed identity=github-work
[lum:git-id identity=github-work]
# lum:git-id:begin
# lum:git-id:end
```

Unmarked files at generated paths are conflicts and must not be overwritten or deleted automatically.

At rest, the HTTP password/token remains only in `git-identities.json`; generated Git configs contain the username and a command that invokes lum, but not the password. Lum does not use an OS keyring or generate a Git credential-store file. On Unix, `init` creates the config as mode `0600`, `sync` restores that mode, and the credential helper refuses to expose a secret if group or other permissions are present. Windows relies on the ACL of the platform user config directory.

## Generated Paths

Generated files are namespaced with `lum-git-id-`:

```text
~/.ssh/lum-git-id-<identity>
~/.ssh/lum-git-id-<identity>.pub
~/.gitconfig-lum-git-id-<identity>
```

`lum git-id pubkey <identity>` prints only the public key to stdout for clipboard piping.

## Git Authentication Routing

Folder-specific routing is done with Git `includeIf` sections and per-identity Git configuration.

After creating managed folders, sync resolves their symlinks before writing `includeIf "gitdir:<folder>/"` patterns. This keeps routing consistent when Git reports the physical repository path, such as `/private/var/...` instead of `/var/...` on macOS. Windows path separators are rendered as forward slashes, preserving drive and UNC network roots. Backslashes in Unix folder names remain literal characters.

Folder paths are literal, not globs: sync escapes wildcard characters and quotes before writing Git conditions. Sync rejects newlines in resolved folder paths instead of writing an invalid global Git config.

SSH-authenticated identity configs include:

```gitconfig
[user]
  name = Jane Doe
  email = jane@company.com
  signingkey = /absolute/path/to/.ssh/lum-git-id-github-work.pub

[core]
  sshCommand = "ssh -i /absolute/path/to/.ssh/lum-git-id-github-work -o IdentitiesOnly=yes"

[commit]
  gpgsign = true

[gpg]
  format = ssh

[gpg "ssh"]
  allowedSignersFile = /absolute/path/to/.ssh/allowed_signers

[url "ssh://git@github.com/"]
  insteadOf = https://github.com/
```

The HTTPS-to-SSH rewrite is scoped to managed folders through the per-identity config. Lum does not rewrite repository remotes.

Direct SSH commands such as `ssh -T git@github.com` are not folder-aware; they use the default domain identity from the generated SSH config.

### HTTP Basic authentication

HTTP-authenticated identities keep the SSH public key for commit signing, but omit `core.sshCommand`, the HTTPS-to-SSH rewrite, and the SSH `Host` entry. Their folder-scoped Git config contains:

```gitconfig
[credential "http://gitlab.internal.example"]
  username = "jane"
  useHttpPath = false
  helper =
  helper = "!'/absolute/path/to/lum' __git_credential <route-id>"
```

The empty helper resets lower-priority global helpers for this exact protocol and host. Git then invokes lum through its standard credential-helper protocol. Lum returns a credential only when the route, protocol, host, and any supplied username match the identity. `store` and `erase` never rewrite the JSON config.

Changing only `authentication.password` takes effect on the next Git operation. Run `lum git-id sync` after changing the authentication type, scheme, username, domain, identity name, or lum executable location.
