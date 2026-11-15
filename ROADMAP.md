# Roadmap

Ibis is a writing companion for the references people already keep. One
shortcut finds a reference in any app and cites it where they are writing.
It is growing toward the everyday work around literature: adding, finding,
remembering, citing and sharing references, for academics, journalists and
researchers. It stays open source, fast and reliable, simple for a first-time
user and deep for an expert, across sources, writing apps, systems and languages.
The engine, command line, MCP server and desktop app share those capabilities.
References and research material define the scope.

The [README](README.md), [architecture](docs/architecture.md) and
[compatibility guide](docs/compatibility.md) describe what works now. This page
holds direction and unfinished work. Versions are working steps, with scope
rather than promised dates. Each finished milestone becomes a numbered release;
Nightly carries daily testing. 1.0 adds no features: people qualify the work and
the maintainer promotes it.

## How we build

- **Simplest correct form.** Build the smallest useful feature that is right.
  Cases nobody has met wait. Functionality first, then polish.
- **Subtract before adding.** One owner and one way to do each job. Delete the
  code, tests and docs a replacement makes unnecessary. Before 1.0, keep no
  compatibility code for Ibis's own formats.
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
  [0008](docs/decisions/0008-offline-by-default.md). Sources stay read-only until
  the explicit write workflow in [0051](docs/decisions/0051-references-in-and-out.md).
- **Claim demonstrated support.** Use invented data for checks; qualify native
  behavior on the systems and apps the compatibility guide names.

## Next

- Finish [engine library and citation ownership](docs/decisions/0052-engine-owns-library-and-citations.md),
  deleting the desktop's duplicate lifecycle and citation policy as each moves.
  Keep each client's writing, libraries and output fixed for its task.
- Finish the picker controller's ownership of asynchronous work and consolidate
  shared controls as screens change. Keep UI state in the interface and library
  and citation behavior in the engine.
- Merge `ibis-zotero-local` into `ibis-sources` behind the engine.
- Replace tests of source/CSS text and call order with supported outcomes;
  generate reproducible action/race sequences for picker, Settings and delivery.
  Use fake time for deterministic waits and reviewed engine snapshots.
- Match performance checks to release sizes: enforce at 10,000 references and
  report a 50,000-reference nightly run.
- Complete native VoiceOver, NVDA and input-method journeys, update permission
  retention and help screenshots.

## 0.4 Citation and integration work

- Allow locators without a type where the output can preserve them.
- Real citation-style previews and writings directly in the format palette.
- Overleaf on free plans: read a downloaded project bibliography offline;
  a browser extension requires Overleaf's written agreement.
- Qualify live Zotero citations through ODF/DOCX Scan and the switch-word-
  processor flow for Google Docs. Trial footnotes and Zotero's Word field only
  if they work; document exactly what was tested.
- Customizable shortcuts; connected libraries together in the app; an empty-
  search tip; opt-in download and install on explicit quit.
- Drag citations into apps. Configurable reference opening, editor and PDF
  viewer choices, several-PDF menus and Open PDF at This Page only with a known
  printed-to-physical page mapping.
- Copy as Markdown. Reusable agent setup using the bundled command. Publish
  the MCP Registry listing.
- Copy Again in the native menu once engine citation sessions support it.
- German and one right-to-left interface language. Public beta.

## 0.5 Find the passage

- Search Chinese, Japanese and Korean names by romanized readings; collections,
  tags, saved searches, filters and Selected in Zotero.
- Search Zotero highlights, notes and comments with their reference and page.
  Keep quotes distinct from notes; physical and printed pages stay distinct.
  Decide whether passages unfold under references or have their own view after
  a comprehension walkthrough.
- Read-only PDF preview for Zotero and BibTeX attachments, through
  [the reader boundary](docs/decisions/0036-reader-trust-boundary.md). EPUB and
  web snapshots follow. Search, preview and quote PDFs or Markdown notes outside
  a reference manager; cite them only when their details are known.
- Drop a PDF to find its reference; Copy Quote with Reference; export selected
  passages as a Markdown brief. Briefs repeat the reference per passage, treat
  questions as literal text and never overwrite a file.
- Name open actions by their target; failed opens offer alternatives rather
  than silently choosing one. Unknown printed pages ask.
- Opt-in agent access to passages per library, off by default, without private
  links. Check quote/comment comprehension with reference-manager and file users.
- Add a note instead of a citation.
- Warn when a successfully copied key changes for a provably identical item;
  design the interaction first and provide Forget. Never infer a BibTeX rename
  from matching titles or identifiers.
- Check whether a pasted identifier/reference is in the library and whether it
  has a PDF or saved web page. Last-cited recall through `ibid`/`ebd.`.

## 0.6 Your document

- Export what a document cites to BibTeX, Hayagriva or CSL-JSON.
- `ibis check` on Markdown, Quarto, LaTeX and Typst manuscripts, optionally with
  a coauthor lock file.
- `ibis format` makes a finished DOCX/ODT copy from citation keys, with a style
  and bibliography; reruns can change style and never edit the original.
- Reference handling follows [0051](docs/decisions/0051-references-in-and-out.md):
  desktop sharing and file destinations; then paste a DOI to cite using the
  single opt-in lookup module; then add by identifier through Zotero's official
  local write API. Attachment sharing needs its own design and review.

## 0.7 Everywhere you work

- WebAssembly-compatible rules/readers with optional native Zotero/SQLite.
  Useful thin Python/npm clients and pipe-based extensions when requested.
- Same-user local CLI connection to the running app, with independent reads
  when it is closed; no network listener. Command plugins use `ibis-foo` on
  PATH, with built-ins taking precedence.
- Pasting on Linux, a language server, launcher and browser extensions.
- A library on another computer through the person's own SSH.
- More translations, EndNote XML and Word Sources.xml, ARM64 builds and local
  meaning-based search while the picker is open.
- Annotations from Paperpile, JabRef, BibDesk/Skim, EndNote, Mendeley and
  Citavi, read from each manager's own data.

## 1.0 Stable

Qualify every planned capability with real clients and supported platforms.
Publish Flathub, Homebrew, Nix, winget and MCP Registry listings, the documentation
site, licence notices and supply-chain records. Paid signing remains a decision
near 1.0; macOS uses the self-signed identity and Windows installers are unsigned.

- Complete VoiceOver/NVDA journeys, Chinese/Japanese/Korean input-method checks
  and Orca; native visual checks include large text, contrast, reduced motion,
  scaling and mixed-direction text.
- Qualify OS minima, Wayland consent/focus, cloud/network-provider recovery,
  non-US keyboard layouts, sleep/wake and Windows 10.
- Extend native editor journeys with LibreOffice rich-text readback on Mac and
  Windows. Check Google Docs and Overleaf in isolated invented documents;
  unavailable accounts or permissions are explicit blockers.
- Measure exact-release latency, idle CPU/wakeups and whole-process memory on
  named Mac/Windows/Linux machines, with settled samples and cycle trends.
- Before numbered releases, check the nightly on all three systems with at
  most 45 risk-ordered scenarios; restore settings and record failures/blocks.
- Qualify VS Code Citation Picker for Zotero against the editor endpoint and
  Zotero 10 ODF Scan with Ibis's own LibreOffice round trip before claiming them.
- Publish release SBOMs, immutable stable releases and the update-key rotation
  and loss procedure.

## Conditional work

Paid signing enables Developer ID/notarization and Windows signing. Opt-in
macOS iCloud settings sync needs a network decision. Windows App Actions and
its agent registry wait for signing.
