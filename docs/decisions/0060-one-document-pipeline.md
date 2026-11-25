# 0060 One document pipeline

Decided 2026-11-25 for "Your document" in
[#2035](https://github.com/ibiscite/ibis/issues/2035), applying
[0059](0059-works-with-nothing-better-with-more.md). It re-lands the closed
[#1991](https://github.com/ibiscite/ibis/pull/1991) (`ibis format`) and moves
the share pack of [0051](0051-references-in-and-out.md) onto the same path.
Parsing stays under [0055](0055-agent-layers-full-text-writes-remote.md) and
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

The line is the one #1991 drew, made a rule:

- **Containers run in the child.** DOCX and ODT are zips of XML from anywhere,
  so the office reader runs only in `ibis __document`, a short-lived child
  through `ibis_sources::child::run` (the same runner, bounds and process
  group as the source adapters). It gets one framed request on standard input
  and no path or environment. Its parsers live in `ibis-sources::office`
  behind a `documents` feature that only `ibis-cli` enables, as `endnote` is.
  The engine holds the parent side only. The app passes its bundled `ibis` as
  the child, as it does for PDFs. A test in `ibis-desktop` proves the feature
  is off in the app's own build. Answers are untrusted and checked again.
- **Plain text is read in-process**, by the readers in
  `ibis-core::syntax` that already read pasted text in the app: linear,
  bounded by the shared work allowance, with no decompression and no XML. The text is the file's bytes
  as UTF-8, at most 16 MiB. A child adds a process and gains nothing over
  these. The same readers read the paragraphs the office child returns.
- Text formats: Markdown and Quarto (Pandoc markers; fenced and inline code,
  comments and front matter skipped; Quarto's `@fig-`, `@tbl-`, `@sec-`,
  `@eq-` cross-references are not citations), LaTeX (the citation commands of
  the capabilities table, `%` comments skipped), Typst (`@key` and
  `#cite(<key>)`; labels the document defines are not citations). Office
  documents read Pandoc markers, as #1991 did.

## The reference set

A reference set is a **CSL-JSON file**: an array of records, `id` and
`citation-key` both the key, the fields through the share-safe filter
(`CitableFields`), nothing else. It is the lock file, the project snapshot and
the pack's manifest, and every tool that reads CSL-JSON reads it: Ibis opens
it as a library, Pandoc and Quarto take it as `bibliography:`, Zotero imports
it.

- **Fingerprints are derived, not stored**: the canonical form of a record's
  share-safe fields. `--locked` compares by key: a key cited but not in the
  set, a key in the set the library no longer has, and a record whose
  fingerprint changed. Nothing ties a key to a library item, so a set carries
  no library ids, paths or account ids.
- **Formatting inputs are not in the set.** The style and locale come from the
  document (front matter `csl:` and `lang:`), the project, or the request, as
  they do today. Bundled styles change only with an Ibis release; a set
  promises the same records, not a byte-identical rendering (#1386, 0028).

## Outputs

- **Records**: BibTeX, BibLaTeX, CSL-JSON (the set) and Hayagriva YAML of what
  a document cites, keeping established keys, naming what was refused.
  Hayagriva is one new writer and one output row.
- **Static copy**: `ibis format` writes a new DOCX or ODT with every marker a
  citation and the list at `[bibliography]` or the end; re-run on the original
  to restyle. Note styles are refused.
- **Zotero-managed ODT copy** (`--for zotero`): the same copy, each citation a
  `ZOTERO_ITEM CSL_CITATION` reference mark with a fresh `citationID`, its
  `formattedCitation`, embedded `itemData` and the item's URI, and the
  document's `ZOTERO_PREF_*` in `meta.xml`, as the 2026-11-25 trial proved in
  LibreOffice with Zotero 10. The URI's library segment comes from the closed
  Zotero reader's private copy of the database (`userID` when synced, else
  `localUserKey`; a group's `groupID`). Without it the citations still restyle
  from their `itemData`, unlinked, and the report says so. Only in-text styles,
  only ODT; Word and note styles refuse with a typed reason and point to the
  static copy. The URI carries the account's id, so this output is only ever
  an explicit choice.
- **Pack**: the share pack gains `References.json` (the set) beside its
  reading list and `.bib`, and takes a document as its selection. Nothing in it
  needs Ibis; Ibis opens it as a library.

## Keys and the project's `.bib`

- **Project-scoped keys.** An item with no key of its own (plain Zotero
  without a native `citationKey`, a CSL-JSON record without `citation-key`)
  gets one only inside a project: `write::citation_key` mints it against every
  key the project's `.bib` and libraries hold, and the project pins it to the
  item's library and item id in `settings.json` beside the project's
  definition, renamed and removed with it, at most 10,000 per project. The
  library never changes; a pin whose item is gone is unused, never moved.
  Without a project, key outputs refuse as today, with the reason "choose a
  project".
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
document). All are offline and none is a ⌘K verb. MCP offers `check` and
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

No Ibis field code in Word or Google Docs (0059). No in-process zip or XML
parser in the app. No footnotes, headers or text boxes in `format` yet; they
are listed as left unchanged. No writing of library records.
