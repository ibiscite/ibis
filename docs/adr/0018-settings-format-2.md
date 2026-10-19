# 0018. Settings format 2: the library names its kind

Status: accepted (2026-10-19); supersedes the `library` field and migration rule of 0016

## Context

ADR 0016 fixed `settings.json` format 1, where `library` is a bare path to a BibTeX/BibLaTeX file. The maintainer agreed in #80 to read Zotero directly (#13), so the library Ibis opens can now be a file or a Zotero library. A bare path cannot say which. ADR 0016 deferred the first migration to #2 and requires a superseding ADR for any change to the format.

## Decision

Write `schema_version: 2`. Everything in ADR 0016 still holds — location, atomic saving with `settings.json.bak`, backup recovery, refusing newer files, ignoring unknown fields — except:

| Field | Format 2 value |
| --- | --- |
| `library` | `{ "bibtex": "/path/refs.bib" }`, `{ "zotero": "user" }` (My Library) or `{ "zotero": { "group": 42 } }` (a group library, by its numeric ID) |

**Migration.** Reading checks the version first, as before. A file of format 1, with no `schema_version`, or with `schema_version: 0` is converted by a pure function `v1_to_v2`: a string `library` becomes `{ "bibtex": <path> }`, and every other field is unchanged. Each later format adds one such step, applied in order, with a test per step. A file is converted in memory when read and written in the new format only on the next save. That save keeps the format 1 file as `settings.json.bak` until the save after it, which replaces the backup as usual. An older Ibis meeting a format 2 file reports it as written by a newer version, leaves it unchanged and uses its backup, as ADR 0016 already specifies.

One library at a time remains the model. Several libraries open together is a later format step, not part of this one.

## Consequences

Zotero can be chosen and remembered without guessing what a path means. Going back to an older Ibis right after the first save in format 2 falls back to the format 1 backup; after a second save it finds no file it can read and starts with defaults, leaving both files unchanged. Downgrades are not supported beyond that. Hand-edited files must use the new `library` shape; the old bare path still works because it is migrated.
