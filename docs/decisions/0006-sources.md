# 0006 Sources

Library readers are read-only. BibTeX and BibLaTeX are parsed entry by entry;
broken entries, duplicate keys and undefined macros produce warnings with line
numbers. Every entry with a repeated key is unavailable, including as an
inheritance parent, because Ibis cannot know which one was meant. Repeated
CSL `id` values likewise refuse every affected record. A CSL `id` is not a
citation key. File libraries are parsed fresh.

Library IDs are opaque tokens keyed by a private local secret. They reveal
neither the source kind nor a guessed path. The command line may create that
identity key and save explicitly requested Ibis settings, never source data.

Running Zotero is read through its local API; closed Zotero through a private
copy of its database and journal. Readers support Zotero 7–10 and exclude
trashed items. Native, Extra and Better BibTeX citation keys must agree for key
output. Local API requests bypass proxies, send no browser `Origin` and check
`Zotero-API-Version`. Windows process, drive and attribute checks use
Win32 directly so they also work under AppLocker and Constrained Language Mode.
Native boundaries are in [0029](0029-native-calls.md).

## Closed Zotero cache (5A)

Decided 2026-11-16 by the maintainer: closed reads reuse **one private per-user
cache copy**, refreshed only when Zotero's files change. The source adapter's
path owner supplies the shared desktop/CLI app cache directory; the reader
does not choose OS paths. Switching profiles replaces this same cache set.

The adjacent versioned JSON stamp records a digest of the source directory,
each file's size, nanosecond modification time and SHA-256 of its first/last
64 KiB, and absent or skipped Better BibTeX files. Zotero's database, WAL,
journal and SHM activity evidence and the Better BibTeX files share one stamp.
An unchanged stamp reuses the set after another quiet-moment recheck; a missing
or invalid cache triggers acquisition. Refresh retains the source bounds,
full byte comparison and three-attempt limit. SQLite receives cache paths only.

All files use `ibis_core::atomic_file` private creation. Refresh stages and
fsyncs the files beside their destinations, removes the old stamp before
renaming the complete set, then atomically publishes a flushed stamp last.
One lease excludes other readers throughout refresh and query. An interrupted
stage leaves the previous cache; an interrupted publication leaves no committed
cache. Numbered temporary files are cleaned under the lease and never queried.
Unix directories are 0700 and files 0600. The shared creator inherits per-user
cache security on Windows; it supplies no separate ACL or ownership repair.

A writing's optional `.bib` is read only to check emitted keys, never searched
or edited. It is checked when chosen, at launch and when the picker opens,
without polling. Only its filename, key states and line numbers reach the
webview. Copy Entry sends the original library entry directly from native code
to the clipboard, including its `file` and `note` fields; this explicit local
copy is described in [PRIVACY.md](../../PRIVACY.md). Sharing instead uses the
field filter in [0051](0051-references-in-and-out.md).
