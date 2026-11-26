# 0061 A folder of PDFs is a library, and one `resolve`

**The maintainer's decision**, 2026-11-25, in the roadmap review
([#2035](https://github.com/ibiscite/ibis/issues/2035)): someone with no
reference manager cites the PDFs in a folder directly. Ibis reads each PDF's
own metadata and DOI locally, asks before any online lookup, and keeps what it
found in a small `.bib` of its own that the person can see and take elsewhere.
Passages are searchable whatever is known; a citation needs known details, and
an unknown detail is shown as unknown, never invented. Online lookup is the DOI
lookup the maintainer approved on 2026-11-18 ([#1300](https://github.com/ibiscite/ibis/issues/1300))
under [0008](0008-offline-by-default.md), re-landed as one tier of a single
`resolve` operation.

Everything below is the engineering design that carries it out.

## The folder source

- **A folder is a BibTeX library Ibis keeps.** `--library DIR` (a folder where
  a file names a library file) and the app's Choose Folder open a library of
  kind `folder`. Its records are `DIR/ibis-references.bib` with relative `file`
  paths, read by the existing BibTeX reader, so keys, attachments, passages and
  every output work as for any `.bib`, and Zotero, JabRef or Overleaf import it
  with its PDFs. It declares the BibTeX reader's capabilities except proposals.
- **Reads never write, and read only the file.** Every read (the app, the
  command line, `serve`, `mcp`, `sources check`) reads `ibis-references.bib`
  and nothing else, so conformance holds and keys never move. A read also
  counts the PDFs no entry's `file` names and warns `pdfs-not-kept` with that
  number and the Keep recovery. A PDF becomes a record only once kept.
- **Keeping is an explicit act.** Choosing the folder in the app (whose dialog
  says Ibis keeps `ibis-references.bib` there), and the app's later refreshes
  of it, and `ibis sources keep --library DIR`, read each unkept PDF in the
  child (below) and append one entry per PDF: only what it found, a key from
  0051's generator over the file's keys and the batch's (with no author, year
  or title, from the file name's words), and its relative `file`. The append
  goes through the proposal writer (`write`: the cross-process writer lock,
  refusal if the file changed since it was read, atomic replace, the last five
  versions in Ibis's data folder), 100 PDFs per write. Ibis's own write wakes
  the file watch, which rereads as for any edit; in the app opening shows the
  file at once and each kept batch publishes as a refresh.
- **Only settled reads are kept.** A PDF the child read, or found damaged, not
  a PDF or too large, gets its entry (a damaged one only its `file`, so its
  passages stay listed and it is not read again). A PDF that timed out, was
  missing, online-only or locked, or met an unavailable child is left unkept
  and tried at the next keep.
- **Appending is the only automatic change.** Ibis never edits or removes an
  entry on its own and never touches a PDF, so a person's edits win and an
  entry whose PDF has gone stays with its attachment missing. Keeping in a
  folder Ibis cannot write refuses with `folder-read-only`; the folder still
  reads whatever `ibis-references.bib` holds.
- **Bounds.** Subfolders to depth 4; links, hidden folders and cloud
  placeholders skipped and counted; at most 5,000 PDFs listed; each within
  `pdf::MAX_FILE_BYTES`; at most four children at once, each under the PDF
  child's deadline and memory ceiling.

## What a PDF says

- **In the child only.** `ibis __extract-pdf --details` answers the Info
  dictionary's Title, Author and `/doi`; the XMP packet's `dc:title`,
  `dc:creator`, `dc:publisher`, `dc:identifier`, `prism:doi`, `pdfx:doi`,
  `crossmark:DOI`, `prism:publicationName`, `prism:volume`, `prism:number`,
  `prism:startingPage`, `prism:endingPage` and `prism:coverDate` or
  `prism:publicationDate`; and the text of the first two pages. The parser is
  its own file and gets a fuzz target in the threat model; the parent
  (`ibis_sources::pdf`) bounds every string and checks the answer first.
- **Kept as the PDF states it,** dropping only known noise: a title that
  names a file or a template (`Microsoft Word - …`, `untitled`), and an author that is a program or an account name (`jsmith`). A
  short title or a one-word name is kept. Dates come only from the
  publication fields, never the file's dates. With no journal the type is
  `document`.
- **The DOI** is one the metadata names: `prism:doi`, `pdfx:doi`,
  `crossmark:DOI`, an Info `/doi`, or a DOI in `dc:identifier`. The only
  distinct DOI printed on the first page is a **candidate**, never written as
  the record's DOI: it may be a cited work's. It is used to suggest library
  matches and to look up details, which the person confirms.

## Known details

A record with no title, no author and no editor is refused for formatted
citations, from any source: the shared eligibility check
(`ibis_core::citation::check`) refuses styled outputs with `missing-details`,
recovery Review Library. Key outputs
keep their existing citation-key validation; record exports (BibTeX, CSL-JSON,
DOI links) carry what a record has and do not ask. Every
style renders such a record as a bare date or nothing at all, so this changes
what was never a usable citation; it is one rule rather than a branch on the
source. A folder's PDF that said nothing is therefore listed and its passages
stay searchable and readable, but it is cited only once its entry has a title,
author or editor. Anything else a style needs and a record lacks renders as
the style renders a missing field. Previews of a folder record show title,
author, date and container as unknown when absent.

## One `resolve`

`resolve(identifier | text | file) → candidates` answers "is this in my
library", a held DOI and a dropped PDF. It is one method-table row whose
`network` names `doi.org`, `api.crossref.org` and `data.crosscite.org`.

1. **Library.** Identifiers (DOI, ISBN, arXiv, PMID, PMCID) are read locally
   and matched against every open library's records; text matches by title
   words through search. Each candidate names its item and how it matched.
2. **File.** A PDF's details are read in the child; a folder entry for that
   same file matches first, then its DOI and title go through tier 1.
3. **Network**, DOIs only, through `ibis-engine::lookup` from #1984 and its
   review fixes, under 0008: only for DOIs tier 1 did not find, with every
   open library read fully, `online_lookup` on and DOI lookup confirmed.
   Otherwise the answer says a lookup is possible and exactly what it would
   send where. The one exception is **Look Up Details** below: a DOI held only
   by folder records that lack details, or a first-page candidate, may be
   looked up on the person's click. This amends 0051's "an identifier an open
   library already holds is never looked up" for that case alone.

**Consent is not a method.** `online_lookup` and `doi_lookup_confirmed` are
written only by a front end that asked a person: the app's own dialog and
settings screen, and the command line's terminal prompt. No method-table row
sets them, so MCP and `serve` cannot. MCP's `resolve` has no network tier
([0051](0051-references-in-and-out.md)). `ibis resolve` replaces the planned
`ibis lookup`.

**Looked-up details are kept only on a click.** A looked-up record is cited
through the records path and kept nowhere, except for a folder record: **Look
Up Details** takes one record, or every folder record missing details that
has a DOI or a candidate, at most 50 per action under one ask, and shows the
found fields beside the PDF's own and its first page; **Use These Details**
writes the chosen ones, a confirmed candidate DOI among them, into those
entries through the proposal writer, with Undo. This is the one edit Ibis
makes to an existing entry, and PRIVACY.md says so.

## Where it lives

| Part | Home |
| --- | --- |
| Details parser (untrusted) | the PDF child's own file, moving with the text extractor into `ibis-sources`' content child |
| Details answer and its checks, listing, entry text | `ibis_sources::pdf` and `ibis_sources::folder` |
| `Source::Folder`, keeping, `missing-details`, `resolve`, lookup | `ibis-engine` |
| Choose Folder, unknowns, Look Up Details | the app, with the libraries family ([0057](0057-one-method-table.md) step 10) |

The app parses PDFs only through its bundled `ibis` as the child, as the
document pipeline will; until then a folder opens in the command line, `serve`
and `mcp`. The `network` field takes a named-hosts value beside `offline`.

## Order

1. This record.
2. The details parser and its checks, with an independent review.
3. The folder source, `ibis sources keep`, `pdfs-not-kept`,
   `missing-details` and the conformance suite over invented PDFs.
4. `resolve` with its network tier, after the method table's row fields land,
   with an independent review; PRIVACY.md, README and the website change with it.
5. The app: Choose Folder, unknowns, Look Up Details.

## Not doing

No title guessed from text layout or a file name, no OCR, no lookup by title
words, no change to a PDF, no store beside the `.bib`, and no renamed-file
matching by content before people ask for it.
