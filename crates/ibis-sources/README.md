# Read-only library sources

`load_file` reads a BibTeX/BibLaTeX or CSL-JSON library into a `LoadedLibrary`:
its opaque `LibraryId`, immutable `Snapshot`, and fixed-code warnings with
1-based source lines. BibTeX native keys and CSL-JSON IDs use the `record:`
namespace; attachment keys use a separate namespace. Citation keys keep their
original spelling and provenance. CSL-JSON prefers `citation-key`, then `id`.

`load_file_snapshot` also returns the initial observation for
`SourceWatch::with_snapshot`. The watcher retains directory notifications,
rename-save and symlink handling, registration-gap recovery, coalescing and
capped retries. A replacement requires two identical reads separated by a
quiet interval. Errors leave installation of a replacement to the caller, which
keeps its last good snapshot. `reload_file` uses the same stable-read rule.
Initial BibTeX opens tolerate damaged entries; strict reloads reject unfinished
entries and exhausted work budgets. Stable empty files produce empty snapshots.

Files are limited to 64 MiB. BibTeX retains its existing field, recursion and
load-work budgets. CSL-JSON validates each record and known field independently;
a record above 1 MiB is skipped with a work-limit warning, and a limited reload
is refused. Unknown/vendor metadata is retained only in raw provenance. Raw
JSON tokens are skipped iteratively, so a deeply nested private `custom` field
cannot overflow a stack or hide adjacent records. The supported field vocabulary
follows the CSL 1.0.2 input schema.

`file` fields support literal paths and escaped JabRef-style
`description:path:media-type` entries separated by semicolons. Locations stay
private in the source owner; snapshots and diagnostics never contain them.
Availability checks inspect filesystem metadata without reading attachment
contents. Explicit network locations are left unknown without filesystem
access; HTTP(S) links are marked online and never fetched. Ambiguous encodings
produce an attachment warning rather than a guessed location. Known local
file URLs and vendor location fields are omitted from public CSL data.

The existing CLI and picker are cut over in #1199/#1201. These readers emit
`Record` directly and do not adapt the old `Reference` model.
