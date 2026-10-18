# Roadmap

Each release is usable on its own. Dates are not promised; scope is. Work is tracked in GitHub milestones of the same names.

| Release | Scope |
| --- | --- |
| **0.0 Foundation** | Core library, `ibis` command line tool (search, cite, check, CSL-JSON export), BibTeX/BibLaTeX reading that survives damaged files, key-based outputs (Pandoc, BibLaTeX, LaTeX, Typst), fixture corpus, CI on Windows, macOS and Linux. |
| **0.1 Picker** | Desktop app: global shortcut, search window, copy citation keys or plain citations, choose a library file from the menu bar and reload changes automatically while keeping the last good snapshot. One process for now (ADR 0012). Unsigned preview builds. |
| **0.2 Zotero and styles** | Zotero libraries with Zotero open or closed (personal and group libraries), formatted citations in any official CSL style with preview, page numbers per reference, multiple references, first-run setup. |
| **0.3 Insertion** | Insert into the previously active app, destination profiles, clipboard preservation, explicit copy fallback, compatibility matrix for common editors and browsers. |
| **0.4 Search by meaning** | Local multilingual meaning-based search combined with keyword search, with match explanations. No cloud services. |
| **0.5 Tools and health** | Library health checks (explain-only), RIS and CSL-JSON sources, editor endpoint compatible with Better BibTeX "Cite as you Write", read-only MCP server for AI assistants, optional reference verification. Public beta through Homebrew, winget and Flathub. |
| **1.0** | Accessibility verified with VoiceOver, NVDA and Orca; signed builds; verified updates; documentation site. |

## After 1.0

- Cite a highlight: quote plus page-correct citation from Zotero annotations; notes search.
- Reviewed corrections to libraries through official APIs.
- Out-of-process extensions, then sandboxed WebAssembly extensions.
- Browser extension for web editors; word-processor add-ins.
- Additional reference managers (JabRef, Paperpile exports, Calibre, Bookends and others).
- A web version built from the same core.
