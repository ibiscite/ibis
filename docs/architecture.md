# Architecture

This is the target architecture for the rebuild agreed on 2026-10-26 ([ADR 0028](adr/0028-engine-first.md)). Code on `main` still follows the older design in places; anything rebuilt follows this document. Build order is in [ROADMAP.md](../ROADMAP.md).

```
                ┌─ ibis (command line): every capability, for people, scripts and AI agents
                │    --json on every command · ibis serve --stdio (JSON-RPC) · ibis mcp
 ENGINE ────────┼─ Desktop app: Tauri 2 shell + React/React Aria picker; links the engine in-process
 (Rust crates)  ├─ Editor endpoint compatible with Better BibTeX Cite as you Write
                └─ Later: language server, launcher integrations
   │
   ├─ ibis-core     pure rules: model, citation request/result, key syntaxes, rich text, locators, search
   ├─ ibis-sources  Zotero (local API, private copy of the database), BibTeX/BibLaTeX, CSL-JSON, snapshots
   ├─ ibis-csl      citeproc-js in embedded QuickJS + ICU4X collation; official CSL styles and locales
   ├─ ibis-engine   typed methods: search, cite, show, styles, check, export…; JSON envelope, errors
   └─ ibis-cli      argument parsing and terminal output over ibis-engine
```

## Principles

- **The engine is the product; front ends are clients.** Every capability lives in the Rust engine as typed methods. The command line exposes all of it; the app, `ibis serve` and `ibis mcp` each expose an explicit subset suited to them (the picker does not administer the engine; headless clients do not open windows). A test fails when two front ends give different results for the same request. No front end formats, resolves or decides anything itself.
- **One citation request, one result.** Preview, copy, paste, export, the command line and the editor endpoint all call `cite(snapshot, request) -> result`. What you see is exactly what is pasted.
- **Only what Ibis can get right.** A field a format cannot carry is reported as a typed loss; a citation form it cannot carry is refused. Where correctness needs the whole document (numbering, ibid., 2020a/b), Ibis gives what is correct on its own and says why.
- **Read-only sources.** Never write to Zotero, `.bib` files or any library. Raw records are kept next to normalised data.
- **Stable identity.** An item is `library + native key`, never a citation key or a file path.
- **Never fail silently or guess data.** Skipped data becomes a warning; a failed paste leaves the citation copied with a reason; keys are never invented.
- **Offline by default.** No telemetry. Network access needs an accepted ADR and is opt-in (DOI lookup is planned this way for 0.6).
- **Behaviour as data.** Presets, app rules and templates are validated data; nothing a user configures executes code.
- **Escape every output** for its target: LaTeX, Pandoc, Typst, org, HTML, RTF.
- **Keep private data private.** Paths never reach the webview; fixtures are synthetic; diagnostics use fixed codes, never raw text or payloads.
- **Local endpoints** bind to `127.0.0.1`, require a token and reject browser `Origin` headers.
- **Long-lived foundations.** Standards and widely used open source first (CSL 1.0.2, CSL-JSON, BibTeX, SQLite, Zotero's APIs, PDF.js). Each dependency that could fade sits behind one Ibis module so it can be replaced: Tauri, React Aria, PDF.js, citeproc-js, the BibTeX parser.

## Data model (ibis-core)

- **Objects:** `Record` (a citable work: CSL-normalised fields plus raw provenance and citation key with aliases), `Attachment`, `Note`, `Annotation`, `Collection`, `Tag`, `SavedSearch` (name and conditions only). Each is addressed by `ItemRef { library, key }`; children point to their parent.
- **Availability** of an attachment is computed by Ibis the same way in every mode: `OnDisk`, `CloudPlaceholder`, `NotDownloaded`, `Online`, `BrokenLink`, `Unreadable`, `Unknown`. Standalone notes and files are shown without inventing a parent.
- **Library IDs are opaque** (`file:<hash>`, `zotero:<library>`). File locations stay in the engine and never serialise.
- **Snapshots are immutable.** A refresh builds a new snapshot and reuses unchanged libraries. Each object has two fingerprints: `cite` (what a citation depends on) and `full` (everything shown). A picker session keeps the snapshot it started from. At copy time, in one step against the newest snapshot, each pick is checked by `ItemRef` and `cite` fingerprint and, for key outputs, its key is checked to still name exactly that one item across all loaded libraries; a pick that was deleted, changed what it cites or lost a required key to another item is refused with the reason. Formatted citations do not require a citation key. The picker shows the snapshot's age when it is old and says when a refresh failed.
- **Source identity lifecycle:** a file library's ID follows its canonical path (a moved file is a new library; an edited file keeps its ID), and Zotero uses one logical default personal library across live and closed reads. Default-directory discovery supplies only the closed fallback; API metadata does not prove a physical data directory. A single private source generation and ready flag retire prepared Zotero citations during an uncertain full read or detected switch; only a coherent publication makes the source ready again. The generation is captured with the render snapshot and checked again under the native copy barrier, alongside citation fingerprints and the prepared content hash.

The initial shared vocabulary is in `ibis_core::contract`; its generated schema is
`fixtures/engine-contract.schema.json`. Source adapters, formatters and front ends
adopt it in the ordered rebuild issues. Raw provenance stays engine-only and is
excluded from the public projection and both fingerprints. Non-item objects use
source-namespaced native keys so tags and collections cannot collide with item
keys. Rich text is a fixed-depth tree of styled text runs; strings remain
untrusted data and are never injected as markup.

`Record.identifiers` carries canonical source-neutral search identifiers, including
arXiv eprints that have no printed CSL field. It is omitted when empty and affects
only the full fingerprint; changing search metadata does not invalidate a citation.
`identifier::of_record` combines these identifiers with recognised CSL fields.
Explicit identifier values pass through the same canonical parsers as pasted
queries; malformed or oversized values are ignored before arXiv/DOI equivalence.

## Sources (ibis-sources)

- **Capabilities are a fixed list per source kind**, not a plugin system. Zotero open and Zotero closed declare the same list, and a test enforces it; anything only one path can provide is not offered.
- **Zotero:** Ibis maps Zotero's raw item data to CSL itself, from a pinned copy of `zotero-schema`, instead of reading Zotero's `csljson` export (it changes between releases, plugins alter it, and it is the slowest part of a scan). Both access paths fill the same neutral item.
  - **Running:** the local API (needs Zotero's "Allow other applications"). Zotero 10 reconciles its complete item-version map and fetches changed raw records; missing IDs remove deletions. Zotero 7–9 conservatively reread all records. Reload, a changed API instance or a backwards library counter rereads everything.
  - **Closed:** only when no Zotero process holds the data directory, Ibis copies `zotero.sqlite` and its WAL or journal into a private directory (owner-only permissions, excluded from backups, size-checked against free space), checks that the source did not change during the copy (size, modification time and SQLite header before and after), runs SQLite's quick check on the copy, reads it and deletes it. A changed or failing copy is retried up to three times, then refused with a reason. Leftover copies are deleted at start. The live database is never opened. Versions are told apart by schema; a database newer than Ibis knows is refused with a message. Copying while Zotero runs (for a faster start) stays off until a test with concurrent writes, checkpoints and journal recovery shows it safe.
  - **Citation keys** are read from Zotero's `citationKey` field (Zotero 8+), a `Citation Key:` line in Extra, and Better BibTeX's key store (read as a copy, through its own versioned adapter). Each key keeps where it came from. When they agree, that is the key; when they disagree, the item shows the conflict and key outputs for it are refused until it is fixed in Zotero, because Ibis cannot know which key the document's bibliography uses. Aliases come from Extra. A key that matches zero or several items is refused for that item. An item's identity is always its `ItemRef`, never its key.
  - Zotero 10's local write API exists; Ibis stays read-only until an opt-in ADR says otherwise.
- **BibTeX/BibLaTeX:** each entry is parsed independently, so a broken entry produces a warning with its line while the rest loads; work is bounded so hostile files cannot hang the engine. Long-lived clients watch files and only replace them after two identical reads. A watch failure keeps the successfully read snapshot usable and reports the fixed `live-refresh-off` warning; manual Reload remains available. One-shot CLI calls install no watches.
- **CSL-JSON:** `citation-key`, then `id`, is the key. An `id` only proves a match against a bibliography made from that same file, which the item's details say.

## Citations (ibis-core, ibis-csl)

- **Request:** ordered items (the same reference may appear twice); a citation form (normal, narrative, author only, year only, full, bibliography only) and "keep my order"; per item a typed locator (one of the 29 CSL 1.0.2 locator terms), rich-text prefix and suffix (italic, bold, sub, superscript), author suppression, and the annotation it came from if any; an output spec or named preset; an input language for parsing typed locators and an output locale.
- **Result:** text, HTML, RTF and a display tree; one outcome per item; typed losses; notices such as "needs the whole document"; a content hash that copy and paste must match.
- **Key syntaxes** are pure Rust, each with a capability table (which fields and forms it carries, its escaping, a fixture checked against its own tool): Pandoc, BibLaTeX (any command, multicite), natbib, LaTeX `\cite`, Typst, org-cite, MyST, MultiMarkdown, AsciiDoc, Jupyter, Jekyll Scholar, Scannable Cite, bare keys, CSL-JSON, Better BibTeX's CAYW JSON.
- **Formatted output:** Zotero's citeproc-js runs in embedded QuickJS (`rquickjs`) with collation from ICU4X, so the command line and the app give byte-identical output on every system. Two formatter runtimes on their own threads: one for interactive previews and copies, which always goes first, and one for bibliographies and batches. Each has a bounded queue, cancellation through QuickJS's interrupt handler, memory and time limits, and is restarted after an interruption. Any official style and locale. Before formatting, Ibis applies Zotero's own data conversions (Extra fields, creators, dates, URL stripping for printed articles with pages); differential fixtures compare the result with pinned Zotero output. The formatter and AI agents then get only `ibis_core::citable` fields: note field overrides become fields, and notes, annotations and private links are dropped.
- **What a citation cannot know:** selected items (or even the whole library) cannot establish a document's numbering, year suffixes or ibid. Numeric styles give the reference entry with a notice (today's fallback), note styles give the full first note, and 2020a/2020b suffixes are withheld with a notice. Byte-identical output across front ends is necessary, not sufficient: correctness is checked against Zotero.

A result may report partial outcomes for inspection, but clipboard clients must
use `copy_content(expected_items)` to require all requested items, no refusals and
a matching content hash. An annotation pick carries its own cite fingerprint. Native clients obtain an opaque `PreparedCitation` from the engine. `commit_citation` checks its engine, preview hash, current picked records, annotation parent/file/page and any key bindings before invoking a native clipboard callback under the final publication barrier. Formatting and validation happen outside that barrier. The app holds its source/intent generation guard first, then enters the engine barrier; its callback only writes the supplied canonical content and never reenters the engine. The serialized method surface has no clipboard operation, and publication notifications use channels rather than synchronous app callbacks.
Omitted input language means `en-US`; omitted output locale means the style's
default, never the operating system's locale.

Today's key outputs use `ibis_core::syntax::cite(&Snapshot, &CitationRequest)`.
Its capability table drives supported forms and typed field losses; explicit
unsupported forms are refused. Source order and repeated selections are retained.
Key output writes instructions for the destination processor, whose style can
subsequently sort or format the citation. The CLI and picker prepare citations
through the shared engine; the picker copies only its prepared result. The
remaining unused desktop commands and old core helpers are removed in #1260,
after their live discovery, settings and transport callers are replaced.

Literal affixes escape default consumer smart punctuation as well as markup.
Pandoc narrative, plain LaTeX and Typst place prefixes outside the citation and
report a typed prefix/flattened loss, since a footnote-style citation cannot
carry that prefix in the same position. Capability tables list forms restricted to one item;
multi-item Pandoc narrative requests are refused until mixed clusters are built
in 0.4, rather than silently becoming several citation clusters. Pandoc affixes
isolate superscripts from citation brackets and use explicit locator boundaries,
so a literal suffix cannot become a page number. Locators containing braces,
brackets or backslashes alongside a suffix are refused when Pandoc cannot
preserve that boundary. Empty affixes add no punctuation.
BibLaTeX page locators followed by literal suffixes carry an explicit localized
page label; LaTeX locator quotes remain literal while range dashes stay intentional.

## Engine interface (ibis-engine, ibis-cli)

The rollout starts with the file-based engine and CLI, then ports today's picker
onto that surface. Zotero mapping and the closed/live readers proceed in parallel
on raw item data; #1197 integrates both providers, and #1200 gates full parity.
Advanced mixed citation editing remains in 0.4; unsupported intents are refused
explicitly rather than approximated. The 0.2 release still requires all sources and the picker. Engine `cite` routes
`OutputSpec::Csl` to the shared bounded formatter on the same immutable snapshot;
`styles` lists the five offline styles. Human CLI citation/bibliography options,
canonical JSON requests and stdio use those same results. RPC cancellation reaches
the formatter task, and exhaustion remains a typed resource-limit error.

- **Typed engine methods** are the source of truth for the command line, `ibis serve --stdio` (JSON-RPC 2.0, the way Codex's app-server and language servers work), `ibis mcp` for AI agents, and the app. Each front end lists which methods it exposes; the app keeps per-window command permissions (ADR 0024), so a generic dispatcher never widens what a window may call.
- **Every `--json` output is a versioned envelope** `{ "ibis": 1, "ok": …, "data" | "error": … }` with stable error codes, including argument errors. `data` carries partial results with per-item outcomes, losses and refusals; an empty search is `ok` with no results. Only the envelope goes to stdout; logs go to stderr.
- **Exit codes** describe how a command line run ended: 0 ok (including empty results), 1 done with findings (refusals, losses, source warnings or refresh failures), 2 usage, 3 bad input, 4 a named item or library not found, 5 source unavailable or resource limit exceeded, 6 newer format than Ibis knows, 70 internal error, 130 cancelled.
- **`ibis serve --stdio`** uses JSON-RPC 2.0 with LSP-style `Content-Length` framing, notifications for progress and library changes, `$/cancelRequest`, a message size limit and nothing else on stdout. Its errors are JSON-RPC errors whose `data` carries the Ibis error code; exit codes do not apply per request. `ibis mcp` keeps MCP's own envelope over the same methods.
- **The command line ships on its own:** every nightly and release publishes `ibis` for each system as an archive (`ibis-<system>.tar.gz`, `ibis-Windows.zip`) with the licence and third-party notices, checksums and the same build provenance as the app, and the release workflow runs each archive on its system before publishing (the Intel macOS archive is cross-built and not run). The app does not install it on PATH.
- **`ibis schema`** prints the JSON Schema of every method (generated with `schemars`); TypeScript types for the app are generated from it. `ibis call <method>` reaches any method from a shell.
- **Bounds and invalidation:** request JSON is at most 1 MiB and request IDs at most 1,024 UTF-8 bytes. Results share a 16 MiB response budget with 8 KiB reserved for ID/envelope overhead; oversized results return `resource-limit` (exit 5). Batches aggregate actual serialized bytes. Library notifications are bounded and coalesced; a sequence gap produces `resync: true` with the newest snapshot, requiring a full library refresh. Refresh failures retain the prior snapshot and report its last successful update time plus a fixed error code.
- **Golden cases** (a new `cases` folder in `fixtures`: library, request, exact expected result) are replayed through every front end that exposes the method and must match byte for byte.

## Desktop app

- **Shell:** Tauri 2 with the system web view; React with React Aria Components for every control (ADR 0021); tokens in `tokens.css`; shared components in `src/ui/`. The app links the engine in-process and calls the same method functions; the web view never touches the clipboard, files or the network.
- **State:** one pure reducer with `useSyncExternalStore` and a controller with session tickets; no state library.
- **Keys:** one command registry (id, default chord per system, user override) drives the keys sheet, footer, menus and docs. Conflicts are checked against Ibis's own keys, a reserved list per system and a real registration attempt. Escape, Tab, Enter and arrows cannot be rebound.
- **Pasting into the previous app:** a pure decision in the engine and thin per-system code. macOS: the app in front is captured (bundle ID, process, name; never window titles) before the picker shows, and after a clean copy hides the picker one ⌘V, keyboard-layout aware, is posted only to that process once it is back in front and the clipboard still holds the citation; guards for terminals, remote desktops and VMs; the password-field guard ADR 0009 requires follows in #1280. Windows: one atomic `SendInput`, refused for elevated targets. Linux copies and says why. Every refusal leaves the citation copied with a reason. macOS ties the Accessibility permission to the signing identity, so builds need a stable signing certificate.
- **Reader:** PDF.js (`pdfjs-dist`, legacy build) behind a small adapter. File bytes arrive over IPC from a command that checks the item and never exposes paths; PDF scripting and eval are off; a plain page-text view backs up accessibility. Ibis keeps the physical page index, the printed page label and an annotation's own page label apart. The printed label comes from the PDF's page labels, a Zotero annotation on that page, or an offset the user confirms; a confirmed offset covers a page range (front matter, missing pages and supplements break a single offset), is tied to the file's content hash and is dropped when the file changes. The record's page range is only a hint.
- **Accessibility:** combobox and listbox (not grid) for results, `TagGroup` for the shelf, one announcer, native announcements after the picker hides. Tested with axe in jsdom, Playwright WebKit snapshots, native accessibility trees and screen-reader runs per release.
- **Language:** the engine returns message codes; the app words them. Fluent arrives with the first translation.
- **Budgets (release builds):** picker interactive p95 ≤ 100 ms after the shortcut, typing p95 ≤ 50 ms at 100,000 references, measured by in-app probes and compared against `main`.

## What gets replaced

`model.rs`, `library.rs`, `settings.rs`, the Zotero and search adapters, the CLI and the desktop library, copy and preferences code are rebuilt on the contracts above. The BibTeX lexer and resolver, date parsing, citation escaping, locator parsing, the native shortcut, panel, clipboard and updater modules, the UI components and tokens, and every test fixture are kept. Settings migrations and identity carry-over code are deleted (ADR 0027).
