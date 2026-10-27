# Read-only library sources

`load_file` reads a BibTeX/BibLaTeX or CSL-JSON library into a `LoadedLibrary`:
its opaque `LibraryId`, immutable `Snapshot`, and fixed-code warnings with
1-based source lines. BibTeX native keys and CSL-JSON IDs use the `record:`
namespace; attachment keys use a separate namespace. Citation keys keep their
original spelling and provenance. CSL-JSON prefers `citation-key`, then `id`.
Numeric IDs use citeproc-js spelling; numbers beyond its safe integer range
require a string ID or explicit citation key to prevent identity collisions.
BibTeX arXiv eprints populate normalized search identifiers without changing CSL
fields or citation fingerprints.

`load_file_snapshot` also returns the initial observation for
`SourceWatch::with_snapshot`. The watcher retains directory notifications,
rename-save and symlink handling, registration-gap recovery, coalescing and
capped retries. A replacement requires two identical reads separated by a
quiet interval. Errors leave installation of a replacement to the caller, which
keeps its last good snapshot. Install a fresh owner and its warnings even when
public snapshot hashes are unchanged: private locations and raw provenance may
have changed. `reload_file` uses the same stable-read rule.
Initial BibTeX opens tolerate damaged entries; strict reloads reject unfinished
entries and exhausted work budgets. Stable empty files produce empty snapshots.

Opens recheck the descriptor type and size; on Unix they are nonblocking so a
regular file replaced by a FIFO cannot hang a reload. Files are limited to 64 MiB.
BibTeX retains its existing field, recursion and load-work budgets.
File libraries allow at most 200,000 entries in either format (BibTeX counts
all blocks, including string definitions and comments). Memory follows file
size more than record count; a 58 MiB file of 200,000 records loaded in about
2.6 GiB in a release build. Overflow keeps the accepted prefix with a work-limit warning;
strict reloads refuse it. BibTeX charges another 2 KiB of fixed work per block,
and both scanners retain only a bounded prefix of entry metadata.
CSL-JSON arrays share the same cap. CSL-JSON validates each record and known field independently;
record property names must be unique; an ambiguous record is skipped with a
syntax warning. A record above 1 MiB is skipped with a work-limit warning, and a
limited reload is refused. Unknown/vendor metadata is retained only in raw provenance. Raw
JSON tokens are skipped iteratively, so a deeply nested private `custom` field
cannot overflow a stack or hide adjacent records. The supported field vocabulary
follows the CSL 1.0.2 input schema.

`file` fields support literal paths and escaped JabRef-style
`description:path:media-type` entries separated by semicolons. Locations stay
private in the source owner; snapshots and diagnostics never contain them.
Availability checks inspect filesystem metadata without reading attachment
contents. Explicit network locations are left unknown without filesystem
access; HTTP(S) links are marked online and never fetched. Ambiguous encodings
produce an attachment warning rather than a guessed location. Windows UNC,
network/device namespaces (including mixed separators), drive-relative paths and
file URIs never trigger metadata probes. MIME values require bounded type/subtype
tokens. All adapters share one public URL policy, including citeproc URL overrides in
`note`: local file URLs, UNC paths, file-sharing schemes and bare document
filenames are omitted from public CSL data and retained only in private
provenance. Public URLs and bare domain names remain available for formatting.

Attachment fields are scanned lazily: at most 256 tokens and 64 KiB per record,
16 KiB per token and 4096 bytes per decoded/native location. A library permits
100,000 attachments, 200,000 tokens and 64 MiB of total attachment allocation
work, with 256 KiB of allocation work per record. This includes field bytes,
generated IDs, cloned parent references, possible private-map keys and native
path capacity, reserved before cloning or allocation. Long native IDs cannot
multiply into unbounded attachment keys. The count budget allows 100,000
ordinary single-attachment records. Exceeding a limit warns with
`work-limit` and preserves citable records; strict reloads refuse the replacement
so the caller retains its last good owner/snapshot.

The existing CLI and picker are cut over in #1199/#1201. These readers emit
`Record` directly and do not adapt the old `Reference` model.
