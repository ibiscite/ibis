# Closed-reader synthetic databases

These SQL files generate minimal invented databases during tests. They contain no
library captures. The common physical-table subset is checked against Zotero's
AGPL-3.0-or-later userdata schema for [7.0.0](https://github.com/zotero/zotero/blob/7.0.0/resource/schema/userdata.sql),
[8.0](https://github.com/zotero/zotero/blob/8.0/resource/schema/userdata.sql),
[9.0](https://github.com/zotero/zotero/blob/9.0/resource/schema/userdata.sql) and
[10.0.6](https://github.com/zotero/zotero/blob/10.0.6/resource/schema/userdata.sql).
The tested userdata range is 123–131. The tagged [10.0.9 schema](https://github.com/zotero/zotero/blob/10.0.9/resource/schema/userdata.sql)
was compared with 10.0.6 on 2026-11-11: only the version header changes (130 to 131).
[Migration 131](https://github.com/zotero/zotero/blob/10.0.9/chrome/content/zotero/xpcom/schema.js#L3630)
repairs incomplete earlier migrations; it adds no new reader dependency.
Newer userdata versions read with `zotero-newer-than-tested` when the required
physical tables, ordinary columns, affinities and lookup primary keys validate.
An incompatible newer layout refuses with the existing TooNew wording and a fixed
table name in JSON detail. Schemas below 123 still refuse as TooOld.
The compatibility warning appears in library status and doctor, without turning
searches or citations into source findings.
It does not infer a version from a filename or installed application.

Multipart date decoding follows Zotero utilities' `multipartToStr` at commit
[4051881](https://github.com/zotero/utilities/blob/4051881d59c6c12d34417d4c226f37218051be90/date.js).
Stored strings and exact numeric storage values remain in engine-private provenance;
only typed text reaches mapping. The value column is deliberately untyped, matching
Zotero. Numeric values follow Zotero's JavaScript Number coercion via `ryu-js`.

The private reader rejects hot rollback journals without interpreting any
super-journal pathname. Empty or zero-header copied journals are discarded before
SQLite opens; the source journal is untouched. Logical page claims in the main
header and WAL frames are bounded before SQLite can allocate integrity-check
structures; queries stream without automatic indexes or unbounded sorts.

Copy tests create WALs, journals, source mutations and process holders using only
these disposable databases. No test discovers a user profile or installed Zotero.

Better BibTeX's `better-bibtex.sqlite` (with its WAL and journal) and
`better-bibtex/read-only.json` are copied and checked with Zotero's files. Key tests
build invented stores beside the synthetic databases. The store is read only when
`settings` records Zotero 7 as the last to run (Better BibTeX 8 renames it after
migrating its keys into Zotero's own field). A store or key file that cannot be
read leaves the snapshot's keys unknown; it never fails the library read.
