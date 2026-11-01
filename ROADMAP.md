# Roadmap

Every feature planned here ships before 1.0. The version numbers before 1.0 are working steps and may change; each release is usable on its own, and each milestone on GitHub has the same name. 1.0 adds no features: people test everything, then the maintainer moves to 1.0 by hand. Dates are not promised; scope is.

## Direction

Ibis sits between the references people already keep and the apps they already write in, for first-time and expert users alike, on every major operating system and in many languages.

- **Keep the person's sources and workflow.** Zotero with or without Better BibTeX, `.bib` files kept by hand, BibDesk, spreadsheets and other reference managers are all valid starting points, reached through documented integrations, exports and the person's own files. Ibis states which formats it supports, never needs its own library and never writes to a source library or reads another app's private cloud account.
- **Every writing app; citation keys first.** LaTeX and Overleaf come first, with Markdown, Quarto and Typst given equal care. Formatted citations for Word, Google Docs and other apps stay fully supported. Ibis respects the citation syntax and bibliography the person already uses.
- **Engine first.** The `ibis` command line can do everything, with `--json`, a JSON-RPC server and an MCP server for AI agents. The app is a fast client of the same engine ([decision 0028](docs/decisions.md#0028-engine-first-replaces-0003-and-0005), [architecture](docs/architecture.md)).
- **Defaults the person controls.** A global default, remembered choices per app, a sensible Automatic mode and a one-time override. Ibis shows what it will paste, never changes a setting silently, and finishes setup steps by itself ([design](docs/design.md)).
- **Only what Ibis can get right.** A plausible but wrong citation is worse than none. Where correctness needs the whole document (numbering, ibid., 2020a and 2020b), Ibis gives what is correct on its own and says why. Every feature must be right from what Ibis knows and save real work.
- **Languages and platforms count.** Names, search, input methods, text direction and citation locales work on Windows, macOS and Linux; limits are written down, not hidden.
- **Offline by default.** Any online feature is opt-in and needs a recorded network decision and the maintainer's yes ([decision 0008](docs/decisions.md#0008-offline-by-default)).
- **Long-lived foundations.** Open standards (CSL, CSL-JSON, BibTeX) and widely used open source (citeproc-js, SQLite, PDF.js, Tauri, React Aria), each behind one Ibis module so it can be replaced.

Each release claims only what it has demonstrated, in the [compatibility guide](docs/compatibility.md). Until 1.0 nothing is kept for compatibility (decision 0027): anything wrong is rebuilt.

## Releases

### 0.0 Foundation and 0.1 Picker (released)

The first command line, BibTeX reading that survives damaged files, key outputs, and a preview picker with five styles and signed in-app updates. See the [changelog](CHANGELOG.md).

### 0.2 The new engine (released)

- One engine for the command line and the app. Zotero read through its local API while it runs and from a private copy of its database while it is closed (Zotero 7–10, Better BibTeX keys with conflicts shown, group libraries); `.bib` and CSL-JSON files.
- One citation request and result for every output; today's key syntaxes; formatted citations in the engine with five styles, checked against Zotero's own output.
- The command line does everything with `--json`, `ibis schema` and `ibis serve --stdio`. The picker runs on the engine and opens in under 100 ms.

### 0.3 Straight into your document

- **The first citation with almost no setup.** The output follows the app you are writing in and the footer shows what Enter will do ("Paste into Word · APA"); one key changes it just this once or for that app, with a way back to automatic. The first paste into a browser, code editor or unknown app asks once, and can keep that app to copying. Setup finishes itself, including a permission detour that keeps the prepared citation. Sample references when there is no library, the same invented set as the website and help.
- **Enter pastes** into that app on macOS and Windows, on by default with a switch to turn it off. Enter pressed while results load runs once. Ibis copies instead, and says why, wherever pasting is unsafe: terminals, remote desktops and viewers, editors whose paste key does something else, elevated apps, secure typing on macOS and password fields on Windows. Any other app can be set to "copy only here". A pending paste is cancelled when the computer sleeps, locks or switches user. A one-time note per word processor says that documents using a reference manager's plugin should insert with it. After the window closes a short receipt says what happened; the last outcome stays in More.
- **Linux** copies and says why. On Wayland the shortcut comes from the desktop portal, or Ibis shows the exact binding to add.
- **Updates** keep the paste permission on macOS through a stable self-signed identity, and an update never opens the picker.
- **Citations:** rich text (RTF and HTML) on the clipboard, Copy as Plain Text and Copy Reference as Markdown; recent citations per app; changing the last citation as an explicit action that replaces only a selected citation; a page or page range; narrative and author-omitted citations. Short selected text becomes the search, so the paste replaces a placeholder like "smith 2020". A peek at a reference's full details; a `.bib` or CSL-JSON file dropped on the window becomes a library.
- **For scripts and AI agents** ([decision 0033](docs/decisions.md#0033-the-engine-for-agents-and-tools)): `ibis mcp` speaks MCP 2026-07-28 and still answers 2025 clients ([#1591](https://github.com/ibiscite/ibis/issues/1591)); an Agent Skill in one plugin for Claude Code and Codex; `ibis` uses the libraries chosen in the app when none are named; the app installs the matching `ibis` command; a Homebrew tap and winget.
- **Release gates** ([decision 0032](docs/decisions.md#0032-release-gates)): the whole citation works with VoiceOver and NVDA and with Chinese, Japanese and Korean input methods; interface text is ready for translation; speed and resource limits are checks that run.

### 0.4 Build any citation

- **Everything Zotero's citation dialog does, visible in place:** several references, reordering (also by dragging), all 29 locator types behind one searchable control with ranges and a literal fallback, prefix and suffix with italics, citation forms (full, year only, mixed within one citation) and keep my order.
- **Every key syntax Ibis can write correctly** (Typst, org-cite, MyST, AsciiDoc, ConTeXt and more), each with its capability table. **Any official CSL style and locale**, output presets, bibliography entries, and "References used this session" as an editable bibliography.
- **The project's own bibliography as the authority:** results say "In thesis.bib" or "Missing from thesis.bib", and a missing entry can be copied with its existing key.
- **Overleaf-only bibliographies** on free plans without Git or Dropbox: after one setup step (the project link, a dropped download or a pasted bibliography), one key opens the project to download it, and Ibis reads every `.bib` in that file or project ZIP. A newer download with the same name that keeps most keys replaces it with a footer note; otherwise Ibis asks and lists the keys that would go. It keeps its last good copy and needs no sign-in or network use. A browser extension that reads the open Overleaf project's bibliography and inserts keys there, only if Overleaf agrees in writing.
- **More sources:** RIS files (the export of EndNote Web, Citavi Web and many research portals) for formatted citations, since RIS has no keys; a CSV template for references kept in a spreadsheet; a setup line for each reference manager in the compatibility guide, saying which keep their export current.
- **Live citations, where they work:** for Zotero items, markers in LibreOffice `.odt` files that the ODF Scan add-on turns into live citations; trials of pasting real footnotes for note styles (Word, Pages, LibreOffice) and live Zotero citations into Word through Zotero's own field. Each ships only if it works.
- **Settings:** every shortcut can be changed, presets, connected libraries, and a documented settings file to back up and restore on another computer (for example through iCloud Drive or OneDrive); paste permissions stay on each computer ([#1561](https://github.com/ibiscite/ibis/issues/1561)). One rotating tip on the empty search that retires once you use what it teaches, with a switch to turn tips off. An opt-in "Download quietly, install when I quit" for updates.
- **Links and integrations:** drag the citation from the shelf into any app; `ibis://` links that open the picker filled in, with Shortcuts (Spotlight) and Alfred recipes; Show in Zotero and Open PDF in Zotero at This Page; an editor endpoint compatible with Better BibTeX's Cite as You Write, claimed once a named editor passes.
- **For scripts and agents:** source health on every result, citation receipts and a strict mode; citing CSL-JSON a caller sends; the command line picks the output for the app you name; the MCP Registry listing.
- **The interface in German and one right-to-left language.** Public beta.

### 0.5 Find the passage

- **Search** that forgives the remaining cases (Chinese, Japanese and Korean names, romanised readings); collections, tags, saved searches and filters; "Selected in Zotero"; suggestions before typing.
- **Saved passages:** Zotero highlights, notes and personal comments searchable from the picker, each result showing the passage, the separate comment, the reference and the page. Open Passage returns to the exact annotation or a verified PDF location. Use Reference and Page fills in both, shown as "PDF page 8 · cite p. 417", with the printed page taken from the PDF, the annotation or an offset the person confirms, never assumed.
- **Quotes:** a PDF dropped on the window finds its reference; Copy Quote with Reference as plain text or Markdown, with the quotation, known page, reference and source link, and comments kept separate. Selected passages and comments exported as one Markdown evidence brief ([#1517](https://github.com/ibiscite/ibis/issues/1517)).
- **Is this in my library?** for a pasted identifier, reference or reference list.
- **Attachments:** results say whether a reference has a PDF or a saved web page, with the PDF's page count where it can be read safely.
- **A read-only reader inside Ibis** for PDFs, EPUB and web snapshots, for what the reader people already use cannot do ([#1252](https://github.com/ibiscite/ibis/issues/1252)).

### 0.6 Your document

- Export what a document cites to `.bib`, Hayagriva or CSL-JSON.
- `ibis check` on a manuscript (`.md`, `.qmd`, `.tex`, `.typ`) as a build gate that also catches citations typed without Ibis, with an optional lock file so co-authors get the same references.
- A warning when a reference's citation key changed since it was last copied ([#1092](https://github.com/ibiscite/ibis/issues/1092)).
- Opt-in DOI lookup for references not in the library (network decision first).

### 0.7 Everywhere you work

- Pasting on Linux desktops, and per-app paste rules.
- A language server (`ibis lsp`), a Raycast extension and other launcher extensions, and a browser extension.
- A library on another computer, over the person's own SSH ([#1542](https://github.com/ibiscite/ibis/issues/1542)).
- Adding a missing item to Zotero through its official local write API (opt-in, decided first).
- More interface translations; EndNote XML and Word Sources.xml files; ARM64 builds.
- Search by meaning, combined with keyword search.
- Short research briefs, then builds where Ibis can get them right: decision memory, resuming research and an AI companion.

### 1.0 Stable

No new features. People test everything, then the maintainer moves to 1.0. The release gates: accessibility verified with VoiceOver, NVDA and Orca; a versioned engine contract proved by real clients; Flathub, Homebrew and winget, with Linux support stated per desktop, display server and package; the documentation site; licence notices and supply-chain records. macOS keeps its stable self-signed identity and Windows installers stay unsigned with first-launch steps: 1.0 needs no paid signing (decision 0032).

## Saved passages and project checks

These five workflows were approved on 2026-10-30 and all ship before 1.0. The first three come first; each starts with the smallest path that is reliable, and a slice that cannot yet be proved is narrowed rather than allowed to weaken its claims. Detail and limits are in the [research](docs/source-workflow-research.md); interaction rules in [design](docs/design.md#source-checks-and-passage-reuse-planned).

1. **Check the project's bibliography** ([#1377](https://github.com/ibiscite/ibis/issues/1377)): choose a read-only `.bib`; results say "In thesis.bib", "Missing from thesis.bib" or a specific unknown or conflict state. Copy a safe missing entry with its existing key, or explain how to export it from the reference manager.
2. **Search highlights and comments, then open the exact passage** ([#54](https://github.com/ibiscite/ibis/issues/54), [#359](https://github.com/ibiscite/ibis/issues/359)).
3. **Copy a quote with its reference** ([#1385](https://github.com/ibiscite/ibis/issues/1385)), with personal comments kept separate.
4. **Export selected passages as an evidence brief** ([#1517](https://github.com/ibiscite/ibis/issues/1517)): one new Markdown file with quotations, separate comments, references, pages and source links; no notebook or publishing service.
5. **Warn when a previously copied key changes** ([#1092](https://github.com/ibiscite/ibis/issues/1092)), only with proven source, library and item continuity, with bounded private history and no guessed `.bib` renames or document edits.

Notes and comments stay distinct from quotations. Zotero annotation support is stated per version and per live or closed mode. None of this widens MCP access to notes, annotations or private links.

## After 1.0: needs paid signing

- Apple Developer ID signing and notarization, and Windows code signing, when there is money or a free programme accepts Ibis.
- Opt-in iCloud settings sync on macOS, which needs a Developer ID build (network decision first) ([#1561](https://github.com/ibiscite/ibis/issues/1561)).
- Windows App Actions (Click to Do) and the Windows agent registry, which need signed packages.

## Not planned

Ibis is the fast front door to your library, not a reference manager: no organising, merging or attaching files (Zotero does this), no citation key generation, export translators or auto-export (Better BibTeX does this), no live citation fields in Word or Google Docs (Zotero's plugins do this), no reading other reference managers' private databases or cloud accounts, no AI writing, no legal citation short forms and no Ibis plugin for Zotero.
