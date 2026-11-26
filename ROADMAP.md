# Roadmap

Ibis aims to be the best base for everything people do with references, not a
collection of features. One shortcut finds a reference in any app and cites it
where they are writing: Word, LibreOffice, Markdown, LaTeX and the rest, as far
as each app allows. Around that it finds the exact passage and page and cites
it without leaving Ibis, looks up references, shares them with someone, acts as
a quick command prompt beside Zotero and works well with AI agents. Once these
core jobs are in one place, people can stop juggling workflows, apps and
systems. It serves academics, journalists and researchers, stays open source,
fast and reliable, and is simple for a first-time user and deep for an expert.

Like FFmpeg, Ibis is a hard-core engine that handles all of this, and every
front end is a client of it: the command line, MCP server, editor endpoint and
desktop app ([0056](docs/decisions/0056-open-layer-for-research-material.md)).
The aim is not an early 1.0. It is every feature that makes sense, built on a
back end and front end that stay stable, fit any use and get cheaper to extend
and maintain.

The [README](README.md), [architecture](docs/architecture.md) and
[compatibility guide](docs/compatibility.md) describe what works now. This page
holds direction and unfinished work. Versions are working steps, with scope
rather than promised dates. Each finished milestone becomes a numbered release;
Nightly carries daily testing. 1.0 adds no features: it qualifies everything in
milestones A to E on real systems and apps, and the maintainer promotes it.
Nothing is labelled Preview to escape that gate.

## How we build

- **Works with nothing, better with more.** Every capability is useful to
  someone who has only Ibis and their files, with no Zotero and no Better
  BibTeX. It gets better by itself when more is installed and wanted. Ibis never
  asks which tools a person has, and nobody installs several tools to do one
  small thing ([0059](docs/decisions/0059-works-with-nothing-better-with-more.md)).
- **Simplest correct form.** Build the smallest useful feature that is right.
  Cases nobody has met wait. Functionality first, then polish.
- **Subtract before adding.** One owner and one way to do each job. Delete the
  code, tests and docs a replacement makes unnecessary. Before 1.0, keep no
  compatibility code for Ibis's own formats, and settings may reset on update.
- **Define once.** Derive contracts, wording, interface values and checks from
  their owner. Remove layers and process steps that make changes harder.
- **Engine first.** Shared facts and behavior live in Rust; front ends submit
  requests, show results and own their system effects
  ([0052](docs/decisions/0052-engine-owns-library-and-citations.md)).
- **Only correct output.** A wrong citation is worse than none. Document-wide
  numbering, ibid. and generated year suffixes need document-wide knowledge.
- **Reveal as needed.** Search and cite first; documents, pages and sharing
  appear when useful. Plain words and shared interface parts throughout.
- **Offline by default.** Online lookup follows
  [0008](docs/decisions/0008-offline-by-default.md). Libraries change only
  through proposals the person confirms in Ibis
  ([0055](docs/decisions/0055-agent-layers-full-text-writes-remote.md)). Ibis
  writes only two files itself: a project's own `.bib`, when that project turns
  on appending, and the `.bib` it keeps for a folder of PDFs.
- **Claim demonstrated support.** Use invented data for checks; qualify native
  behavior on the systems and apps the compatibility guide names. A support
  claim is a target row with recorded evidence.

## The engine

```
inputs → model → operations → outputs → thin front ends
```

- **Inputs:** sources (Zotero, `.bib`, CSL-JSON, RIS, EndNote, adapters, a
  folder of PDFs) and documents (Markdown, Quarto, LaTeX, Typst, DOCX, ODT).
  Untrusted parsers run in a child process of `ibis`.
- **Model:** five nouns
  ([0059](docs/decisions/0059-works-with-nothing-better-with-more.md)). A
  **Source** is a snapshot with declared capabilities.
  A **Selection** is ordered references with optional anchors, prefixes,
  suffixes and forms. An **Anchor** is a place inside content; a locator is a
  projection of it. A **Context** resolves optional facts (app, site, project,
  explicit choice) to format, style, locale and bibliography. An **Output** is
  written to a **Sink**: clipboard and paste, editor reply, file, standard
  output, drag or open.
- **Operations:** `find`, `locate`, `render`, `scan` and `propose`. Check,
  format, export and share are compositions of `scan` and `render`.
- **Four declared tables:** methods
  ([0057](docs/decisions/0057-one-method-table.md)), sources, outputs and
  targets. Rows say what they offer or `needs`, their `network` use and whether
  they are a `palette` verb. Consent screens, PRIVACY.md, MCP tools, the ⌘K list
  and the compatibility guide derive from them.
- **Front ends** are projections with no handlers of their own. Settings have
  one schema; everything Ibis says reaches people as a `Problem` in the notice
  slot.

Adding a manager costs one adapter and the conformance suite; a document type,
one scanner and fixtures; an output, one row and one writer; an app, one target
row with evidence; a front end, one projection.

## Milestones

Each foundation lands just before the first feature that needs it. When items
compete, the first person to serve is an academic writing in LaTeX or Markdown,
and Ibis should feel like an invisible utility: summon, cite, gone, with depth
only when asked. The tracking issue holds the order and what is in flight.

Every capability says what a person gets at each rung. **Rung 0:** Ibis with
files, PDFs or a `.bib`. **Rung 1:** plus Zotero. **Rung 2:** plus Better
BibTeX or the ODF/DOCX Scan plugin for Zotero. A rung is detected from what a
read returns, never asked; a verb whose needs are unmet is not offered, or
refuses with a reason and the next-best verb. Delivery, context, packs, the
reader and agents work the same at every rung, on whatever the engine can open.

### A. Public beta (0.3.x)

**Payoff:** a stranger installs Ibis and cites in their own app, and every
support claim has proof.

- **Cite in any app:** one delivery bar and dense rows; formats remembered per
  website; welcome-screen switches; the paste permission asked on the practice
  line, with Just Copy. One delivery ticket on macOS and Windows.
- **Proof:** target rows with evidence (app, version, system, date), and the
  compatibility guide's writing-apps table generated from them. First recordings
  on macOS and Windows for VS Code, one TeX editor, Obsidian, Overleaf and Word,
  with Zotero 7 and 10 running and closed. Nothing claims live citations that
  only a marker test covers.
- **Two trials:** Linux paste (X11, then GNOME Wayland) settles Linux's delivery
  ticket; Zotero-managed citations (Scan markers, then a generated field
  document) settle Zotero-managed copies in C.
- **Release:** every package ships its licence notices, using the standard SPDX
  text where a dependency ships no file; every package is verified on its system
  before publishing; parsers are fuzzed nightly; releases carry provenance and
  SBOMs, and stable ones cannot change.
- **Distribution:** Nix and the project's own Homebrew tap. Until signing is
  decided near 1.0, macOS uses the self-signed identity and Windows installers
  are unsigned, so the official Homebrew cask waits.
- **Help:** a documentation site covering install (with pictures of each
  system's first-launch block), first use, privacy and troubleshooting; an
  offline Copy Diagnostics for bug reports instead of telemetry; help
  screenshots kept current.
- **Then** a user study of the whole experience; its results reorder B.

### B. 0.4 One contract, one context, one delivery

**Payoff:** the right format in every app, site and project; automatic paste on
all three systems; one ⌘K list.

| Capability | Rung 0 | Rung 1 | Rung 2 |
| --- | --- | --- | --- |
| Every citation form | several references with prefix, suffix and form; every locator; real style previews | the same | the same |
| Beside Zotero | not applicable | ⌘K with Zotero in front: search, reveal, open at the page, copy, share; the selected collection | Zotero's selected items through Better BibTeX |

- **Context:** one resolution over optional facts: app, site, project, editor
  request and explicit choice. A site is a fact about the target, and a site
  with a project is the Overleaf writer's context. Overleaf's downloaded `.bib`
  works on free plans.
- **Delivery:** automatic paste on macOS, Windows, X11, and GNOME and KDE on
  Wayland after a one-time system consent. Sway and Hyprland copy and say so
  until their desktops offer the portal. Wayland names no target app, so there
  paste is one choice for all apps.
- **Verbs:** one list chosen by selection and target: Enter, ⌘K, drag, Copy as
  Markdown, Copy Again and Open. With Zotero in front the defaults are Copy,
  Open at Page, Reveal and Share.
- **Citations:** the citation dialog, every locator and real style previews,
  with projects directly in the format palette; multi-reference citations with
  prefix and suffix, taught.
- **Settings:** one settings schema; then customizable shortcuts, quiet updates
  installed on quit, the Libraries page with connected libraries searched
  together, Zotero while it is open always clear and never blocking, and an
  empty-search tip.
- **Agents:** MCP as a projection of the table, with argument renames recorded
  once; a "May propose changes" switch in Settings › AI Apps beside the
  `--allow-writes` flag, with its review sheet; a library on another computer
  through `ssh HOST ibis mcp`; then the MCP Registry listing.
- **Order of foundations** ([0064](docs/decisions/0064-one-citation-one-status-generated-surfaces.md)):
  words only from the catalogue; CLI and MCP from the method table; one
  status owner (0063's status line); closed parts, regions and the gate
  ([0062](docs/decisions/0062-closed-parts-owned-space-fitted-layers.md)); the
  empty-search history and formats only in the picker
  ([0063](docs/decisions/0063-form-follows-function.md)); generated events; the
  settings row and schema; the citation-text grammar; the Draft, then typed
  citations in the picker; the connection runtime; generated grants.
- **Foundations,** each just before its feature: 0057 steps 3 to 12 with the
  three row fields, which finish moving the app onto the engine (0052); the
  Selection request replacing the separate item shapes of `cite`, `records` and
  `share`; one output descriptor replacing the output booleans; the whole
  Anchor type replacing the two page vocabularies; the `writings` settings key
  renamed `projects`; the picker controller owning its asynchronous work.
- **Checks:** tests of supported outcomes instead of source or CSS text;
  generated action and race sequences for the picker, Settings and delivery,
  with fake time and reviewed engine snapshots; published engine and CLI limits
  for start, search, cite and memory in the performance checks.
- **Last,** once screens stop changing: the design-system pass, accessibility on
  the cite path with VoiceOver, NVDA and input methods, and translations
  refreshed after a string freeze.

### C. 0.5 Your document

**Payoff:** a submission-ready document with only Ibis and a `.bib`, and
co-authors on different managers working from one reference set.

| Capability | Rung 0 | Rung 1 | Rung 2 |
| --- | --- | --- | --- |
| Check and export | `ibis check` on Markdown, Quarto, LaTeX, Typst, DOCX and ODT; export what a document cites to BibTeX, Hayagriva or CSL-JSON; missing keys minted into the project's reference set, never the library | Zotero's own keys | pinned Better BibTeX keys |
| Finish a document | `ibis format` makes a static DOCX or ODT copy and never edits the original; rerunning on the original restyles it | also a copy whose citations Zotero's plugin keeps live (Word, LibreOffice; Google Docs through Zotero's transfer document), if the trial passed | Scan markers that the Scan plugin turns into Zotero citations |
| Keep the project's `.bib` complete | append a cited entry to the project's own `.bib`: append-only, with Undo, off by default per project | the same | the same |
| Is this in my library | a pasted identifier, reference or dropped PDF resolves to candidates; a held DOI cites at once; online lookup only after asking | Zotero's records | the same |
| Share | a pack with a readable list, a `.bib` and the PDFs, from the app and the command line | imports cleanly into Zotero | the same |

- **The reference set:** cited records, keys and fingerprints in one artifact
  that serves as the lock file, the project snapshot and the pack manifest. Ibis
  opens a set as a library, which serves co-authors and round-trip tests. A
  changed key is a `check` finding against the set, with Forget; Ibis never
  infers a BibTeX rename from matching titles or identifiers.
- **Everywhere a document goes:** Pages and Scrivener through `format` on an
  exported DOCX or ODT; document operations as MCP tools.
- **Qualification:** Pandoc, Quarto, Typst and LaTeX builds end to end; Word,
  LibreOffice and Google Docs round trips; Zotero 10 ODF Scan with Ibis's own
  LibreOffice readback.
- **Foundations:** document parsers move from the CLI crate into
  `ibis-sources`, behind a feature only `ibis` enables, so no untrusted parser
  links into the app; one engine document module for `scan`, `check`, rendering
  and packs; `format` and DOI lookup re-land as rows, lookup as `resolve`'s
  network tier.

### D. 0.6 The place in the source

**Payoff:** find a passage, quote it and cite its page without leaving Ibis,
from any library or a folder of PDFs.

| Capability | Rung 0 | Rung 1 | Rung 2 |
| --- | --- | --- | --- |
| Cite a folder of PDFs | each PDF's own metadata or DOI, read locally; lookup only after asking; results kept in a small Ibis-owned `.bib` the person can see and take elsewhere | not needed | not needed |
| Passage and page | text of PDFs linked from a `.bib` or in a folder, by the child extractor | Zotero's text cache; Zotero highlights, notes and comments with their page; open at the page in Zotero | the same |
| Read and quote | the built-in read-only reader on all three systems; Copy Quote with Reference | the same, on Zotero attachments | the same |

- **More of the library:** sources carry highlights and notes in the
  whole-library read; Obsidian reference notes as the second adapter; EndNote
  in the app; collections, tags, saved searches, filters and Zotero's selection;
  Chinese, Japanese and Korean names by romanized reading.
- **Passages:** quotes stay distinct from notes, and physical and printed pages
  stay distinct, and whether passages unfold under references or have their own
  view is decided after a walkthrough with reference-manager and file users; a
  dropped PDF finds its reference; the evidence brief is a pack
  of anchors; add a note instead of a citation; `ibid` recall; opt-in agent
  access to passages per library.
- **Opening:** open at an anchor in the person's chosen viewer or editor, with
  a menu when a reference has several PDFs. Open actions are named by their
  target, a failed open offers alternatives, and
  an unknown printed page asks.
- **The reader** comes last, through
  [the reader boundary](docs/decisions/0036-reader-trust-boundary.md): PDF.js
  only paints in its own window, and the engine is the only source of text,
  page labels and anchors.

### E. 0.7 Every system, every client

**Payoff:** the same Ibis on every desktop and in every agent.

- **Windows:** tailored windows, focus and setup text; Windows 10 and 11
  qualified with NVDA.
- **Linux:** tailored windows and setup text, the desktop accent, Orca; paste
  and the reader qualified on X11, GNOME and KDE.
- **Every system:** ARM64 builds and declared oldest systems; non-US keyboard
  layouts, sleep and wake, cloud and network-drive recovery; LibreOffice
  readback on Mac and Windows; winget and Flathub.
- **Every client:** Claude, Cursor and Codex qualified, and one real outside
  client on the stable tier; VS Code Citation Picker for Zotero qualified
  against the editor endpoint. On request: a same-user socket to the running app
  (no network listener) and thin editor clients.

### F. 1.0 Stable

Everything in A to E qualified, then the maintainer promotes it. Paid signing
is decided near 1.0: Developer ID and notarization on macOS, Windows signing,
then the official Homebrew cask and Windows App Actions. The documentation site
is complete. From 1.0, an update keeps settings.

## Definition of done for 1.0

Every row of the generated compatibility table has recorded evidence on the
exact release.

- **Systems:** macOS 13 and later on Apple silicon and Intel; Windows 10 and 11;
  Linux with automatic paste on X11 and on GNOME and KDE Wayland; ARM64 builds.
- **Sources:** Zotero 7 to 10, running and closed, with and without Better
  BibTeX; `.bib`, CSL-JSON, RIS, EndNote and a folder of PDFs; 10,000
  references enforced and 50,000 reported.
- **Apps:** VS Code, a TeX editor, Overleaf, Obsidian, Typst, RStudio and
  Quarto, Word, LibreOffice and Google Docs; the reader on all three systems.
- **Agents:** Claude, Cursor and Codex; one real outside client on the stable
  tier.
- **Accessibility:** VoiceOver, NVDA and Orca; Chinese, Japanese and Korean
  input methods; large text, contrast, reduced motion, scaling, right-to-left
  and mixed-direction text.
- **Operations:** a real update on each system keeping settings and the paste
  permission; licence notices, SBOMs and immutable releases; the update-key
  rotation and loss procedure rehearsed; latency, idle CPU, wakeups and memory
  measured on named machines; before each numbered release, the nightly checked
  on all three systems with at most 45 risk-ordered scenarios.

## Later, on request

- **Add by identifier, and proposals for Zotero,** through Zotero's local write
  API: it needs Zotero 10's write authorization and its own review.
- **EPUB and web snapshots in the reader:** PDFs come first.
- **Retraction and status display:** needs a dated data source and its own
  network entry.
- **Local meaning-based search** ([0010](docs/decisions/0010-search.md)):
  keyword search comes first.
- **A Word clipboard field and real footnotes on paste:** plausible but
  unproven and large; the Scan and generated-document routes come first.
- **Other managers' annotations** (Paperpile, JabRef, BibDesk and Skim, EndNote,
  Mendeley, Citavi): each through its adapter, when its users ask.
- **Word Sources.xml:** one output row when Word users ask.
- **Launcher extensions:** `ibis://` links and the command line already serve
  launchers.
- **Command plugins** (`ibis-foo` on PATH, built-ins first): they wait for a
  second outside use.
- **Python and npm clients:** the command line and JSON-RPC serve scripts today.
- **WebAssembly builds** of the rules and readers: no client needs them yet.
- **SSH beyond `ssh HOST ibis mcp`:** the person's own SSH covers the first need.
- **More interface languages:** as translators offer them.

## Do not build

- **Live Word or Google Docs fields of our own, or an Office.js add-in.** At
  1.0, Word and Google Docs get static citations plus `ibis format`; Zotero keeps
  its managed copies live.
- **An Ibis Zotero plugin before 1.0.** Exact selection comes from Better
  BibTeX where present.
- **Reference storage, import or sync,** or a replacement reference manager.
- **A PDF or annotation editor.** The reader is read-only.
- **A browser extension,** blocked on Overleaf's written agreement.
- **A language server.**
- **The Zotero Web API, iCloud settings sync** or a plugin marketplace.
- **Hosted share links or accounts.** A pack is a file.
- **Telemetry.**
- **An AI literature-review writer** or discovery index.
- **A citation-style editor** or general document converter.
