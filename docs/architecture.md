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
   ├─ ibis-engine   one method list: search, cite, show, styles, check, export…; JSON envelope, errors
   └─ ibis-cli      argument parsing and terminal output over ibis-engine
```

## Principles

- **The engine is the product; front ends are clients.** Every capability lives in the Rust engine behind one method list. The command line exposes all of it, the app uses the same methods, and a parity test fails when one front end can do something the other cannot. No front end formats, resolves or decides anything itself.
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
- **Snapshots are immutable.** A refresh builds a new snapshot and reuses unchanged libraries. Each object has two fingerprints: `cite` (what a citation depends on) and `full` (everything shown). A picker session keeps the snapshot it started from; at copy time each pick is checked by `ItemRef` and `cite` fingerprint, so a refresh only refuses a pick whose key changed or which was deleted.

## Sources (ibis-sources)

- **Capabilities are a fixed list per source kind**, not a plugin system. Zotero open and Zotero closed declare the same list, and a test enforces it; anything only one path can provide is not offered.
- **Zotero:** Ibis maps Zotero's raw item data to CSL itself, from a pinned copy of `zotero-schema`, instead of reading Zotero's `csljson` export (it changes between releases, plugins alter it, and it is the slowest part of a scan). Both access paths fill the same neutral item.
  - **Running:** the local API (needs Zotero's "Allow other applications"). Zotero 10 gives exact incremental refresh; 7–9 use a `dateModified` watermark with a full check on Reload.
  - **Closed, and for a fast start:** a byte copy of `zotero.sqlite` plus its WAL or journal into a private directory, checked for stability, read, then deleted. The live database is never opened. Versions are told apart by schema; a database newer than Ibis knows is refused with a message.
  - **Citation keys, in order:** Zotero's `citationKey` field (Zotero 8+), a `Citation Key:` line in Extra, Better BibTeX's key store (read as a copy). Aliases come from Extra. A key can match zero, one or many items; ambiguous and missing keys are refused for that item.
  - Zotero 10's local write API exists; Ibis stays read-only until an opt-in ADR says otherwise.
- **BibTeX/BibLaTeX:** each entry is parsed independently, so a broken entry produces a warning with its line while the rest loads; work is bounded so hostile files cannot hang the engine. Files are watched and only replaced after two identical reads.
- **CSL-JSON:** `citation-key`, then `id`, is the key.

## Citations (ibis-core, ibis-csl)

- **Request:** ordered items (the same reference may appear twice); a citation form (normal, narrative, author only, year only, full, bibliography only) and "keep my order"; per item a typed locator (one of the 29 CSL 1.0.2 locator terms), rich-text prefix and suffix (italic, bold, sub, superscript), author suppression, and the annotation it came from if any; an output spec or named preset; an input language for parsing typed locators and an output locale.
- **Result:** text, HTML, RTF and a display tree; one outcome per item; typed losses; notices such as "needs the whole document"; a content hash that copy and paste must match.
- **Key syntaxes** are pure Rust, each with a capability table (which fields and forms it carries, its escaping, a fixture checked against its own tool): Pandoc, BibLaTeX (any command, multicite), natbib, LaTeX `\cite`, Typst, org-cite, MyST, MultiMarkdown, AsciiDoc, Jupyter, Jekyll Scholar, Scannable Cite, bare keys, CSL-JSON, Better BibTeX's CAYW JSON.
- **Formatted output:** Zotero's citeproc-js runs in embedded QuickJS (`rquickjs`) on one engine thread, with collation from ICU4X, so the command line and the app give byte-identical output on every system. Any official style and locale; bibliographies run in the background with progress. Before formatting, Ibis applies Zotero's own data conversions (Extra fields, URL stripping for printed articles with pages) so output matches Zotero.

## Engine interface (ibis-engine, ibis-cli)

- **One method list** is the source of truth for the command line, `ibis serve --stdio` (JSON-RPC 2.0, the way Codex's app-server and language servers work), `ibis mcp` for AI agents, and the app.
- **Every `--json` output is a versioned envelope** `{ "ibis": 1, "ok": …, "data" | "error": … }` with stable error codes. Exit codes: 0 ok, 1 findings, 2 usage, 3 bad input, 4 not found, 5 source unavailable, 6 newer format, 70 internal.
- **`ibis schema`** prints the JSON Schema of every method (generated with `schemars`); TypeScript types for the app are generated from it. `ibis call <method>` reaches any method from a shell.
- **Golden cases** (a new `cases` folder in `fixtures`: library, request, exact expected result) are replayed through the command line and the app and must match byte for byte.

## Desktop app

- **Shell:** Tauri 2 with the system web view; React with React Aria Components for every control (ADR 0021); tokens in `tokens.css`; shared components in `src/ui/`. The app links the engine in-process and calls the same method functions; the web view never touches the clipboard, files or the network.
- **State:** one pure reducer with `useSyncExternalStore` and a controller with session tickets; no state library.
- **Keys:** one command registry (id, default chord per system, user override) drives the keys sheet, footer, menus and docs. Conflicts are checked against Ibis's own keys, a reserved list per system and a real registration attempt. Escape, Tab, Enter and arrows cannot be rebound.
- **Pasting into the previous app:** a pure decision in the engine and thin per-system code. macOS: the panel never activates Ibis, so ⌘V goes to the app in front, keyboard-layout aware; guards for Secure Event Input, terminals and remote desktops. Windows: one atomic `SendInput`, refused for elevated targets. Linux copies and says why. Every refusal leaves the citation copied with a reason. macOS ties the Accessibility permission to the signing identity, so builds need a stable signing certificate.
- **Reader:** PDF.js (`pdfjs-dist`, legacy build) behind a small adapter. File bytes arrive over IPC from a command that checks the item and never exposes paths; PDF scripting and eval are off; a plain page-text view backs up accessibility. Page labels come from the PDF, the record's page range or Zotero annotations, always as a suggestion the user confirms once per file.
- **Accessibility:** combobox and listbox (not grid) for results, `TagGroup` for the shelf, one announcer, native announcements after the picker hides. Tested with axe in jsdom, Playwright WebKit snapshots, native accessibility trees and screen-reader runs per release.
- **Language:** the engine returns message codes; the app words them. Fluent arrives with the first translation.
- **Budgets (release builds):** picker interactive p95 ≤ 100 ms after the shortcut, typing p95 ≤ 50 ms at 100,000 references, measured by in-app probes and compared against `main`.

## What gets replaced

`model.rs`, `library.rs`, `settings.rs`, the Zotero and search adapters, the CLI and the desktop library, copy and preferences code are rebuilt on the contracts above. The BibTeX lexer and resolver, date parsing, citation escaping, locator parsing, the native shortcut, panel, clipboard and updater modules, the UI components and tokens, and every test fixture are kept. Settings migrations and identity carry-over code are deleted (ADR 0027).
