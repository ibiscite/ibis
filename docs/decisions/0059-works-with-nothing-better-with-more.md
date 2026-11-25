# 0059 Works with nothing, better with more

Decided by the maintainer on 2026-11-25 in the roadmap review
([#2035](https://github.com/ibiscite/ibis/issues/2035)). It builds on
[0028](0028-engine-first.md), [0056](0056-open-layer-for-research-material.md)
and [0057](0057-one-method-table.md), adds three fields to 0057's rows, and
moves the reader of [0036](0036-reader-trust-boundary.md) into its own window.

## The principle

Every capability is useful to someone who has only Ibis and their files: a
`.bib`, a CSL-JSON or RIS export, or a folder of PDFs. It gets better by itself
when Zotero, Better BibTeX or Zotero's Scan plugin is present and wanted. The
aim is not every feature for everyone; it is the best possible result at each
level, so nobody installs and sets up several tools to do one small thing.

- **Rungs.** Rung 0 is Ibis with files; rung 1 adds Zotero; rung 2 adds Better
  BibTeX or the Scan plugin. Each capability on the roadmap states its result
  at each rung, and a feature without a sensible rung-0 result needs one before
  it is planned.
- **Detected, never asked.** Ibis learns the rung from what a read returned
  (keys, attachments, a text cache), discovery, a probe of a local endpoint or
  system interface, and target facts at summon, and reads them again on
  refresh.
- **One path for every rung.** Sources, outputs and targets declare what they
  offer; methods and outputs declare what they need. The engine intersects them.
  A verb whose needs are unmet is not offered, or refuses with a typed reason
  and the next-best verb. No code branches on a product name.

## Five nouns

- **Source:** a snapshot of a library with declared capabilities.
- **Selection:** ordered references, each a library item or a supplied record,
  with an optional Anchor, prefix, suffix and form. One citation is the
  smallest Selection.
- **Anchor:** a place inside content: a physical page, a printed page label or
  a text quote. A citation locator is a projection of an Anchor, so the engine
  has one page vocabulary.
- **Context:** optional facts (app, site, document or project, explicit choice)
  that resolve to a format, style, locale and bibliography. Every fact may be
  missing; a site is a fact about the target, not a system.
- **Output and Sink:** what is written (described by one output descriptor:
  unit, carrier and needs) and where it goes: clipboard and paste, an editor's
  reply, a file, standard output, a drag or an open.

## Five operations

- `find`: search, resolve an identifier, text or file to candidates, and look
  up online as `resolve`'s network tier under [0008](0008-offline-by-default.md).
- `locate`: outline, read and find inside content, returning Anchors.
- `render`: a Selection in a Context becomes an Output.
- `scan`: a document becomes Selections and problems.
- `propose`: a change the person confirms in Ibis
  ([0055](0055-agent-layers-full-text-writes-remote.md)).

Check, format, export and share are compositions of `scan` and `render`, not
operations of their own. Their shared artifact is the **reference set**: cited
records, keys and fingerprints, used as a lock file, a project snapshot and a
pack manifest, and readable by Ibis as a library.

## Four declared tables and three new fields

Methods (0057), sources, outputs and targets are declared tables. Consent
screens, PRIVACY.md, MCP tool offering, the ⌘K list and
[the compatibility guide](../compatibility.md) are generated from them. A target
row carries evidence (app, version, system, date); a support claim is a row with
evidence. Each method row gains three fields before the CLI and MCP derive from
the table:

| Field | Says | Generates |
| --- | --- | --- |
| `needs` | The library capability the method requires | Whether it is offered for a library, and its refusal |
| `network` | None, or the hosts it may reach | Consent screens and PRIVACY.md's network list |
| `palette` | Whether it is a ⌘K verb, and for which selection kinds | The ⌘K list and the verbs offered beside Zotero |

## Writes

Libraries change only through proposals. Ibis writes two files of its own: a
cited entry appended to a project's own `.bib` (append-only, with Undo, off by
default per project), and a small `.bib` holding what it found for a folder of
PDFs, which the person can see and take elsewhere.

## Not doing

No new crate and no published crate before 1.0. No citation fields of Ibis's
own in Word or Google Docs: Zotero keeps managed copies live. No Ibis Zotero
plugin before 1.0: exact selection comes from Better BibTeX where present.
