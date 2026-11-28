# Reference set, version 1

A reference set is the records a document cites, written as one file that
co-authors, build tools and agents can share. It serves as the document's lock
file, its snapshot of the references, and the manifest of a share pack
([0060](decisions/0060-one-document-pipeline.md)). This page is its contract:
what the file holds, how Ibis writes and reads it, and how a check compares a
library with it. Any tool may write or read a set by following it.

## The file

A set is a **CSL-JSON file**: a JSON array of CSL 1.0.2 items, UTF-8 without a
byte-order mark. Nothing else is in it: no wrapper object, no header and no
fields of Ibis's own. Pandoc, Quarto, citeproc-js, Zotero and Ibis read it as
it is. The usual name is `references.json`, next to the document.

```json
[
  {
    "id": "particle2026",
    "citation-key": "particle2026",
    "author": [
      {
        "dropping-particle": "de",
        "family": "Vries",
        "given": "Ada"
      }
    ],
    "issued": {
      "date-parts": [
        [
          2026
        ]
      ]
    },
    "publisher": "Synthetic Press",
    "title": "Invented <i>orchid</i> atlas",
    "type": "book"
  }
]
```

- **One record per work**, in the order the document first cites them; a set
  made from a selection keeps the selection's order.
- **`citation-key`** is the record's key: the library's own, or one Ibis
  minted for the project (0060). Every record has one: Ibis refuses to write
  a record without a key (`missing-key`).
- **`id`** equals `citation-key`, because Pandoc and citeproc cite by `id`.
  A library's own ids (Zotero item URIs, a file's `id`) never leave.
- **Fields** are the share-safe fields Ibis exports anywhere, the same as
  `ibis cite KEY --format csl-json`: CSL variables only, with rich text in
  CSL's HTML-like markup. A set never holds notes, annotations, attachments or
  their paths, library or account ids, the `file` field, local or private
  links, or Extra-field text that is not a CSL variable.

## How Ibis writes one

`ibis cite KEY… --format csl-json` writes a set, and so does exporting what a
manuscript cites. Ibis writes the same bytes for the same records, so a set in
a repository changes only when a reference does:

- Two-space indentation, one field per line and `\n` line ends, as
  `serde_json`'s pretty printer writes them.
- In each record `id` first, then `citation-key`, then the other fields in
  code-point order of their names. Nested objects keep the same order; arrays
  keep theirs.
- Text is NFC. JSON escapes only what JSON requires.

## How Ibis reads one

A set is an ordinary CSL-JSON library file (`--file references.json`), read
under the same rules and bounds as any other: unknown fields warn and are
ignored, private and local links are removed, `file` is dropped unread. Two
records with one `citation-key` make that key unavailable: Ibis never guesses
between them.

## Checking against a set

A check reads a manuscript, looks up each key it cites in the person's
libraries, and compares those records with the set by key:
`ibis check MANUSCRIPT --set references.json`, or the `check` method with
`set` ([CLI](cli.md#checking-a-manuscript)).

| Finding | When |
| --- | --- |
| `not-in-set` | The manuscript cites a key the set does not hold. |
| `changed` | The library's record for the key differs from the set's. |
| `twice-in-set` | The set holds the key twice, as a merge can leave it. |
| `uncompared` | Ibis could not write the set's record to compare it. |

Each is named once, at the key's first citation, beside what the manuscript
itself has wrong (a key no library holds, a damaged citation). A key the
library has renamed is compared under its current name. Any finding means the
check does not pass.

A record is **changed** when it differs after both sides pass through the
same writer of the running Ibis: the set's record is read and written again,
the library's record is written, and the two are compared without `id`. So a
newer Ibis that writes a field differently does not report every record as
changed, and the comparison needs no stored fingerprint.

## Checking quotations

A check can also look up each quotation that ends just before a citation,
such as `"the lanterns guide every ship" [@vale2024, p. 11]` or
``` ``…''~\cite[p.~11]{vale2024} ```, in the text of the cited reference's
attachments (decision 0055). The comparison is exact and repeatable: case,
line breaks, typographic quotes, apostrophes and dashes, and a word
hyphenated at a line break do not count, and an ellipsis stands for words
left out. Nothing is guessed or paraphrased.

| Finding | When |
| --- | --- |
| `quote-not-found` | No attachment of the reference holds the words. |
| `quote-on-other-page` | The words are there, but not on the cited page; the finding lists the pages they are on. |
| `quote-page-unknown` | The words are there, but the PDF has no printed page labels, so the cited page could not be checked. |
| `quote-unchecked` | The reference has no attachment whose text Ibis can read. |

A citation of several references passes when one of them holds the words on
its cited page. Pages are printed pages, from the PDF's own labels, never
the file's page numbers.

## Co-authors on different managers

The set is committed with the document. Each co-author checks the document
against it from their own library (Zotero, a `.bib`, EndNote, CSL-JSON):

- A co-author whose library lacks a work can open the set itself as a library
  and cite from it, so nobody needs the same manager.
- Keys are the shared names. A key minted by one co-author's project travels
  in the set; Ibis never renames a key and never infers a rename from matching
  titles or identifiers (ROADMAP › C).
- Two co-authors who changed the same record produce a JSON merge conflict
  inside one record, or, if both versions survive a merge, `twice-in-set`.

## What a set does not lock

Formatting inputs. A Markdown, LaTeX or Typst build names its own style file,
so the set locks records only.

## Versions

This is version 1. A set carries no version number while it has no fields of
Ibis's own. A later version that needs one adds it under CSL's `custom`
object of each record, which other CSL-JSON readers ignore, so a version-1
reader still reads it as plain CSL-JSON.

[`fixtures/reference-set/`](../fixtures/reference-set/README.md) holds a set
written by these rules, and a test that Ibis writes it byte for byte.
