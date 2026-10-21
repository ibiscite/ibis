# 0022. Keep unknown settings fields and flush settings before replacing them

Status: accepted (2026-10-21); amends 0016

## Context

ADR 0016 lets a save drop fields this build does not know, and does not promise durability after a power cut. ADR 0019 and 0020 then added fields within format 2 on the basis that older builds ignore them. Preview testers move between Stable, Nightly and older builds, so each save from an older build silently erased settings a newer build had saved, such as the citation style. Saving also renamed an unflushed temporary file over `settings.json`; after a power cut some file systems leave that file empty, and only the previous state can be recovered from the backup. See [#678](https://github.com/ibiscite/ibis/issues/678).

## Decision

- **Unknown fields are kept.** A top-level field this build does not know is read as it is and written back, with its value, on every save, including after a format 1 file is migrated. Ibis never interprets, displays or logs these values.
- **Unknown values are not.** An unknown value of a known field, such as a newer citation style, still makes the file invalid, as ADR 0020 decides.
- **Saves are flushed.** The temporary file is written and flushed to disk before it replaces `settings.json`, and the backup copy is flushed. On Unix the folder is flushed after the rename, on a best-effort basis; NTFS journals the rename itself.

Everything else in ADR 0016 and 0018 still holds: location, the backup, recovery, refusing newer files, and migration.

## Consequences

- Builds that add fields within format 2 no longer erase each other's settings. ADR 0020's note that an older build's save drops the picker fields no longer applies to builds that include this change.
- A field added by hand is kept, typos included, until the person removes it.
- Each save costs one or two flushes of a small file, which is negligible for a file written only when a setting changes.
