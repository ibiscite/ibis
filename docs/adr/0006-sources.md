# 0006. Read-only sources; Zotero access ladder

Status: accepted (2026-10-17)

Verified against Zotero 10.0.6: the live database is locked while Zotero runs; a copy including the WAL files opens consistently; the local API answers in milliseconds and rejects requests carrying a browser Origin header.

## Decision
- 1.0 never writes to any library. Later fixes go only through official APIs after the user reviews each change.
- BibTeX/BibLaTeX: each entry parsed independently; broken entries, duplicate keys and undefined macros become warnings with line numbers.
- Zotero: (1) local API when Zotero runs and the user enabled it; (2) read-only snapshot of `zotero.sqlite` + `-wal` + `-shm` when it is closed, through a schema-versioned adapter tested on Zotero 7/8/9/10 fixtures, excluding trashed items and children of trashed parents; (3) Better BibTeX exports. Citation keys: native field, then Extra, then BBT.
- Local API requests send no browser `Origin`, bypass proxies, verify `Zotero-API-Version`, and fail closed if the library changes mid-scan (lessons from NoteroPDF).

## Consequences
Works with Zotero closed, at the cost of tracking Zotero schema changes several times a year; CI fixtures make breakage visible before users see it.
