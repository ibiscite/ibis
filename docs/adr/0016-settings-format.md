# 0016. Keep versioned, human-editable settings in the platform configuration folder

Status: accepted (2026-10-18); the `library` field and the migration rule are superseded by [0018](0018-settings-format-2.md)

## Context

The picker already saves preferences that users will keep across upgrades. Before
the first preview, their format, recovery behavior and future migration rule need
to be explicit. This records the existing implementation in
`crates/ibis-core/src/settings.rs` and the desktop's `SettingsFile` wrapper; it
does not introduce a new schema or migration implementation. See
[#135](https://github.com/ibiscite/ibis/issues/135).

## Decision

Use UTF-8 JSON in `settings.json`, written with `schema_version: 1`. The other
fields are optional (omitted or `null` means use the app's default):

| Field | Value |
| --- | --- |
| `shortcut` | Shortcut string, such as `CommandOrControl+Shift+Y` |
| `library` | Path string naming the BibTeX/BibLaTeX file to open |
| `copy_format` | `pandoc`, `autocite`, `parencite`, `textcite`, `cite`, `typst` or `keys` |

Missing fields use defaults; an omitted `schema_version` reads as 1. Unknown
fields are ignored when reading and are not preserved on a later save. Invalid
field types or unknown format names are reported as invalid settings. A numeric
schema version newer than this build supports is detected before decoding other
fields, so changed field types do not hide the newer-version error.

Use Tauri's `app_config_dir` for identifier `io.github.ibiscite.ibis`:

- macOS: `~/Library/Application Support/io.github.ibiscite.ibis/`
- Windows: `%APPDATA%\io.github.ibiscite.ibis\` (roaming)
- Linux: `$XDG_CONFIG_HOME/io.github.ibiscite.ibis/`, falling back to
  `~/.config/io.github.ibiscite.ibis/`

For 0.1, retain the Windows roaming location and accept that remembered absolute
library paths are machine-specific. Ibis neither copies a library nor translates
its path on another machine. An unavailable path produces a load error; **Choose
library** selects that machine's file. The same path on two machines may name
different libraries. Roaming preferences are not library synchronization.

A missing primary file gives defaults immediately. An unreadable, invalid or
newer primary reports its problem and tries `settings.json.bak`, then defaults.
Loading a backup does not repair or replace the primary. Desktop updates reread
the primary and refuse to overwrite an unreadable, invalid or newer file.

Saving writes `settings.json.tmp`, copies an existing primary to
`settings.json.bak`, then renames the temporary file over the primary. The
desktop's mutex serializes its own read/change/save operations. Rereading keeps
known-field edits made before an update; this is not locking against simultaneous
external edits or a guarantee of durability after power loss.

When the format first changes, increment `schema_version` and implement migration
as pure, tested functions for each supported version step, preserving recovery
and newer-version refusal. No migration step exists yet because v1 is the first
format; the first migration is deferred to
[#2](https://github.com/ibiscite/ibis/issues/2). A change to this contract needs a
superseding ADR under ADR 0001.

## Consequences

Users can inspect and edit a small local file, and upgrades have an explicit
compatibility boundary. Backups help recover bad edits, while errors remain
visible. Windows users may need to choose a library again on another machine.
Unknown fields and concurrent external changes have limited preservation; this
record does not promise a general-purpose configuration database.
