# 0056 An open layer for research material

Direction recorded on 2026-11-17 at the maintainer's request. It builds on
[0028](0028-engine-first.md), [0033](0033-engine-for-agents-and-tools.md),
[0039](0039-extending-ibis-from-outside.md), [0051](0051-references-in-and-out.md)
and [0055](0055-agent-layers-full-text-writes-remote.md), and changes none of
them. Nothing here is built before the six foundation items on the tracking issue.

## Aim

Any tool, agent or app that needs a person's research material can ask Ibis,
whichever manager or folder holds it: Zotero, a `.bib` file, EndNote (today
through its RIS export), a manual list or a custom store. Research material
means references, their files (PDF, EPUB, web snapshots), the passages,
highlights and notes inside them, the collections and tags that group them, and
the manuscripts that cite them. Ibis
reads, cites and proposes changes; the person's own managers and files remain the
source of truth. Today each agent integration and plugin re-implements Zotero
reading, citation keys and CSL formatting. One correct shared layer turns many
tools times many managers into many tools plus many managers, as the Language
Server Protocol did for editors and languages.

## Master of one job

The job: give any program a correct, fast and safe answer about a person's
research material, from any source, the same way everywhere. Ibis aims to be the
obvious choice for that job, as FFmpeg is for media, and is judged by it alone.
Storage, sync, editing PDFs and writing manuscripts belong to other tools.

- **Correct.** Never a wrong citation or a guessed identity: refuse with a typed
  reason, or answer partly with typed losses and source health. The BibTeX and
  PDF readers are fuzzed (`damaged_bibtex`, `damaged_pdf`); every other reader
  gains a target, and output is checked against the manager's own where one exists.
- **Fast and small.** The engine and CLI get their own published limits for cold
  start, search, cite and memory, measured at the 10,000 references of
  [0032](0032-release-gates.md) and reported at 50,000. Numbers come from
  measurement, not guesses.
- **Consistent.** One contract, one vocabulary and one answer for the same
  request through the CLI, JSON-RPC, MCP and the app.
- **Everywhere.** macOS, Windows and Linux, with ARM builds and readers that
  build for WebAssembly in 0.7; stdio protocols any language can speak.
- **Safe to build in.** No network unless approved, bounded input and output,
  deadlines, PDFs parsed only in a child process (0055) and every new parser of
  untrusted files likewise, and no paths leaving the engine. A program that
  builds Ibis in can trust it with a person's library.
- **Easy to start.** One install command on every system (the 1.0 listings), and
  one page that shows a working script, agent and app integration in minutes.

## The app and the primitive are one plan

- **App first.** People adopt tools, not protocols. The app is how researchers
  meet Ibis and how the engine is proven correct every day. Ghostty shipped the
  terminal before extracting libghostty; Zed built gpui for Zed.
- **Primitive.** A layer others build on is worth more than one more plugin,
  and it keeps Ibis useful wherever a person works.
- **Together.** The app stays the primary product and the first client of the
  public contract, with no private door ([0052](0052-engine-owns-library-and-citations.md)).
  Every capability goes engine, then contract, then CLI and MCP, then the app.
  Building the app builds the primitive; there is no pivot and no second product.

## What already fits

Checked against `main` at 3613e88:

- Records are CSL-JSON (`Record.csl`) plus a few Ibis fields: an opaque
  `ItemRef`, citation keys with their source, and fingerprints.
- The wire contract has its own version (`ibis: 1`), is described by
  `ibis schema`, changes only additively within a version, and has stable error
  codes, recoveries and exit codes.
- One engine with thin layers: CLI, JSON-RPC with LSP framing, MCP and the Agent
  Skill. Canonical requests in `fixtures/cases` run through every front end.
- Agent writes are proposals the person confirms in Ibis, with a backup.
- The contract already names attachments, notes, annotations, collections, tags
  and saved searches; attachment text is read with verified pages.

The gaps: `SourceCapability` covers only records and keys, and reading content
branches on `LibraryKind` directly, so there is no source interface yet; the
stable surface is not written down; annotations (`page_label`, `page_index`) and
attachment text (`pdf_page`, `cite_page`) use two page vocabularies; and others
cannot add a manager.

## Lessons from other open-source foundations

| Project | What made it a foundation | For Ibis |
| --- | --- | --- |
| git | Stable plumbing under changeable porcelain; `git-remote-<name>` helpers add transports out of process; libgit2 came later, from others. | Name the stable machine surface; add managers as `ibis-source-<name>` programs. |
| Language Server Protocol | One small protocol over stdio; servers in any language. | The contract is the product for others; adapters and clients in any language. |
| FFmpeg | Libraries under the CLI; format coverage is the moat; frequent API breaks cost downstream users. | Coverage of managers and formats is the moat; keep the public surface small and versioned. |
| SQLite | A stable format and API for decades; testing is why people trust it. | Promise little and keep it; correctness tests are the reason to depend on Ibis. |
| tree-sitter | A small core; grammars as separate packages, generated and tested with one CLI. | An adapter test kit rather than adapters written by the core team only. |
| CSL and CSL-JSON | A shared format and style repository that every manager adopted. | Adopt standards rather than invent: CSL-JSON, W3C Web Annotation selectors, MCP, JSON Schema. |
| libghostty, Zed's gpui | Extracted from a working app once its own use had proven it, smallest piece first. | App first; publish a piece only after real use. |
| PyTorch | Won on an easy first five minutes, not completeness; kept its API while rebuilding internals; governance came after adoption. | One command and one example for builders; stable surface, internals free to change. |
| CUDA | Closed, yet its moat is documentation, examples, compatibility and libraries on top. | Documentation, examples, the skill and plugins count as much as the engine. |

## Beyond references

Research material forms a graph: a work has files, files have passages, notes and
highlights point at passages, collections group works, and manuscripts cite them.
Ibis covers this graph read-only first, then through confirmed proposals.

- **One anchor.** A single contract type says where inside content something
  is: one page pair replacing today's two vocabularies, plus a text-quote
  selector from the W3C Web Annotation model; time offsets for audio and video
  can join later. Highlights, quotes, `find_in` passages and
  locators share it, so a passage an agent finds can be cited, quoted or opened
  at the same place.
- **Content kinds.** PDF now; EPUB and web snapshots next (Zotero's index already
  holds their text); Markdown and Obsidian notes; datasets and software through
  their identifiers; manuscripts as input to the planned `ibis check` and
  `ibis format`.
- **Derived indexes,** such as local meaning-based search
  ([0010](0010-search.md)), are caches Ibis can rebuild, never a store of record.
- **Same rules for every kind:** opaque ids, no paths leave the engine, a
  per-library opt-in before agents read content, untrusted parsing in a child
  process (0055), and content is data, never instructions.

## Order

After the six foundation items:

1. **Source interface.** When `ibis-zotero-local` merges into `ibis-sources`,
   one Rust interface serves both real implementations (files and Zotero) with
   declared capabilities, including content access, replacing the engine's
   branches on `LibraryKind`. Each source reports a change token, and the
   contract gains one whole-library read with that token, which tools that
   mirror a library need first.
2. **Plumbing and porcelain.** [The CLI reference](../cli.md) lists the stable
   surface (`--json` envelopes, `call`, `serve`, `mcp`, `schema`, error codes and
   exit codes) and what may change freely (human output and wording). Contract 1
   stays additive, with the support window that reference already gives. The
   engine and CLI limits above are measured and added to the performance checks.
   One real outside use, such as a Pandoc filter or an agent tool that today reads
   Zotero itself, proves the surface.
3. **Anchors.** The anchor type lands in `ibis-core::contract` before
   annotations and passages ship in 0.5.
4. **External source adapters.** An `ibis-source-<name>` program on PATH, beside
   0039's `ibis-<name>` commands, speaks contract JSON over stdio: it hands the
   engine CSL-JSON records, keys, attachments and a change token. The engine keeps
   identity, validation, the privacy filter, citation and every rule above.
   It is designed with the first manager that has no built-in reader (EndNote,
   Mendeley, Paperpile, JabRef or an Obsidian vault), not before, together with
   shared invented fixtures and an `ibis` check that every adapter, built-in or
   external, passes. Adapters run in their own process, so they choose their own
   licence (0039).

## Not doing

- **Publishing the Rust engine crates for linking before 1.0.** Their API
  changes freely, and the engine is AGPL, so the process boundary is where
  outside code meets Ibis; any program can already call `ibis`, as many call the
  `ffmpeg` command. FFmpeg is also linked into products because its libraries
  are LGPL. Whether Ibis offers a linkable library, and under which licence, is
  a question for the maintainer at 1.0 ([0002](0002-licence.md)).
- **A library of Ibis's own.** Ibis does not become a reference manager, PDF
  reader or notes app.
- **A hosted service, REST API or network listener.** [0008](0008-offline-by-default.md)
  stands; remote libraries start with SSH (0055).
- **A plugin marketplace, many SDKs, or a foundation** before outside
  contributors exist.
- **Freezing the contract early.** It freezes at 1.0.
