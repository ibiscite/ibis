# Roadmap

Each release is usable on its own. Dates are not promised; scope is. Work is tracked in the GitHub milestone with the same version number. Every planned feature ships before 1.0; the numbers before it are working steps and may be renumbered. The maintainer moves to 1.0 by hand once people have tested everything (2026-11-01).

## Direction

Ibis is the assistant between the references people already keep and the apps they already write in. It should serve technical and nontechnical users, from a first citation to an expert workflow, across operating systems and languages. Picking references, getting citation keys, formatting and pasting citations, reading the exact page and driving the engine from scripts should fit the person's existing way of working.

- **Keep the user's sources and workflow.** Zotero alone or with Better BibTeX, manually maintained `.bib` files, BibDesk, Excel-based reference tracking and other applications are all legitimate starting points. Reach them through documented integrations, maintained exports, manual exports or uploads, and users' own files as format coverage grows. State which formats are supported; naming a source does not imply a direct importer exists. People keep choosing how their references and bibliographies are maintained. Citing never requires an Ibis-managed library or bibliography. Ibis never reads another app's private cloud account or writes to a source library.
- **Serve writing apps equally; citation keys first.** LaTeX/Overleaf citation-key workflows are the first concrete acceptance, with Markdown, Quarto, Typst and equivalent workflows given equal care. Formatted citations for Word, Google Docs and other writing apps remain supported, with their workflow improvements following the key workflows. Respect the citation syntax and bibliography arrangement the person uses.
- **Engine first.** The `ibis` command line is the engine: it can do everything, with `--json`, a JSON-RPC server mode and an MCP server for AI agents. The app is a fast, friendly client of the same engine ([decision 0028](docs/decisions.md#0028-engine-first-replaces-0003-and-0005), [architecture](docs/architecture.md)).
- **Defaults the person controls.** Provide a configurable global default, remembered per-app choices, sensible Automatic behaviour and an explicit one-time override. Show what will be pasted. Keep these choices predictable without repeated setup or silent setting changes; when the app alone cannot identify the document's format, keep the person's explicit choice. Setup steps finish themselves and every extra is one key away ([design](docs/design.md)).
- **Only what Ibis can get right.** A plausible but wrong citation is worse than none. Where correctness needs the whole document (numbering, ibid., 2020a and 2020b), Ibis gives what is correct on its own instead and says why. Bibliography preparation, checking and export are optional assistance with files the person chooses, without source-library writes or a required new bibliography workflow. Every feature passes two questions: can Ibis get it right from what it knows, and does it save real work?
- **Languages and platforms are part of coverage.** Metadata and names, search, keyboard and input methods, text direction and citation locales all need usable, demonstrated paths on Windows, macOS and Linux. Localise the interface incrementally too. Record limitations in each area rather than treating English or one operating system as proof of universal support.
- **Offline by default.** Any online feature is opt-in and needs a recorded network decision ([decisions](docs/decisions.md#0008-offline-by-default)) and the maintainer's yes.
- **Long-lived foundations.** Open standards (CSL, CSL-JSON, BibTeX) and widely used open source (Zotero's citeproc-js, SQLite, PDF.js, Tauri, React Aria), each behind one Ibis module so it can be replaced.

Coverage grows incrementally. Use a compatibility matrix of source/integration or file format, writing app/output, operating system and language/input/locale, with tested, limited and planned states tied to evidence. Each release claims only the combinations demonstrated; the direction above does not add universal coverage to the first stable release or change the release scopes below.

Until 1.0 nothing is kept for compatibility (decision 0027): anything that is wrong is rebuilt.

<a id="approved-source-workflows-before-10"></a>

## Source-workflow priorities for stable

The maintainer approved these five directions on 2026-10-30, then clarified
that stable should focus on clear, major, cohesive workflows that can be made
reliable. On 2026-11-01 he set that all five ship before 1.0. Finish the
current 0.3 work and source-storage prerequisites first; prioritize the first three
below. Prove a small passage search/open/copy slice without waiting for every
annotation kind, reader or manager. Narrow or defer an unproven slice rather than
weaken its claims or delay stable. Other approved release scope and correctness,
accessibility, signing, update and resource gates remain in place.

1. **Check the project's bibliography** ([#1377](https://github.com/ibiscite/ibis/issues/1377)).
   Choose a read-only `.bib`; results show "In thesis.bib", "Missing from
   thesis.bib" or a specific unknown/conflict state. Copy a safe missing entry
   with its existing key when available; otherwise explain source-manager export.
2. **Search highlights and personal comments, then open the exact passage**
   ([#54](https://github.com/ibiscite/ibis/issues/54), [#359](https://github.com/ibiscite/ibis/issues/359)).
   Show the matching passage, separate comment, reference and page; open the
   matching Zotero annotation or verified PDF location.
3. **Copy a quote with its reference** ([#1385](https://github.com/ibiscite/ibis/issues/1385)).
   Plain text or Markdown includes the quotation, accurate page when known,
   reference and source link; personal comments stay separate.
4. **Export selected passages as an evidence brief**
   ([#1517](https://github.com/ibiscite/ibis/issues/1517)). Select several passages,
   optionally enter a question, and export one Markdown file with quotations,
   separate comments, references, pages and source links. No new notebook or
   publishing service; local-only links are identified as such.
5. **Warn when a previously copied key changes**
   ([#1092](https://github.com/ibiscite/ibis/issues/1092)). Show the old and current
   keys only with proven source/library/item continuity (qualifying Zotero, or
   CSL-JSON with a stable `id`); keep bounded private local item/key history, with no
   guessed `.bib` rename matching or library/document writes.

The evidence brief and remembered-key warning follow the first three. Decision
memory, a "resume research" workflow and a separate AI companion start as short
research briefs in 0.7, built only where Ibis can get them right.
The five features do not broaden MCP access to notes, annotations or private links.
Interaction rules are in [design](docs/design.md#source-checks-and-passage-reuse-planned).

The [first-version research](docs/source-workflow-research.md) narrows these
features before implementation: directly matched saved passages with clear actions; one
eligible saved quotation copied with a standalone reference and separate location;
the same block reused in a fixed-layout brief saved only to a new Markdown file;
and key warnings only with proven source/library/item continuity. Notes and
comments stay distinct from quotations. Zotero annotation support is qualified by
version and live/closed mode; bibliography imports alone do not enable passages.
These bounds add no manager adapters, persistent quote presets, document repair or
new MCP access. The brief and key-warning contracts are retained for evaluation,
not as additional release requirements.

| Release | Scope |
| --- | --- |
| **0.0 Foundation** and **0.1 Picker** (released) | The first command line, BibTeX reading that survives damaged files, key outputs, and a preview picker with five styles and signed in-app updates. See the [changelog](CHANGELOG.md). |
| **0.2 The new engine** | The rebuild: one engine for the command line and the app, on today's sources and outputs. Zotero read through its local API while it runs and from a private copy of its database while it is closed (Zotero 7–10, Better BibTeX keys with conflicts shown, group libraries); `.bib` and CSL-JSON files. One citation request and result for every output; today's key syntaxes on the new contract; formatted citations in the engine with the five current styles, checked against Zotero's own output. The command line does everything with `--json`, `ibis schema` and `ibis serve --stdio`. Today's picker running on the engine and opening in under 100 ms. Installers stay unsigned on Windows with first-launch steps. |
| **0.3 Straight into your document** | A first citation with almost no setup: output chosen from the app you are writing in, shown in the footer as what Enter will do ("Paste into Word · APA") and changed with one key (just this once, or remembered for that app, with a way back to automatic); the first paste in each browser, code editor or app Ibis does not recognise opens the chooser once, which can also keep that app to copying. Setup that finishes itself, including a permission detour that keeps the prepared citation; sample references when there is no library, the same invented set as the website and help. Enter pastes into that app on macOS (Windows may follow in 0.3.x), on by default with a switch to turn it off; Enter pressed while results load runs once; a copy with the reason whenever pasting is unsafe (terminals, remote desktops and remote viewers, editors whose paste key does something else, elevated apps, system-wide secure typing on macOS and known password fields on Windows), a remembered "copy only here" for any other app, a pending paste cancelled when the computer sleeps, locks or switches user, and a one-time note per word processor that documents using a reference manager's plugin (Zotero, Mendeley, EndNote, Citavi, Paperpile and others) should insert with it. After the window closes a short receipt says what happened, and the last outcome stays in More. Linux copies and says why; on Wayland the shortcut comes from the desktop portal, or Ibis shows the exact binding to add. A stable, self-signed macOS identity so the paste permission survives updates (no paid Apple Developer ID yet); an update never opens the picker. Rich text (RTF and HTML) on the clipboard, "Copy as plain text" and "Copy reference as Markdown"; recent citations per app; changing the last citation as an explicit action that replaces only a selected citation. A page or page range, narrative and author-omitted citations. Short selected text becomes the search, so the paste replaces a placeholder like "smith 2020". For scripts and AI agents ([decision 0033](docs/decisions.md#0033-the-engine-for-agents-and-tools)): `ibis mcp` speaks MCP 2026-07-28 and still answers 2025 clients ([#1591](https://github.com/ibiscite/ibis/issues/1591)); an Agent Skill shipped in one plugin for Claude Code and Codex; `ibis` uses the libraries chosen in the app when none are named; the app installs the matching `ibis` command; a Homebrew tap and winget. A peek at a reference's full details before citing; a `.bib` or CSL-JSON file dropped on the window becomes a library. Release gates ([decision 0032](docs/decisions.md#0032-release-gates)): the whole citation works with VoiceOver and NVDA and with Chinese, Japanese and Korean input methods, interface text is ready for translation, and speed and resource budgets fail CI. |
| **0.4 Build any citation** | Everything Zotero's dialog does to a citation, visible in place: several references, reorder (also by dragging), all 29 locator types behind one searchable control with ranges and a literal fallback, prefix and suffix with italics, citation forms (full, year only, mixed within one citation), keep my order. Every key syntax Ibis can write correctly (Typst, org-cite, MyST, AsciiDoc, ConTeXt and more), each with its capability table. Any official CSL style and locale; output presets; bibliography entries; "References used this session" as an editable bibliography. The project's own `.bib` as the authority: results say "In thesis.bib" or "Missing from thesis.bib", and a missing entry can be copied with its existing key. A bibliography kept only in Overleaf, for free plans without Git or Dropbox: after one setup step (the project link, a dropped download or a pasted bibliography), one key opens the project in the browser to download it, and Ibis reads every `.bib` in that file or project ZIP. It switches to a newer download with the same name that keeps most of the same keys and says so in the footer, otherwise asks and lists the keys that would go. It keeps its last good copy and needs no sign-in or network use. A browser extension that reads the open Overleaf project's bibliography and later inserts keys there, only if Overleaf agrees in writing (asked 2026-10-30). A test of pasting real footnotes for note styles in Word, Pages and LibreOffice, shipped only if it works. A test of pasting live Zotero citations into Word for items read from Zotero, using Zotero's own citation field so Zotero's Word plugin updates them and adds them to its bibliography, shipped only if it works. A CSV template as a source for references kept in a spreadsheet. RIS files, the export EndNote Web, Citavi Web and many research portals offer, as a source that gives formatted citations only, because RIS has no citation keys; a setup line for each reference manager in the compatibility guide, saying which keep their export current by themselves. For items read from Zotero, markers for LibreOffice `.odt` files that the ODF Scan add-on converts to live citations; refused without the original library and item key. Drag the citation from the shelf into any app. A Settings window with customisable shortcuts and presets, and a documented config file you can back up and restore on another computer, for example through iCloud Drive or OneDrive; paste permissions stay on each computer ([#1561](https://github.com/ibiscite/ibis/issues/1561)). `ibis://` links that open the picker filled in, with Shortcuts (Spotlight) and Alfred recipes; "Show in Zotero" and "Open PDF in Zotero at this page" through Zotero's own links. An editor endpoint compatible with Better BibTeX's Cite as you Write, claimed only once a named editor client passes. Citation receipts and a strict mode for scripts and agents; the command line picks the output for the app you name; the MCP Registry listing. The interface in German and one right-to-left language. An opt-in "Download quietly, install when I quit" for updates. Public beta. |
| **0.5 Find the passage** | Forgiving search for the remaining cases (Chinese, Japanese and Korean names, romanised readings). Collections, tags, saved searches and filters; "Selected in Zotero"; suggestions before typing. Zotero highlights, notes and personal comments searchable from the picker: results show the matching passage, separate comment, reference and page; Open passage returns to the exact annotation or verified PDF location. An explicit Use reference/page action fills in its reference and page, shown as "PDF page 8 · cite p. 417", with the printed page taken from the PDF, the annotation or an offset you confirm, never assumed. A PDF dropped on the window finds its reference; "Copy quote with reference" as plain text or Markdown, including the quotation, known page, reference and source link, with comments separate. "Is this in my library?" for a pasted identifier, reference or reference list. Selected passages and comments exported as one Markdown evidence brief ([#1517](https://github.com/ibiscite/ibis/issues/1517)). A read-only PDF reader inside Ibis, with EPUB and web snapshots, for what the reader people already use cannot do ([#1252](https://github.com/ibiscite/ibis/issues/1252)). |
| **0.6 Your document** | Export what a document cites to `.bib`, Hayagriva or CSL-JSON; `ibis check` on a manuscript (`.md`, `.qmd`, `.tex`, `.typ`) as a build gate that also catches citations typed without Ibis, with an optional lock file so co-authors reproduce the same references; opt-in DOI lookup for references not in the library (network decision first); a warning when a reference's citation key changed since it was last copied ([#1092](https://github.com/ibiscite/ibis/issues/1092)). |
| **0.7 Everywhere you work** | Pasting on Linux desktops and per-app paste rules. A language server (`ibis lsp`), a Raycast extension and other launcher extensions, a browser extension. A library on another computer over the person's own SSH ([#1542](https://github.com/ibiscite/ibis/issues/1542)). Adding a missing item to Zotero through its official local write API (opt-in, decided first). More interface translations; EndNote XML and Word Sources.xml files; ARM64 builds. Search by meaning, combined with keyword search. Research briefs for decision memory, resuming research and an AI companion. |
| **1.0 Stable** | No new features: the maintainer has people test everything, then moves to 1.0. Accessibility verified with VoiceOver, NVDA and Orca; a stable self-signed macOS identity and unsigned Windows installers with first-launch steps (no paid signing, maintainer 2026-11-01); a versioned engine contract; Flathub, Homebrew and winget, with Linux support stated per desktop, display server and package; documentation site. |

## After 1.0: needs paid signing

- Apple Developer ID signing and notarization, and Windows code signing, when there is money or a free programme accepts Ibis.
- Opt-in iCloud settings sync on macOS, which needs a Developer ID build (network decision first) ([#1561](https://github.com/ibiscite/ibis/issues/1561)).
- Windows App Actions (Click to Do) and the Windows agent registry, which need signed packages.

## Not planned

Ibis is the fast front door to your library, not a reference manager: no organising, merging or attaching files (Zotero does this), no citation key generation, export translators or auto-export (Better BibTeX does this), no live citation fields in Word or Google Docs (Zotero's plugins do this), no reading other reference managers' private databases or cloud accounts, no AI writing, no legal citation short forms and no Ibis plugin for Zotero.
