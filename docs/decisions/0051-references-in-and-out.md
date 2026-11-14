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
- **Writes** (`ibis-engine::write`): nothing until a slice needs it.
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
The shared public-URL check rejects ambiguous numeric, encoded and non-ASCII
hosts; internationalized public domains can use their ASCII punycode form.
CSL-JSON preserves only plain source-id tokens (letters, digits, `_`, `-`);
other source ids use the citation key. Device paths after value delimiters stay
private too. BibTeX identifier fields are verbatim, with URL braces percent-encoded.
For articles, issue takes precedence over number; malformed structured dates
are omitted rather than exported as empty or partial dates.

## Writes

Zotero first, only through Zotero 10's official local write API once Zotero 10 is
stable, never the Connector endpoints, and only to the library the person chose.
Writing to `.bib` files comes later, once Ibis can generate good citation keys.
Every write shows a preview and waits for the person's click; Ibis refuses if the
target changed, writes atomically, keeps a backup and offers Undo.

## Attachments

Sharing may include attached files only when the person ticks "Include
attachments" for that share; the default is metadata only. Attachments are
private files, so this option gets its own design and an independent review
before it ships.

## Agents and the command line

MCP never gets lookup or write tools. `ibis lookup` exists only while the person
has turned lookup on. Writes stay out of the command line until they are decided
for it. Lookup and write are kept out of the engine methods the command line
generates automatically. An agent with a shell can still change settings or run
`ibis`, and PRIVACY.md says so when lookup ships.
