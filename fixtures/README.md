# Fixture corpus

Synthetic inputs that reproduce real-world problems. Inspired by FFmpeg's FATE suite: every bug fix adds the smallest fixture that reproduces it, so the same problem can never return unnoticed.

Rules:

- **Synthetic only.** Never add records from a real library, real people's private data, or copyrighted abstracts. Invent names, titles and identifiers (use `10.0000/...` DOIs and `example.org` URLs).
- **One quirk per record** where possible, with a comment saying what it exercises.
- **Snapshots are the expected result.** `crates/ibis-core/tests/fixtures.rs` loads each file and compares against `crates/ibis-core/tests/snapshots/`. Review snapshot changes like code.

| Folder | Contents |
| --- | --- |
| `bibtex/` | `.bib` files: real-world quirks, damaged files, empty file |
| `updates/` | A synthetic update package signed with a throwaway test key, and a second unrelated public key (neither private half was kept), for the offline update check test |
| `zotero/` | Synthetic Zotero local-API item envelopes (`key`, `links`, `data`, `csljson`); database snapshots for Zotero 7–10, including trashed parents and children, are still planned |

`bibtex/oversized-range-year.bib` preserves the minimized synthetic fuzz input
from #111. The robustness tests also exercise invalid zero months in uncertain
dates: malformed dates produce field warnings without discarding the reference.

`bibtex/malformed-date-parts.bib` contains separate year/month/day inputs
from #113: zero months, an oversized day embedded in a month field, and a
non-Latin numeric month. Unsafe parts produce field warnings and retain
a valid year rather than discarding the whole reference.

`bibtex/deep-commands.bib` has 4,000 nested emphasis commands in one synthetic
title, plus safe sibling fields and a second reference. Core and CLI tests
check that loading on a 2 MiB stack omits the title with a warning and retains
the other data; dedicated tests also cover the 256-level accepted boundary.

`bibtex/unbalanced-values.bib` covers #128 with quoted and braced values in
both entry delimiters, safe sibling fields and neighboring references.
Value-structure tests verify explicit warnings, omission of unsafe fields,
literal escaped braces, and CSL export without invented author/date metadata.

`bibtex/fuzz-signed-year.bib` is the exact 25-byte synthetic reproducer from
#155. Date-guard tests require its reference to survive with field warnings
instead of a caught parser panic, in both debug and release builds.

`zotero/attachments.json` (#573) holds one parent item per attachment case:
a stored PDF, a saved web page (`text/html`), an XHTML page, a linked URL
only (Zotero reports no best attachment), an EPUB, a parent without children,
a PDF with MIME parameters and odd case, and a child attachment item. Each
`_fixture` field says what the item exercises. `zotero/attachments-hostile.json`
has malformed `links` values (numbers, arrays, oversized or empty MIME types,
links as text) and sentinel private values (href, key, title, path, URL,
size) that must never appear outside `raw`. `crates/ibis-core/tests/zotero_attachments.rs`
loads both in the object and the local-API text CSL form.
