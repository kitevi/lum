# Apps Own Settings, Not Config Files

## Status

Accepted. ADR-0027 changes how owned settings are written.

## Context

`lum apps` should leave an app configured, not just installed. An app's config file mixes preferences lum has an opinion on, machine-specific paths (BIOS files, ROM folders), controller mappings set in the app, and bookkeeping such as recent files. mGBA rewrites its whole `config.ini` from memory on every settings change and every ROM load, dropping comments and key order.

Some apps cannot be configured through a file at all: OpenEmu keeps its preferences in a plist that `cfprefsd` caches, which is unsafe to edit directly.

## Decision

- An app may list **owned settings**: single INI keys (section, key, value) in the app's own config file, at the path the app itself reads on each OS. lum never owns a whole file, and apps whose configuration is not a plain file get no settings.
- Values are hardcoded in the catalog, like env aliases and the tools catalog, so one lum binary configures every machine the same way.
- `install` and `update` apply owned settings after installing; `sync` also puts back **drift**, an owned key whose value is missing or different. `status` and `ls` show drift. Drift is read from the file each time; no settings state is stored.
- A write changes only owned keys and replaces the file with one rename, keeping its permissions. How a line is read and written is ADR-0027's.
- lum refuses to write settings while the app runs, because the app would overwrite them on its next save.
- The first owned setting is mGBA's `updateAutoCheck=0`: lum installs updates, and mGBA's updater would replace lum's files and leave lum's recorded version wrong.

## Consequences

- An owned setting changed in the app lasts until the next `lum apps sync`. To change it for good, change the catalog.
- A key lum stops owning keeps its last value in the file.
- Comments and key order survive a write, since ADR-0027 edits the file line by line instead of rewriting it. mGBA drops them on its own saves anyway.
