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

- `name` is the stable identity ID. Renaming creates a new identity and orphans old marked artifacts. Removing or renaming an identity also drops it from `allowed_signers`, so Git reports its old commits as "No principal matched".
- `name` becomes part of file names, so it must not contain path separators or control characters, or end in `.pub`.
- `author_name` is Git's `user.name`; it is not a hosting-service username.
- `email` must not contain whitespace, quotes, or commas, which change the meaning of an `allowed_signers` line.
- Duplicate identity names are rejected.
- Duplicate exact managed folders are rejected.
- Duplicate `email + domain` is rejected.
- Duplicate `author_name + domain` is rejected.
- Same email or author name across different domains is allowed.
- `authentication.username` is the hosting-service login; it is separate from `author_name`.
- `authentication.password` is stored as plaintext in this config. Prefer a narrowly scoped access token when the provider supports one.
- A domain is a host with an optional port, without a URL scheme or repository path. Uppercase letters are lowercased. On an SSH identity, a port applies to both the SSH URL and the HTTPS URL it rewrites.
- A folder must be an absolute path or start with `~/`.
- Unknown fields are rejected, so a misspelled key cannot silently change behavior. Unknown fields inside `authentication` are ignored, because a misspelled required field already fails as missing.

Git merges every matching include, and a later config cannot unset an inherited `url.<base>.insteadOf`, so an SSH identity's SSH command and HTTPS rewrite still apply in folders nested under it:

- Another SSH identity on the same domain can be nested safely. So can anything under an HTTP identity, whose credentials only apply to its own host.
- An HTTPS identity on the same domain is rejected, because the rewrite means its credentials would never be used.
- Other nestings sync with a warning. They work for the inner identity's own domain, but the outer identity's routing also applies there.

## Language

**Git identity**: A named profile that represents one Git commit author and authentication setup for repositories under one or more managed folders.

**Author name**: The human-readable Git commit author name written to `user.name`. Avoid using `user` or `username` for this field.

**Managed folder**: A directory tree where repositories should automatically use a specific Git identity. When managed folders overlap, the most specific matching folder wins for each setting it defines.

## Sync Behavior

`sync` is declarative: the JSON config is the source of truth.

For each configured identity, sync:

1. Restricts the JSON config to owner-only permissions on Unix.
2. Creates managed folders.
3. Generates a namespaced Ed25519 SSH signing key via `ssh-keygen` when missing.
4. Writes a per-identity Git config with either SSH routing or an HTTP credential helper.
5. Updates lum-marked sections in global `~/.gitconfig`.
6. Removes any lum-marked section an older version left in `~/.ssh/config`.
7. Updates lum-marked sections in `~/.ssh/allowed_signers`.
8. Backs up and removes orphaned lum-marked artifacts.
9. Deletes backups older than 30 days.

An identity that fails, for example because of an unmarked key at its path, is left out of the global files. The others still converge, and sync exits non-zero.

## Upgrading

`sync` converts everything older versions generated, without any edits to the JSON config, which lum never rewrites:

- Per-identity Git configs and managed sections are rewritten with quoted values.
- The `Host` section in `~/.ssh/config` is removed, leaving the rest of that file as it was (ADR 0021). Sync prints what it removed, so you can add it back yourself if you relied on it outside managed folders.
- Existing backups are restricted to owner-only permissions.
- Keys are reused, so nothing needs uploading again and old signatures still verify.

Until that sync, the generated files keep working and `status` reports `needs sync`. A config that needs an edit fails `sync` with a specific error and changes nothing. The credential helper reads only the fields it needs and does not apply the other rules on this page, so a problem elsewhere in the config cannot block Git authentication.

## Status and Where

`status` compares each identity's key, Git config, and routing with what `sync` would write, and reports `active` or `needs sync (<reason>)`.

`where` asks Git where `user.email` comes from in the current repository, so it reports the identity Git is really using and fails if a setting overrides it. Outside a repository it reports the identity for the managed folder.

Git matches `includeIf "gitdir:"` against the real `.git` directory, so a linked worktree follows its main repository, not the folder it is checked out in.

## Ownership and Safety

Git ID uses marker-only ownership, not a state file. Lum may mutate or delete an artifact only when it carries a valid `lum:git-id` marker.

Markers:

```text
# lum:git-id:managed identity=github-work
[lum:git-id identity=github-work]
# lum:git-id:begin
# lum:git-id:end
```

Unmarked files at generated paths are conflicts and must not be overwritten or deleted automatically. Unbalanced `begin` and `end` markers stop sync and leave the file unchanged, because lum cannot tell which lines it owns.

Lum's section in `~/.gitconfig` must stay last to take precedence. Git appends new sections to the end of the file, so `git config --global user.email` on a file with no `[user]` section overrides every identity. `status` and `where` report this, and the next `sync` moves lum's section back to the end.

Generated keys have no passphrase. Each orphan backup is a separate owner-only archive, because it can hold a private key.

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
  name = "Jane Doe"
  email = "jane@company.com"
  signingkey = "/absolute/path/to/.ssh/lum-git-id-github-work.pub"

[commit]
  gpgsign = true

[gpg]
  format = ssh

[gpg "ssh"]
  allowedSignersFile = "/absolute/path/to/.ssh/allowed_signers"

[core]
  sshCommand = "ssh -i '/absolute/path/to/.ssh/lum-git-id-github-work' -o IdentitiesOnly=yes"

[url "ssh://git@github.com/"]
  insteadOf = https://github.com/
```

The HTTPS-to-SSH rewrite is scoped to managed folders through the per-identity config. Lum does not rewrite repository remotes.

### Outside managed folders

Lum does not write `~/.ssh/config` (ADR 0021), so direct SSH commands and Git operations outside managed folders use your own SSH configuration. To test an identity's key directly:

```sh
ssh -T -o IdentitiesOnly=yes -i ~/.ssh/lum-git-id-github-work git@github.com
```

Tools that run Git outside the managed folder, such as Cargo with `net.git-fetch-with-cli`, do not get the identity's key. To use one by default for a host, add this to `~/.ssh/config` yourself:

```sshconfig
Host github.com
  IdentityFile ~/.ssh/lum-git-id-github-work
  IdentitiesOnly yes
```

`IdentityFile` lines accumulate, and `IdentitiesOnly` only filters agent keys. With a block like this, or an `IdentityFile` under `Host *`, a managed folder whose own key is rejected falls through to that key, which can mean another account. `ssh -G <host>` lists the keys that apply.

### HTTP Basic authentication

HTTP-authenticated identities keep the SSH public key for commit signing, but omit `core.sshCommand` and the HTTPS-to-SSH rewrite. Their folder-scoped Git config contains:

```gitconfig
[credential "http://gitlab.internal.example"]
  username = "jane"
  useHttpPath = false
  helper =
  helper = "!'/absolute/path/to/lum' __git_credential <route-id>"
```

The empty helper resets lower-priority global helpers for this exact protocol and host. Git then invokes lum through its standard credential-helper protocol. Lum returns a credential only when the route, protocol, host, and any supplied username match the identity. `store` and `erase` never rewrite the JSON config.

Changing only `authentication.password` takes effect on the next Git operation. Run `lum git-id sync` after changing the authentication type, scheme, username, domain, identity name, or lum executable location.
