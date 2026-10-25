# Roadmap

Each release is usable on its own. Dates are not promised; scope is. Work is tracked in the GitHub milestone with the same version number; ideas outside these themes wait in Later.

## Direction

Ibis is the one-shortcut way to cite from Zotero or a `.bib` file in any app that has no citation plugin. It works offline and keeps the citation keys and bibliography your document depends on correct. The first people it serves are Zotero users who write in plain text (Markdown, Pandoc, Quarto, LaTeX, Typst, Obsidian), then people writing in Notion, Google Docs and email.

Zotero stays the library: Ibis never changes it unless you turn that on, and then only through official APIs. Ibis works offline first; any online feature is opt-in and needs a network ADR. One engine (`ibis-core`) serves the picker and the `ibis` command line.

**Core first (2026-10-25).** Each release has one theme and finishes it before the next starts. Until 1.0 the picker keeps the look it has: one shared design with the system accent, with Liquid Glass on supported macOS and Acrylic on supported Windows. Further per-platform depth, accents and metrics come after 1.0. Ideas outside the themes below wait in the Later milestone until people ask for them.

| Release | Scope |
| --- | --- |
| **0.0 Foundation** | Core library, `ibis` command line tool (search, cite, check, CSL-JSON export), BibTeX/BibLaTeX reading that survives damaged files, key-based outputs (Pandoc, BibLaTeX, LaTeX, Typst), fixture corpus, CI on Windows, macOS and Linux. |
| **0.1 Picker** (released as 0.1.0 and 0.1.1) | The first preview: a global shortcut opens the picker; search a `.bib` file or Zotero while it is open, and copy citation keys or a citation in one of five styles. Opt-in, signed in-app updates, with a separate Ibis Nightly app. See the [changelog](CHANGELOG.md). |
| **0.2 Cite anywhere, trust the key** | The first version to share: install, choose a library, find, copy and recover on macOS, Windows and Linux. First-run setup, result rows that tell similar references apart, `?` for the shortcuts, a footer that says what Enter will do, page numbers typed in the search, finding a reference by pasting its DOI, PubMed ID, ISBN or arXiv ID, `ibis toggle` and the Wayland shortcut portal for desktops where the in-app shortcut cannot work, the accessibility baseline and search that stays fast on large libraries. A quality pass before sharing. Installers stay unsigned on Windows and macOS, with first-launch steps documented. The app identifier moves to `com.ibiscite.ibis` with a one-time settings migration. |
| **0.3 Your document's bibliography** | Export what a document cites to `.bib`, Hayagriva or CSL-JSON, and a session bibliography. Any official CSL style with prefix, suffix and author omission, and editing the locator of a reference already added, and plain plus rich text on the clipboard. A warning when a citation key changes. Zotero libraries while Zotero is closed, preferring Zotero's local API whenever it runs, with a warning when its data sits in a cloud-synced folder. A minimal Settings window with a customizable shortcut. |
| **0.4 Check before you submit** | A documented, stable command line contract, then `ibis check` on a manuscript (`.md`, `.qmd`, `.tex`, `.typ`): keys that do not resolve or have changed, and a pasted reference list matched against the library. "Is this in my library?" for a pasted identifier. An editor endpoint compatible with Better BibTeX "Cite as you Write", so existing editor integrations work with Zotero closed. |
| **1.0 Stable** | Accessibility verified with VoiceOver, NVDA and Orca; signed Windows builds and signed, notarized macOS builds; supply-chain records; package managers (Homebrew, winget, Flathub); documentation site. |

## After 1.0, or when people ask

- Per-platform depth, native accents and metrics beyond what has shipped: GNOME and Windows system accents, versioned macOS radii, GNOME icon weight.
- Inserting into the previously active app, destination profiles and clipboard preservation (copy and paste works everywhere today).
- An opt-in online mode: metadata for identifiers missing from the library, reference existence checks and retraction warnings.
- A read-only MCP server and a language server (`ibis lsp`); formatting citations outside the window.
- Adding missing items to Zotero and reviewed corrections, through official APIs.
- Library health checks, RIS and CSL-JSON sources, portable settings, an installer IT departments can deploy, ARM64 builds.
- Interface translations, starting with German, and more curated styles such as GB/T 7714-2025.
- Search by meaning, citing a highlight, notes search, extensions, a browser extension, a web version, an iOS keyboard, other reference managers.

## Not planned

Ibis is the fast front door to your library, not a reference manager. These stay out unless the project's direction changes: managing libraries inside Ibis (organising, merging, attaching files: Zotero does this; Ibis changes a library only when you turn that on, and then only through official APIs, starting with adding a missing item and reviewed corrections), live citation fields in Word or Google Docs (Zotero's own plugins do this), AI writing, legal citation short forms, and an Ibis plugin for Zotero.
