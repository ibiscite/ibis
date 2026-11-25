# 0060 One document pipeline

Decided 2026-11-25 for "Your document" in
[#2035](https://github.com/ibiscite/ibis/issues/2035), applying
[0059](0059-works-with-nothing-better-with-more.md). It re-lands the closed
[#1991](https://github.com/ibiscite/ibis/pull/1991) (`ibis format`) and moves
the share pack of [0051](0051-references-in-and-out.md) onto the same path.
Every document is parsed in a child, under
[0055](0055-agent-layers-full-text-writes-remote.md) and
[0056](0056-open-layer-for-research-material.md).

## The pipeline

`scan → reference set → render → sink`, in one engine module,
`ibis-engine::document`. Check, export, format and share are compositions of
it, never operations of their own.

- **Scan** turns a document into citations in document order, each with its
  place (part, paragraph or line, column), keys, prefix, locator, suffix and
  form, plus typed problems (damaged, cut short). It also says what it could
  not see, so a check never reports a clean pass over text it skipped.
- **Reference set**: the cited records, one per key, in first-citation order.
  It is resolved from the open libraries, or from a set file given as the
  library.
- **Render**: the set and the citations in a context (style, locale, output)
  become an output: a records file, a formatted document, a pack.
- **Sink**: a new file (created, never replacing one), standard output, or the
  project's own `.bib` (append only).

## What is parsed where

Every document file is parsed only in a child (0056), whatever its format:

- `ibis __document` is a short-lived child run through
  `ibis_sources::child::run` (the runner, bounds and process group of the
  source adapters). It gets one framed request on standard input (the command,
  then the file's bytes) and no path or environment. It answers `scan` with
  the citations it read and their places, and for office files `write` with
  the new document.
- Its parsers sit behind a `documents` feature that only `ibis-cli` enables,
  as `endnote` is: the office readers in `ibis-sources::document::office`, and
  the manuscript readers in `ibis-core::syntax::manuscript`, which share the
  pasted-key readers. A test in `ibis-desktop` proves the feature is off in the
  app's own build, which passes its bundled `ibis` as the child, as it does for
  PDFs. The engine holds the parent side only and checks every answer again
  (counts, sizes, places, keys).
- The parent reads the file once under the local-file rules (a regular local
  file, never a link; at most 16 MiB of text or 64 MiB of office file) and
  gives the child those bytes.
- Text formats: Markdown, Quarto and R Markdown (Pandoc markers; fenced and
  inline code, comments and front matter skipped; Quarto and pandoc-crossref
  cross-references such as `@fig-map` are not citations), LaTeX (the citation
  commands of BibTeX, natbib and BibLaTeX; `%` comments and verbatim skipped),
  Typst (`@key` and `#cite(<key>)`; labels the document defines are not
  citations). Office documents read Pandoc markers, as #1991 did.
- One work allowance covers the whole text inside the child too, so a text
  read only in part is reported as such and never passes a check.

## The reference set

Inside the engine a reference set is the ordered selection of cited records,
each with its key when it has one and its source identity. Written out it is a
**CSL-JSON file**: share-safe fields (`CitableFields`), `citation-key` where
there is a key, no library ids, paths or account ids. A record with no key is
kept without `citation-key`: it reads and formats, and is never cited by key.
The file is the lock file, the project snapshot and the pack's manifest, and
every tool that reads CSL-JSON reads it: Ibis opens it as a library, Pandoc
and Quarto take it as `bibliography:`, Zotero imports it.

- **Check against a set** (`--set FILE`) compares by key, both sides through
  the same share-safe export: a cited key the set lacks, and a record that
  changed since the set was written. Fingerprints are derived, not stored.
- **Formatting inputs are not locked.** For Markdown, LaTeX and Typst builds
  the style is a file the project names and versions itself (`csl:`, a `.bst`,
  a Typst style); Ibis supplies records, so the set locks records. Only
  `ibis format` renders with Ibis's own styles: its report names the style,
  locale and Ibis version, and a receipt with their digests (0028) is added
  with it if co-authors need to reproduce a copy exactly.

## Outputs

- **Records**: BibTeX, BibLaTeX, CSL-JSON (the set) and Hayagriva YAML of what
  a document cites, keeping established keys, naming what was refused.
  Hayagriva is one new writer and one output row.
- **Static copy**: `ibis format` writes a new DOCX or ODT with every marker a
  citation and the list at `[bibliography]` or the end; re-run on the original
  to restyle. Note styles are refused.
- **Zotero-managed ODT copy** (`--for zotero`): the same copy, each citation a
  `ZOTERO_ITEM CSL_CITATION` reference mark with a fresh `citationID`, its
  `formattedCitation`, embedded `itemData` and one nonempty URI per work, and
  the document's `ZOTERO_PREF_*` in `meta.xml`, as the 2026-11-25 trial proved
  in LibreOffice with Zotero 10. The URI is the item's Zotero URI when Ibis
  knows the library's segment (`userID` when synced, else `localUserKey`; a
  group's `groupID`), remembered from its last closed read of that data folder;
  otherwise a stable document-local URI per work, so repeated citations stay
  one item and restyle from `itemData`, unlinked, and the report says so. Its
  qualification includes repeated citations through Refresh. Only in-text
  styles, only ODT; Word and note styles refuse with a typed reason and point
  to the static copy. The URI can carry the account's id, so this output is
  only ever an explicit choice.
- **Pack**: the share pack gains `References.json` (the set) beside its
  reading list and `.bib`, and takes a document as its selection. Nothing in it
  needs Ibis; Ibis opens it as a library.

## Keys and the project's `.bib`

- **Project-scoped keys.** An item with no key of its own (plain Zotero
  without a native `citationKey`, a CSL-JSON record without `citation-key`)
  gets one only inside a project. `write::citation_key` mints it against every
  key the project's `.bib`, its pins and the libraries hold. The pin records
  the item's library and item id, its stable identifiers (DOI, PMID, PMCID,
  ISBN, arXiv) and a digest of its title, year and first author, normalized. A
  pin is used only while both are exactly as recorded: the same identifiers,
  none added, gone or changed, and the same digest. Any change, including a
  corrected title or an item id reused for another work (a replaced CSL-JSON
  export), gives no key and a typed reason until the person asks for a new one; the old
  key stays reserved, never reassigned. Pins live in Ibis's data folder in one bounded file of their own
  (at most 20,000 pins and 4 MiB across projects, renamed and removed with the
  project), never in `settings.json` or anything shared, and are written
  before a minted key is emitted. The library never changes. Without a
  project, key outputs refuse as today, with the reason "choose a project".
- **Appending to the project's `.bib`** (0059): a project field
  `append_to_bibliography`, off by default. When on, an emitted key missing
  from the `.bib` adds that record's entry at the end, written by
  `write::bib` with the proposals' atomic write and backups; nothing else in
  the file changes. The answer carries an undo token; Undo restores the backup
  only if the file is still exactly what Ibis wrote. The engine owns the
  primitive; the cite-time trigger lands with Delivery.

## Rows

Each operation is one method row with the three fields of 0059, using the
existing values: `check` and `export` (needs `nothing`; a set file or `.bib`
is a library), `format` (needs `records`), `share` (as today, plus a
document, keeping its ⌘K selections of one or several references). All are
offline; `check`, `export` and `format` take a file, so they are not ⌘K
verbs. MCP offers `check` and
`export` read-only; `format` and the pack write files and stay with the
person's front ends.

## Rungs

| Capability | Rung 0: Ibis and files | Rung 1: + Zotero | Rung 2: + Better BibTeX or Scan |
| --- | --- | --- | --- |
| Check | against a `.bib`, CSL-JSON or set; `--locked` | live or closed Zotero; native and project keys | pinned Better BibTeX keys |
| Export cited | BibTeX, BibLaTeX, CSL-JSON, Hayagriva | the same from Zotero | keys as Better BibTeX pins them |
| Format | static DOCX/ODT, restyled by re-running | a Zotero-managed ODT copy | Scan markers remain the fallback route |
| Share | reading list, `.bib`, set, PDFs | imports into Zotero | the same |
| Keys | the file's own keys; minted per project | Zotero's native key | Better BibTeX keys |
| Append | a project's `.bib` | the same | the same |

## Not doing

No Ibis field code in Word or Google Docs (0059). No document parser in the
app's process. No footnotes, headers or text boxes in `format` yet; they
are listed as left unchanged. No writing of library records.
