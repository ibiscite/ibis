# Roadmap

Each release is usable on its own. Dates are not promised; scope is. Work is tracked in GitHub milestones of the same names.

## Direction

Ibis is meant to be the base layer for everyone who handles references, the way FFmpeg is for media: one engine (`ibis-core`) with a stable interface, and thin front ends on it, first the picker and the `ibis` command line, later an editor language server, an MCP server and extensions. Every ability is an action on a search result or on what is on the clipboard, never a new screen. Ibis works offline first: online features are opt-in, show their results in their own section after the local ones, and cache what they fetch so they keep working offline. Zotero stays the library. Ibis never changes it unless you turn that on, and then only through official APIs. Extensions start as a curated set shipped by the project and open up later. Writing to Zotero, the online mode and extensions each need an accepted ADR (`docs/adr/`) before code.

| Release | Scope |
| --- | --- |
| **0.0 Foundation** | Core library, `ibis` command line tool (search, cite, check, CSL-JSON export), BibTeX/BibLaTeX reading that survives damaged files, key-based outputs (Pandoc, BibLaTeX, LaTeX, Typst), fixture corpus, CI on Windows, macOS and Linux. |
| **0.1 Picker** (released as 0.1.0 and 0.1.1) | The first preview: a global shortcut opens the picker; search a `.bib` file or Zotero while it is open, and copy citation keys or a citation in one of five styles. Opt-in, signed in-app updates, with a separate Ibis Nightly app. See the [changelog](CHANGELOG.md). |
| **0.2 Shared preview** | The first version to share widely, finishing what 0.1 started: first-run setup, the rest of the redesign (result rows that tell similar references apart and show their library, `?` for the keyboard shortcuts, a footer that says what Enter will do, useful actions when nothing matches), the accessibility baseline, search that stays fast on large libraries, and finding a reference by pasting its DOI, PubMed ID, ISBN or arXiv ID (bare or as a link). A quality pass before sharing: bugs, rough edges and review follow-ups, no new features. The Windows installer is signed through SignPath Foundation once approved; macOS stays unsigned, with first-launch steps documented. The app identifier moves from `io.github.ibiscite.ibis` to `com.ibiscite.ibis` (a domain the project owns) with a one-time settings migration, while few people have Ibis installed. |
| **0.3 Styles and Zotero offline** | Any official CSL style beyond the first five, with an always-visible preview, prefix, suffix and author omission, and plain plus rich text (including RTF) on the clipboard; page numbers typed in the picker's search, and other locator types; multiple references; a warning when a Zotero citation key changes; Zotero libraries while Zotero is closed, with a warning when Zotero's data sits in a cloud-synced folder. "Is this in my library?": paste a DOI, PubMed ID, ISBN or link and see the matching reference with its actions, or that it is missing, with a hand-off to Zotero to add it. Collections shown on rows and usable as a filter. A German interface and a page for people coming from Citavi; GB/T 7714-2025 among the curated styles, with the citation language shown. Windows and Linux ARM64 builds and a minimum-OS table. |
| **0.4 Insertion** | The action registry: each action declares what it accepts (a reference, an identifier, a file, text), whether it needs the network or writes, and its shortcut; built-in actions, the command line and later extensions share it. Insert into the previously active app, destination profiles, clipboard preservation, explicit copy fallback, compatibility matrix for common editors and browsers. A shortcut command for Wayland and other desktops (`ibis toggle`). Small picker conveniences: open a reference in Zotero or its PDF at the cited page, recently cited and recently added, share templates (reference plus DOI link). A session bibliography, a slides profile and more output templates, plus paste profiles for WPS Office and Hangul. Copy with a reason inside remote desktops. |
| **0.5 Tools and health** | A documented public contract for the command line's JSON output and the editor endpoint, then: library health checks (explain-only), RIS and CSL-JSON sources, editor endpoint compatible with Better BibTeX "Cite as you Write", read-only MCP server for AI assistants. The online mode (opt-in, one network ADR): metadata lookup for an identifier that is not in your library, reference verification and retraction warnings, and a read-only Zotero web library source; online results never delay or reorder local ones and are cached for offline use. Adding a missing item to Zotero directly, opt-in, under an ADR on writing through Zotero's API. Citation formatting moves into a shared core so the command line and integrations can format. Settings portable through a folder the person already syncs. An installer IT departments can deploy. Public beta through Homebrew, winget and Flathub. |
| **1.0** | Accessibility verified with VoiceOver, NVDA and Orca; signed and notarized macOS builds; supply-chain records; documentation site. |

## After 1.0

- Search by meaning: local multilingual meaning-based search combined with keyword search, with match explanations. No cloud services.
- Cite a highlight: quote plus page-correct citation from Zotero annotations; notes search.
- Reviewed corrections to library records through official APIs, opt-in.
- Extensions: a curated set shipped by the project first (out of process), then sandboxed WebAssembly extensions and an open catalogue.
- Browser extension for web editors.
- A citation language server (`ibis lsp`) for code editors.
- `CITATION.cff` and Hayagriva sources; Kindle highlights and browser bookmarks as sources.
- An experiment with Zotero-compatible citations in Word documents.
- An iOS keyboard (needs the paid Apple developer account).
- Additional reference managers (JabRef, Paperpile exports, Calibre, Bookends and others).
- A web version built from the same core (CI checks from 0.5 that the core builds for WebAssembly).

## Not planned

Ibis is the fast front door to your library, not a reference manager. These stay out unless the project's direction changes: managing libraries inside Ibis (organising, merging, attaching files: Zotero does this; Ibis changes a library only when you turn that on, and then only through official APIs, starting with adding a missing item and reviewed corrections), live citation fields in Word or Google Docs (Zotero's own plugins do this), AI writing, legal citation short forms, and an Ibis plugin for Zotero.
