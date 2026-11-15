# 0051 References in and out

Decided by the maintainer on 2026-11-13, after a design study of Zotero, JabRef,
BibDesk, Paperpile, Bookends, citation.js and the doi.org, Crossref and OpenAlex
services. It is the first step of the wider aim in ROADMAP.md (Where Ibis is going).

## Lasting concepts, each with one owner

- **Identity across sources** (`ibis-core::contract`): every record says where
  it came from (a library, a lookup, or supplied by the caller). An identifier
  an open library already holds is never looked up.
- **Identifiers** (`ibis-core::identifier`): DOI, ISBN, arXiv, PMID and PMCID,
  read locally from pasted text. A URL is only a form an identifier arrives in.
- **Lookup** (`ibis-engine::lookup`): the only network client, under decision 0008.
- **Selection → format → destination**: the engine's output catalogue builds what
  is shared, always through the share-safe filter, so notes, private links and
  files never leave by accident; the front ends own the system's destinations.
- **Writes** (`ibis-engine::write`): agent proposals the person confirms in
  Ibis, applied to `.bib` files ([0055](0055-agent-layers-full-text-writes-remote.md)).
- **Workflows**: command-line pipes and saved recipes later reuse the same path.

## Order

1. **Share a selection** first, fully offline: copy DOI links or BibTeX, or save a
   `.bib` or CSL-JSON file. It brings a BibTeX writer built from CSL-JSON with
   Ibis's own escaping.
2. **Paste a DOI to cite** next, the first network feature, with an independent
   review before it merges.
3. **Add by identifier** last, through Zotero.

## Offline sharing

The first engine slice adds `bibtex` and `doi-links` to the output catalogue,
through the existing `cite --format`. All record exports, including CSL-JSON,
require `citable::CitableFields`: bibliographic note overrides survive as fields;
notes, attachments, device paths and private links do not. BibTeX is generated
from these fields with literal escaping, never reused from an imported file.
Missing or unsafe citation keys refuse that record. DOI links need no citation
key, preserve identifier spelling and URL-encode reserved characters; a missing
or unusable DOI is a named per-record refusal. The CLI prints usable records
with a findings exit status and names refusals on stderr. Repeated selections
export each record once. MCP continues to exclude record-sharing formats.
The shared public-URL constructor parses hosts before checking them, so numeric,
encoded and internationalized spellings cannot bypass local-address checks.
CSL-JSON preserves only plain source-id tokens (letters, digits, `_`, `-`);
other source ids use the citation key. Device paths after value delimiters stay
private too, as do paths of two or more segments after any other mark that opens a word or is a dash. BibTeX identifier fields are verbatim, with URL braces percent-encoded.
For articles, issue takes precedence over number; malformed structured dates
are omitted rather than exported as empty or partial dates.
Core decodes entities and normalizes inline text before privacy checks, keeping
literal text separate from supported rich-text tags. Writers and the formatter
consume that checked projection. CSL-JSON text is what CSL processors print:
only literal tags and ampersands that start a character reference are escaped.
Span attributes are checked and discarded. Approximate structured dates keep
their BibLaTeX EDTF `~` marker; the date model has no uncertainty flag.

## Writes

Zotero only through Zotero 10's official local write API once Zotero 10 is
stable, never the Connector endpoints or `zotero.sqlite`, and only to the library
the person chose. `.bib` files come first: amended on 2026-11-15 with
[0055](0055-agent-layers-full-text-writes-remote.md), Ibis generates citation
keys (`authoryearword`, lowercase ASCII, with `a`, `b`, … for one already used)
and writes a `.bib` file when the person applies a proposal. Every write shows a
preview and waits for the person's click; Ibis refuses if the target changed,
edits only the entry it changes, writes atomically, keeps a backup and offers
Undo.

## Attachments

Sharing may include attached files only when the person ticks "Include
attachments" for that share; the default is metadata only. Attachments are
private files, so this option gets its own design and an independent review
before it ships.

## Agents and the command line

MCP never gets lookup or write tools. Amended on 2026-11-15 with
[0055](0055-agent-layers-full-text-writes-remote.md): `ibis mcp --allow-writes`
adds tools that only propose a change, kept for the person, who applies or
rejects it in the Ibis app; a client's own approval never counts. `ibis lookup`
exists only while the person has turned lookup on. The command line lists,
shows and rejects proposals with `ibis proposals`, and cannot apply them.
Lookup, proposals and apply are kept out of the engine methods the command line
generates automatically. An agent with a shell can still change settings or run
`ibis`, and PRIVACY.md says so when lookup ships.
