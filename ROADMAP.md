# Roadmap

Each release is usable on its own. Dates are not promised; scope is. Work is tracked in GitHub milestones of the same names.

| Release | Scope |
| --- | --- |
| **0.0 Foundation** | Core library, `ibis` command line tool (search, cite, check, CSL-JSON export), BibTeX/BibLaTeX reading that survives damaged files, key-based outputs (Pandoc, BibLaTeX, LaTeX, Typst), fixture corpus, CI on Windows, macOS and Linux. |
| **0.1 Picker** | A first preview for the maintainer and a few testers, through nightly builds: global shortcut, search window, copy citation keys in Pandoc, BibLaTeX, LaTeX or Typst syntax, choose a library file from the picker or the menu bar and reload changes automatically while keeping the last good snapshot. Read the personal Zotero library directly while Zotero is open (brought forward from 0.2). Survives damaged and hostile files without crashing or hanging. Keyboard and screen-reader basics. One process for now (ADR 0012). Unsigned preview builds for Windows, macOS and Linux. |
| **0.2 Shared preview** | The first version to share widely: the redesigned picker (docs/design.md), the personal Zotero library plus group libraries, first-run setup, Send feedback, and safe in-app updates with separate Stable and Nightly apps (signed update files, asked once on first launch and off until the person agrees, ADR 0019, listed in `PRIVACY.md`). The Windows installer is signed through SignPath Foundation once approved; macOS stays unsigned, with first-launch steps documented. |
| **0.3 Styles and Zotero offline** | Formatted citations with preview (a curated set of common styles first, then any official CSL style), copied as plain and rich text; Zotero libraries while Zotero is closed; page numbers per reference; multiple references. |
| **0.4 Insertion** | Insert into the previously active app, destination profiles, clipboard preservation, explicit copy fallback, compatibility matrix for common editors and browsers. |
| **0.5 Tools and health** | Library health checks (explain-only), RIS and CSL-JSON sources, editor endpoint compatible with Better BibTeX "Cite as you Write", read-only MCP server for AI assistants, optional reference verification. Public beta through Homebrew, winget and Flathub. |
| **1.0** | Accessibility verified with VoiceOver, NVDA and Orca; signed and notarized macOS builds; supply-chain records; documentation site. |

## After 1.0

- Search by meaning: local multilingual meaning-based search combined with keyword search, with match explanations. No cloud services.
- Cite a highlight: quote plus page-correct citation from Zotero annotations; notes search.
- Reviewed corrections to libraries through official APIs.
- Out-of-process extensions, then sandboxed WebAssembly extensions.
- Browser extension for web editors; word-processor add-ins.
- Additional reference managers (JabRef, Paperpile exports, Calibre, Bookends and others).
- A web version built from the same core.
