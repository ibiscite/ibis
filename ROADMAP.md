# Roadmap

Each release is usable on its own. Dates are not promised; scope is. Work is tracked in the GitHub milestone with the same version number; ideas outside these themes wait in Later.

## Direction

Ibis is the one-shortcut way to cite from Zotero or a `.bib` file in any app that has no citation plugin. It works offline and keeps the citation keys and bibliography your document depends on correct. The first people it serves are Zotero users who write in plain text (Markdown, Pandoc, Quarto, LaTeX, Typst, Obsidian), then people writing where no citation plugin reaches: Word for the web, Pages, Notion and email.

Zotero stays the library: Ibis never changes it unless you turn that on, and then only through official APIs. Ibis works offline first; any online feature is opt-in and needs a network ADR. One engine (`ibis-core`) serves the picker and the `ibis` command line.

Ibis is open to every setup by reading open formats and writing through standard channels. It reads Zotero, BibTeX and BibLaTeX, and CSL-JSON; other reference managers reach it through their exports, never through their private databases or cloud accounts. It writes to the clipboard (plain text and HTML today, RTF from 0.4), into the app you were writing in from 0.3, and, from 0.5, to editors through a stable command line and a Cite as you Write endpoint. Where a document already has live Zotero citations (Word, LibreOffice, Google Docs), Zotero's own plugin is the right tool and Ibis says so. Formatting follows CSL 1.0.2 with citeproc-js, the engine Zotero uses. What works where is in [compatibility](docs/compatibility.md).

**Only what Ibis can get right.** A plausible but wrong citation is worse than none. Where correctness needs the whole document (citation numbers, ibid., 2020a and 2020b), Ibis gives what is correct on its own instead, such as the reference entry, a full note or the key, and says why; numeric styles move to this in 0.2 ([#1129](https://github.com/ibiscite/ibis/issues/1129)). Every feature passes two questions: can Ibis get it right from what it knows, and does it save real work?

**Core first (2026-10-25).** Each release has one theme and finishes it before the next starts. Until 1.0 the picker keeps the look it has: one shared design with the system accent, with Liquid Glass on supported macOS and Acrylic on supported Windows. Further per-platform depth, accents and metrics come after 1.0. Ideas outside the themes below wait in the Later milestone until people ask for them.

| Release | Scope |
| --- | --- |
| **0.0 Foundation** | Core library, `ibis` command line tool (search, cite, check, CSL-JSON export), BibTeX/BibLaTeX reading that survives damaged files, key-based outputs (Pandoc, BibLaTeX, LaTeX, Typst), fixture corpus, CI on Windows, macOS and Linux. |
| **0.1 Picker** (released as 0.1.0 and 0.1.1) | The first preview: a global shortcut opens the picker; search a `.bib` file or Zotero while it is open, and copy citation keys or a citation in one of five styles. Opt-in, signed in-app updates, with a separate Ibis Nightly app. See the [changelog](CHANGELOG.md). |
| **0.2 Cite anywhere, trust the key** | The first version to share: install, choose a library, find, copy and recover on macOS, Windows and Linux. First-run setup, result rows that tell similar references apart, `?` for the shortcuts, a footer that says what Enter will do, page numbers typed in the search, finding a reference by pasting its DOI, PubMed ID, ISBN or arXiv ID, `ibis toggle` and the Wayland shortcut portal for desktops where the in-app shortcut cannot work, the accessibility baseline and search that stays fast on large libraries. A quality pass before sharing. Installers stay unsigned on Windows and macOS, with first-launch steps documented. The app identifier moves to `com.ibiscite.ibis` with a one-time settings migration. |
| **0.3 Straight from Zotero to your document** | Zotero libraries while Zotero is closed: Ibis reads a private read-only copy of Zotero's database and prefers Zotero's local API whenever it runs, with a warning when the data sits in a cloud-synced folder. The citation is pasted into the app you were writing in, on macOS first and then Windows: on by default with a switch to turn it off, the citation left on the clipboard, and a copy with the reason whenever pasting cannot be done safely (terminals, remote desktops, password fields). Linux copies and says why. Restoring the previous clipboard waits until it is proven safe. |
| **0.4 Your document's bibliography** | Export what a document cites to `.bib`, Hayagriva or CSL-JSON, and a session bibliography. Any official CSL style with prefix, suffix and author omission, and editing the locator of a reference already added. A warning when a citation key changes. CSL-JSON files as libraries, Zotero's own citation keys (Zotero 8 and later) and alias keys. Bibliography entries and RTF on the clipboard, and org-cite, MyST and natbib key forms. Formatting moves into the core so the picker and the command line produce the same text. A minimal Settings window with a customizable shortcut. |
| **0.5 Check before you submit** | A documented, stable command line contract, then `ibis check` on a manuscript (`.md`, `.qmd`, `.tex`, `.typ`): keys that do not resolve or have changed, and a pasted reference list matched against the library. "Is this in my library?" for a pasted identifier. An editor endpoint compatible with Better BibTeX "Cite as you Write", on Ibis's own local port, so existing editor integrations work with Zotero closed. |
| **1.0 Stable** | Accessibility verified with VoiceOver, NVDA and Orca; signed Windows builds and signed, notarized macOS builds; supply-chain records; Flathub as the main Linux channel, plus Homebrew and winget; documentation site. |

## After 1.0, or when people ask

- Per-platform depth, native accents and metrics beyond what has shipped: GNOME and Windows system accents, versioned macOS radii, GNOME icon weight.
- Pasting on Linux desktops, and destination profiles for apps that need their own paste rules.
- An opt-in online mode: metadata for identifiers missing from the library, reference existence checks and retraction warnings.
- A read-only MCP server and a language server (`ibis lsp`); formatting citations outside the window.
- Adding missing items to Zotero and reviewed corrections, through official APIs.
- Library health checks, RIS and EndNote XML files, portable settings, an installer IT departments can deploy, ARM64 builds.
- Interface translations, starting with German, and more curated styles such as GB/T 7714-2025.
- Search by meaning, citing a highlight, notes search, extensions, a browser extension, a web version, an iOS keyboard.

## Not planned

Ibis is the fast front door to your library, not a reference manager. These stay out unless the project's direction changes: managing libraries inside Ibis (organising, merging, attaching files: Zotero does this; Ibis changes a library only when you turn that on, and then only through official APIs, starting with adding a missing item and reviewed corrections), live citation fields in Word or Google Docs (Zotero's own plugins do this), reading other reference managers' private databases or cloud accounts (their exports work instead), AI writing, legal citation short forms, and an Ibis plugin for Zotero.
