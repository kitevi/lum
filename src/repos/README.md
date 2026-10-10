# Repos Subcommand

`lum repos` scans directory trees for Git repositories and manages curated mirror clones.

## CLI Shape

```
lum repos scan [--hidden] [-j N] [PATH]
lum repos mirror config-path
lum repos mirror dir
lum repos mirror init
lum repos mirror list
lum repos mirror sync [-j N]
lum repos mirror status [-j N] [--offline]
lum repos mirror watch [TAG]
```

## Scanner (`repos scan`)

Walks a directory tree looking for `.git/` directories. By default, for each repo with a current-branch upstream, runs `git fetch --no-tags --no-recurse-submodules <upstream-remote>` before `git status --porcelain=v1 --branch`. This updates remote-tracking refs but does not modify the worktree. Use `--offline` to avoid contacting remotes or updating refs.

- **Branch** (including detached HEAD)
- **Worktree status** (clean / has uncommitted changes)
- **Upstream sync** (synced / ahead / behind / diverged)

Output: `<path>  <branch>  <status>; <upstream-sync>`

| Flag | Purpose |
|------|---------|
| `[PATH]` | Scan root; defaults to `.` |
| `--hidden` | Descend into dot-prefixed directories |
| `--offline` | Use cached remote refs only; do not fetch |
| `-j N` | Max concurrent git operations (default 4) |

Skips symlinks, `.git/` directories, and (by default) hidden directories. Reports nested repos, including paths excluded by `.gitignore` or `.ignore` files.

## Mirror (`repos mirror`)

Maintains shallow (`--depth 1 --no-single-branch`) read-only clones under `~/Documents/CodeMirror/`. Configuration lives in lum's centralized config directory as `repos.json`.

### Config File

```json
{
  "repos": [
    {
      "url": "https://github.com/example-org/example-repo.git",
      "branch": "main",
      "tags": ["sample"]
    }
  ]
}
```

- `url` is required and must start with `https://`, `git@`, `ssh://`, or `file://`.
- `branch` defaults to `"main"`.
- `tags` are optional metadata appended to directory names.

### Directory Naming

`<basename>-<branch>[-<tag1>-<tag2>-...]`

Where `basename` is the repo URL's last path segment with `.git` stripped, and `/` in branch names is replaced with `-`.

### Exit Codes for `mirror status`

| Code | Meaning |
|------|---------|
| 0 | All mirrors up to date |
| 1 | An error occurred |
| 2 | At least one mirror is behind |

### Sync Behavior

- New repos: `git clone --depth 1 --branch <branch> --no-single-branch --no-tags`
- Existing repos: `git fetch --depth 1 --all --no-tags && git checkout <branch> && git reset --hard origin/<branch>`
- Continues past individual errors; exits non-zero if any failed

### Safety

`validate_in_mirror` ensures all git operations target paths inside the CodeMirror directory (path-traversal guard).

## Worker Pools

`repos scan`, `mirror sync`, and `mirror status` use a dedicated bounded worker pool per command. `-j` limits concurrent repository operations; zero means one worker, and the pool never exceeds the number of repositories. Workers accept another repository as soon as they finish instead of waiting for a batch.

Scan results are buffered and printed in path order after all inspections finish. Mirror commands retain per-repository progress and attempt every configured repository before aggregating errors. For mirror status, errors (exit 1) take precedence over behind or missing repositories (exit 2).

See [ADR 0018](../../docs/adr/0018-repository-worker-pools-use-rayon.md) for the scheduling and reporting trade-offs.

## Watch (`repos mirror watch`)

Long-running foreground process that polls mirror-configured remotes and sends a desktop notification when a tracked branch's HEAD SHA changes — signaling that it's time to rebase.

### Usage

```
lum repos mirror watch <tag>
```

- `<tag>` selects repos by their configured `tags` field. Only repos matching that tag are watched.
- Without `<tag>`: prints guidance (list tags via `mirror list`, or run `mirror init` if no repos configured) and exits.

### Behavior

- On startup: prints "Watching repositories with the {tag} tag" and lists each matching repo (basename/branch). Sends a desktop notification with the same message.
- Polls every **5 minutes** (hardcoded, no CLI flag).
- Uses `git ls-remote` per watched repo — no local clone needed, no mutations.
- **First cycle**: records SHAs silently as baseline. No change notifications.
- **Subsequent cycles**: if a SHA differs from the stored one, sends a desktop notification and updates state.
- Ctrl+C to stop.

### State

`repos-watch-state.json` in lum's state directory. Maps `<url> <branch>` → last-known SHA.

### Notification

Via `notify-rust` crate. Body: `<repo-basename>/<branch> — HEAD changed to <short-sha>`.
