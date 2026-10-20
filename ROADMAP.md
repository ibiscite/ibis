# Roadmap

Each release is usable on its own. Dates are not promised; scope is. Work is tracked in GitHub milestones of the same names.

| Release | Scope |
| --- | --- |
| **0.0 Foundation** | Core library, `ibis` command line tool (search, cite, check, CSL-JSON export), BibTeX/BibLaTeX reading that survives damaged files, key-based outputs (Pandoc, BibLaTeX, LaTeX, Typst), fixture corpus, CI on Windows, macOS and Linux. |
| **0.1 Picker** | A first preview for the maintainer and a few testers, through nightly builds: global shortcut, search window, copy citation keys in Pandoc, BibLaTeX, LaTeX or Typst syntax, choose a library file from the picker or the menu bar and reload changes automatically while keeping the last good snapshot. Read the personal Zotero library directly while Zotero is open (brought forward from 0.2). Survives damaged and hostile files without crashing or hanging. Keyboard and screen-reader basics. One process for now (ADR 0012). Unsigned preview builds for Windows, macOS and Linux. |
| **0.2 Shared preview** | The first version to share widely: the redesigned picker (docs/design.md), the personal Zotero library plus group libraries, first-run setup, Send feedback, and safe in-app updates with separate Stable and Nightly apps (signed update files, asked once on first launch and off until the person agrees, ADR 0019, listed in `PRIVACY.md`). The Windows installer is signed through SignPath Foundation once approved; macOS stays unsigned, with first-launch steps documented. Result rows tell similar references apart and show which library they come from; `?` shows the keyboard shortcuts, and the footer says what Enter will do. Searching works correctly in non-Latin scripts and stays fast on large libraries. |
| **0.3 Styles and Zotero offline** | Formatted citations with an always-visible preview (a curated set of common styles first, then any official CSL style), with prefix, suffix and author omission, copied as plain and rich text together; page numbers typed in the search, and other locator types; multiple references; Zotero's own citation keys (its Citation Key field first, then a key in Extra, then Better BibTeX, with a warning when a key changes); Zotero libraries while Zotero is closed, with a warning when Zotero's data sits in a cloud-synced folder. A German interface and a page for people coming from Citavi; GB/T 7714-2025 among the curated styles, with the citation language shown. Windows and Linux ARM64 builds and a minimum-OS table. |
| **0.4 Insertion** | Insert into the previously active app, destination profiles, clipboard preservation, explicit copy fallback, compatibility matrix for common editors and browsers. A shortcut command for Wayland and other desktops (`ibis toggle`). Small picker conveniences: open a reference in Zotero or its PDF at the cited page, find a reference by DOI, PMID, ISBN or arXiv ID, recently cited and recently added. A session bibliography, a slides profile and more output templates, plus paste profiles for WPS Office and Hangul. Copy with a reason inside remote desktops. |
| **0.5 Tools and health** | A documented public contract for the command line's JSON output and the editor endpoint, then: library health checks (explain-only), RIS and CSL-JSON sources, editor endpoint compatible with Better BibTeX "Cite as you Write", read-only MCP server for AI assistants, optional reference verification and retraction warnings (one network ADR). Citation formatting moves into a shared core so the command line and integrations can format. An optional read-only Zotero web library source (its own network ADR). Settings portable through a folder the person already syncs. An installer IT departments can deploy. Public beta through Homebrew, winget and Flathub. |
| **1.0** | Accessibility verified with VoiceOver, NVDA and Orca; signed and notarized macOS builds; supply-chain records; documentation site. |

## After 1.0

- Search by meaning: local multilingual meaning-based search combined with keyword search, with match explanations. No cloud services.
- Cite a highlight: quote plus page-correct citation from Zotero annotations; notes search.
- Reviewed corrections to libraries through official APIs.
- Out-of-process extensions, then sandboxed WebAssembly extensions.
- Browser extension for web editors.
- A citation language server (`ibis lsp`) for code editors.
- `CITATION.cff` and Hayagriva sources; Kindle highlights and browser bookmarks as sources.
- An experiment with Zotero-compatible citations in Word documents.
- An iOS keyboard (needs the paid Apple developer account).
- Additional reference managers (JabRef, Paperpile exports, Calibre, Bookends and others).
- A web version built from the same core (CI checks from 0.5 that the core builds for WebAssembly).

## Not planned

Ibis is a citation picker, not a reference manager. These stay out unless the project's direction changes: managing libraries (up to 1.0 Ibis never writes to Zotero or `.bib` files; the only change planned after that is reviewed corrections through official APIs, ADR 0006), live citation fields in Word or Google Docs (Zotero's own plugins do this), AI writing, legal citation short forms, and an Ibis plugin for Zotero.
