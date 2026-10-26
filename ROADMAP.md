# Roadmap

Each release is usable on its own. Dates are not promised; scope is. Work is tracked in the GitHub milestone with the same version number. Ideas outside these themes wait in Later.

## Direction

Ibis is one shortcut to cite from your library in any app. It covers what Zotero's citation dialog and Better BibTeX offer for picking references, getting citation keys, formatting citations and pasting them, for first-time and expert users alike, and goes further: a built-in reader to cite the exact page, citations that never silently go wrong, and an engine that people, scripts and AI agents can drive. Nobody should need to open Zotero while citing.

- **Zotero first, open to all.** Zotero users are the first audience, whether Zotero is open or closed, with or without Better BibTeX. BibTeX, BibLaTeX and CSL-JSON files work the same way, so other reference managers reach Ibis through their exports. Ibis never reads another app's private cloud account and never writes to a library.
- **Engine first.** The `ibis` command line is the engine: it can do everything, with `--json`, a JSON-RPC server mode and an MCP server for AI agents. The app is a fast, friendly client of the same engine ([ADR 0028](docs/adr/0028-engine-first.md), [architecture](docs/architecture.md)).
- **Works out of the box.** No setup questions: the output follows the app you are writing in, setup steps finish themselves, and every extra is one key away when you need it ([design](docs/design.md)). Deep customisation is there for those who want it, never required.
- **Only what Ibis can get right.** A plausible but wrong citation is worse than none. Where correctness needs the whole document (numbering, ibid., 2020a and 2020b), Ibis gives what is correct on its own instead and says why. Every feature passes two questions: can Ibis get it right from what it knows, and does it save real work?
- **Offline by default.** Any online feature is opt-in and needs a network ADR.
- **Long-lived foundations.** Open standards (CSL, CSL-JSON, BibTeX) and widely used open source (Zotero's citeproc-js, SQLite, PDF.js, Tauri, React Aria), each behind one Ibis module so it can be replaced.

Until 1.0 nothing is kept for compatibility (ADR 0027): anything that is wrong is rebuilt.

| Release | Scope |
| --- | --- |
| **0.0 Foundation** and **0.1 Picker** (released) | The first command line, BibTeX reading that survives damaged files, key outputs, and a preview picker with five styles and signed in-app updates. See the [changelog](CHANGELOG.md). |
| **0.2 The new engine** | The rebuild: one engine for the command line and the app. Zotero read the same way whether it is open or closed (Zotero 7–10, Better BibTeX keys, group libraries), `.bib` and CSL-JSON files, attachments, notes and annotations in the model. One citation request and result for every output; all key syntaxes Ibis can write correctly; formatted citations in the engine with the five current styles. The command line does everything with `--json`, `ibis schema` and `ibis serve --stdio`. Today's picker running on the engine, with output chosen from the app you are in, setup that finishes itself, forgiving search and a picker that opens in under 100 ms. Installers stay unsigned on Windows with first-launch steps. |
| **0.3 Straight into your document** | Enter pastes into the app you were writing in, on macOS and Windows, on by default with a switch to turn it off; a copy with the reason whenever pasting is unsafe (terminals, remote desktops, password fields, elevated apps). Linux copies and says why. A stable macOS signing identity so the paste permission survives updates. Rich text (RTF and HTML) on the clipboard, recent citations per app, changing the last citation, "Selected in Zotero". |
| **0.4 Build any citation** | Everything Zotero's dialog does to a citation, visible in place: several references, reorder, all 29 locator types, prefix and suffix with italics, citation forms (narrative, author only, year only, full), keep my order. Any official CSL style and locale; output presets; bibliography entries. A Settings window with customisable shortcuts and presets, and a documented config file. `ibis mcp` for AI agents and an editor endpoint compatible with Better BibTeX's Cite as you Write. Public beta, tested with VoiceOver and NVDA. |
| **0.5 Browse and read** | Collections, tags, saved searches and filters; suggestions before typing. A read-only reader for PDFs, notes and Zotero annotations inside Ibis: choose the page to cite, with the page number's source shown and confirmed once per file. Citing an annotation cites its page. "Is this in my library?" for a pasted identifier or reference. |
| **0.6 Your document** | Export what a document cites to `.bib`, Hayagriva or CSL-JSON; a session bibliography; `ibis check` on a manuscript (`.md`, `.qmd`, `.tex`, `.typ`); opt-in DOI lookup for references not in the library (network ADR first); a hint to use Zotero's own plugin in documents that already have live Zotero citations. |
| **1.0 Stable** | Accessibility verified with VoiceOver, NVDA and Orca; signed Windows builds and signed, notarized macOS builds; a versioned engine contract; Flathub, Homebrew and winget; documentation site. |

## After 1.0, or when people ask

- Adding a missing item to Zotero through its official local write API (opt-in ADR).
- Pasting on Linux desktops and per-app paste rules.
- A language server (`ibis lsp`), launcher integrations, a browser extension.
- EPUB and web snapshots in the reader; PDF links from `.bib` files.
- Interface translations, starting with German; RIS and EndNote XML files; ARM64 builds.
- Search by meaning, combined with keyword search.

## Not planned

Ibis is the fast front door to your library, not a reference manager: no organising, merging or attaching files (Zotero does this), no citation key generation, export translators or auto-export (Better BibTeX does this), no live citation fields in Word or Google Docs (Zotero's plugins do this), no reading other reference managers' private databases or cloud accounts, no AI writing, no legal citation short forms and no Ibis plugin for Zotero.
