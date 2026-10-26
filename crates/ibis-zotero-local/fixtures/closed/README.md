# Closed-reader synthetic databases

These SQL files generate minimal invented databases during tests. They contain no
library captures. The common physical-table subset is checked against Zotero's
AGPL-3.0-or-later userdata schema for [7.0.0](https://github.com/zotero/zotero/blob/7.0.0/resource/schema/userdata.sql),
[8.0](https://github.com/zotero/zotero/blob/8.0/resource/schema/userdata.sql),
[9.0](https://github.com/zotero/zotero/blob/9.0/resource/schema/userdata.sql) and
[10.0.6](https://github.com/zotero/zotero/blob/10.0.6/resource/schema/userdata.sql).
The transport reads supported userdata versions 123–130 and refuses newer layouts.
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
