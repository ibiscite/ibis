# Fixture corpus

Synthetic inputs that reproduce real-world problems. Inspired by FFmpeg's FATE suite: every bug fix adds the smallest fixture that reproduces it, so the same problem can never return unnoticed.

Rules:

- **Synthetic only.** Never add records from a real library, real people's private data, or copyrighted abstracts. Invent names, titles and identifiers (use `10.0000/...` DOIs and `example.org` URLs).
- **One quirk per record** where possible, with a comment saying what it exercises.
- **Snapshots are the expected result.** `crates/ibis-core/tests/fixtures.rs` loads each file and compares against `crates/ibis-core/tests/snapshots/`. Review snapshot changes like code.

| Folder | Contents |
| --- | --- |
| `bibtex/` | `.bib` files: real-world quirks, damaged files, empty file |
| `zotero/` | (planned) synthetic Zotero local-API pages and database snapshots for Zotero 7–10, including trashed parents and children |

`bibtex/oversized-range-year.bib` preserves the minimized synthetic fuzz input
from #111. The robustness tests also exercise invalid zero months in uncertain
dates: malformed dates produce field warnings without discarding the reference.

`bibtex/malformed-date-parts.bib` contains separate year/month/day inputs
from #113: zero months, an oversized day embedded in a month field, and a
non-Latin numeric month. Unsafe parts produce field warnings and retain
a valid year rather than discarding the whole reference.
