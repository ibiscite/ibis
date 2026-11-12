# Roadmap

Ibis is a writing companion for the references people already keep. One shortcut, in any app, finds anything in their sources (a reference, its PDF page, a highlight or a note), shows it to be sure, and cites or quotes it where they are writing. People writing in LaTeX, Markdown, Quarto and Typst decide close calls; first-time and expert users, every major operating system and many languages all count. Ibis stays about references and research material, never a general file or web search (maintainer, 2026-11-05).

Every feature in the releases below ships before 1.0. Versions before 1.0 are working steps: each finished milestone becomes one release with a GitHub milestone of the same name, and Nightly carries daily testing. 1.0 adds no features: people test everything, then the maintainer moves to 1.0 by hand. Dates are not promised; scope is. The maintainer adds to or changes this plan at any time; agents bring the plan, docs and code into line the same day.

## How we build

- **Simplest correct form.** Each feature ships as the smallest version that is right and saves real work. Rules for cases no one has met yet wait until someone does; a feature can still grow later. Functionality first, then strict polish of how Ibis looks and behaves.
- **Consolidate before adding.** One way to do each thing, in one place. When something is replaced or withdrawn, its code, tests and docs go in the same change. A change that makes the code smaller and clearer is worth more than a new option. Until 1.0 nothing is kept for compatibility (decision 0027).
- **Easy to change** (maintainer, 2026-11-07). Each thing is defined once, and everything else is derived from it: a type, a message, a colour or a space, a setting, a check. A change then touches as few files as possible, in the engine, the command line, the app and the design alike. Remove layers, wrappers, duplicate definitions and process steps that do not pay for themselves. When a change is hard to make, simplify the structure first, then make the change.
- **Engine first, front ends thin.** Every capability lives once in the Rust engine; the app, `ibis`, `ibis serve` and `ibis mcp` only show it ([decision 0028](docs/decisions/0028-engine-first.md), [architecture](docs/architecture.md)). The engine holds facts and abilities; what a person chose (a format per app, a project) stays a thin layer.
- **Only what Ibis can get right.** A plausible but wrong citation is worse than none. Where correctness needs the whole document (numbering, ibid., 2020a and 2020b), Ibis gives what is correct on its own and says why.
- **Reveal as you go.** The picker finds and cites. Advanced tools appear once they apply (a linked `.bib`, a PDF, a passage) and stay out of the way otherwise ([design](docs/design.md)).
- **Offline by default** ([decision 0008](docs/decisions/0008-offline-by-default.md)), **read-only sources**, open standards (CSL, CSL-JSON, BibTeX) and widely used open source, each behind one Ibis module so it can be replaced.
- Each release claims only what it has demonstrated, in the [compatibility guide](docs/compatibility.md). The maintainer shares builds and gathers feedback while agents keep building.

## Sources and writing apps

- **Sources:** Zotero with or without Better BibTeX, `.bib` files, BibDesk, CSL-JSON, spreadsheets, and exports from other managers (RIS, EndNote XML). EndNote and Mendeley are reached only through their exports, which Ibis says can go stale. PDFs and notes outside a reference manager can be found, previewed and quoted; Ibis cites them only when it knows their details. Ibis never writes to a library or reads another app's private database or cloud account; the one exception is Zotero's own database, read from a private copy while Zotero is closed (decision 0006).
- **Writing apps:** keys first. LaTeX and Overleaf, Markdown, Quarto and Typst get keys in their own syntax. Word, Google Docs and others get formatted citations, and keys reach them too: Zotero's free ODF/DOCX Scan turns keys in a `.docx` or `.odt` into live Zotero citations, and `ibis format` writes a finished copy for people without Zotero. The website and first run say this plainly, since Word users expect a plugin.
- **Agents and scripts:** `ibis` does everything with `--json`, a JSON-RPC server and an MCP server ([decision 0033](docs/decisions/0033-engine-for-agents-and-tools.md)). Ibis's edge is citations that can be checked, from any source, offline. Agents reach highlights and notes only after the person turns that on for a library.

## Releases

### 0.0 to 0.2 (released)

The first command line and picker; then one Rust engine for the command line and the app: Zotero through its local API or a private copy of its database (Zotero 7–10, Better BibTeX keys, group libraries), `.bib` and CSL-JSON, one citation request and result for every output, formatted citations checked against Zotero's own output, and `--json`, `ibis schema` and `ibis serve --stdio`. See the [changelog](CHANGELOG.md).

### Consolidation (now, alongside finishing 0.3)

A month of fast building left parallel paths, rules no one has used and code that does one job twice. This stage makes Ibis smaller and clearer without losing a feature, in this order; 0.3 is released and new 0.4 features start once steps 1 to 4 have landed, while 0.3 fixes continue. Success is fewer owners, conversions and failure paths for the same behaviour; less code (roughly 10 to 15%) is a sign of it, not the goal. Reviewed by Sol on 2026-11-05.

1. **Small docs and a short tracking issue** ([#1690](https://github.com/ibiscite/ibis/issues/1690)): one short file per decision with an index; `design.md` keeps its principles while component rules move beside the components and the gallery; measurement logs live on pull requests; the tracking issue holds only ordered work, owners, pull requests and blockers.
2. **One complete native journey test:** summon Ibis, choose a reference, deliver it into an invented document and read it back, by extending an existing harness; plus the fake clock and platform seam the next steps need ([#1676](https://github.com/ibiscite/ibis/issues/1676)), not a universal simulator.
3. **Withdrawn rules leave the code** ([#1697](https://github.com/ibiscite/ibis/issues/1697), [#1377](https://github.com/ibiscite/ibis/issues/1377)): Apply Updates and retained project settings, last-good bibliography copies and the unverified-key paths, in the engine, the command line and the app (about 2,000 lines).
4. **One engine contract for the app** ([#1260](https://github.com/ibiscite/ibis/issues/1260), [#1693](https://github.com/ibiscite/ibis/issues/1693), [#1675](https://github.com/ibiscite/ibis/issues/1675)): the app talks only to `ibis-engine` and stops depending on `ibis-zotero-local`; types and message codes are generated once from the engine; the engine returns codes and every front end words them from one catalogue.
5. **One set of interface parts and plain words:** buttons, fields, sheets, list rows and the Inspector come from `ui/`, and a check fails a screen that brings its own spacing or colours; the visible glitches go with them (key badges over controls, the sheet without padding, a two-line title over the byline). Every message the app and command line show uses people's words (design principle 12).
6. **One owner for shared settings:** settings, formats per app, the selected project and backup change through one engine transaction; the app's own file and ordering code goes. Paste permissions, windows and other system effects stay with the app.
7. **The picker session, in working slices** ([#1260](https://github.com/ibiscite/ibis/issues/1260)): session behaviour moves into one state machine a slice at a time, the library's lifecycle separately; shared sheets, menus and buttons in `ui/` are consolidated as each screen is touched.
8. **Leaner checks** ([#1678](https://github.com/ibiscite/ibis/issues/1678), [#1693](https://github.com/ibiscite/ibis/issues/1693)): `scripts/check.mjs` plans the checks and `land.sh` uses the same plan; finer CI path classes; long inline programs in `preview.yml` move into scripts; tests that read source or CSS become rendered outcome checks; sleeps become fake clocks; engine output checks become reviewed snapshots.

- Isolate each client's task context: capture project, source scope and output at task start so another client's selection cannot retarget it; source changes still revalidate citations (from architecture).
- Merge `ibis-zotero-local` into `ibis-sources` once its callers reach it through the engine (from architecture).
- Generate action/race sequences for picker, Settings and Rust delivery, save minimized regressions, and run short seeded campaigns in PR CI and larger ones nightly (from testing).
- Align performance gates with the release sizes: block at 10,000 generated references and report a 50,000-reference nightly check, replacing the current 100k timing cases (from architecture, #1536).

### 0.3 Straight into your document

- **The first citation with almost no setup.** The output follows the app you are writing in and the footer shows what Enter will do ("Paste into Word · APA"); one key changes it for that app. The first paste into a browser, code editor or unknown app asks once. Setup finishes itself, including the paste permission. Sample references when there is no library.
- **Enter pastes** on macOS and Windows; Ibis copies instead, and says why, wherever pasting is unsafe (terminals, remote desktops, elevated apps, secure typing, password fields) or the app is set to copy only. A short receipt says what happened.
- **Linux** copies and says why; on Wayland the shortcut comes from the desktop portal, or Ibis shows the binding to add.
- **Citations:** rich text and plain text on the clipboard; recent citations per app; Change Last Citation; a page or range; narrative and author-omitted forms; selected text becomes the search; a peek at a reference's details; a dropped `.bib` or CSL-JSON file becomes a library.
- **Updates** keep the macOS paste permission through a stable self-signed identity.
- **For agents:** `ibis mcp` (MCP 2026-07-28, answering 2025 clients), an Agent Skill for Claude Code and Codex, `ibis` using the app's libraries by default, Homebrew and winget.
- **Release gates** ([decision 0032](docs/decisions/0032-release-gates.md)): VoiceOver, NVDA and Chinese, Japanese and Korean input, checked by hand.
- **Ships when** consolidation steps 1 to 3 have landed and the maintainer's hands-on checks pass: the Mac hands-on run (VoiceOver, updates keeping the paste permission, help screenshots), NVDA on Windows and one input-method check (maintainer, 2026-11-07). Text ready for translation moves into step 4's catalogue, limits that run as checks into step 8, and small picker touches with the word-processor note into 0.4.

### 0.4 Build any citation

- Add locators without a type where the output can preserve them (from parity).

- **Everything Zotero's citation dialog does** (built first in 0.4, maintainer, 2026-11-07)**:** several references and reordering, every locator type with ranges, prefix and suffix, citation forms and keep my order.
- **Every key syntax Ibis can write correctly** (Typst, org-cite, MyST, AsciiDoc, ConTeXt and more); **any official CSL style and locale**; output presets; bibliography entries; references cited since Ibis opened as an editable bibliography.
- **The project's bibliography:** link a `.bib` and results say "In thesis.bib" or "Missing from thesis.bib", with the missing entry to copy. The check is advice: an unreadable file is a warning, and only a key known to name a different work is refused.
- **Optional writing projects** ([#1697](https://github.com/ibiscite/ibis/issues/1697)): a project is a `.bib` file and its format, nothing more, picked in the format menu and remembered per app ([decision 0031](docs/decisions/0031-what-ibis-remembers-per-app.md)).
- **Overleaf on free plans:** one key downloads the project's bibliography and Ibis reads its `.bib` files, with no sign-in or network use; a browser extension only if Overleaf agrees in writing.
- **More sources:** RIS files and a CSV template for spreadsheets; one setup line per reference manager in the compatibility guide.
- **Live citations through keys:** Pandoc keys in Word or LibreOffice become live Zotero citations through Zotero's ODF/DOCX Scan, Google Docs through Zotero's switch-word-processor step; Ibis documents and tests the flow, checks its keys match Zotero's, and writes ODF Scan markers. Trials of real footnotes and Zotero's own Word field ship only if they work.
- **Settings:** every shortcut can be changed, connected libraries (Zotero and `.bib` together), and a settings file to back up and restore ([#1561](https://github.com/ibiscite/ibis/issues/1561)); one rotating tip on the empty search; an opt-in "install updates when I quit".
- **Links and integrations:** drag the citation into any app; `ibis://` links with Shortcuts and Alfred recipes; Show in Zotero and Open PDF at This Page; with Zotero in front, the same picker opens and Enter opens the PDF or shows the item; an editor endpoint compatible with Better BibTeX's Cite as You Write.
- **Moved from 0.3** (maintainer, 2026-11-07): the app installs the matching `ibis` command, and Copy as Markdown gives a formatted citation in Markdown.
- **For agents:** source health on every result, citation receipts and a strict mode; citing CSL-JSON a caller sends; output for an app named on the command line; the MCP Registry listing.
- **The interface in German and one right-to-left language.** Public beta.

### 0.5 Find the passage

- Add last-cited recall through `ibid`/`ebd.` (from parity).

- **Search** that forgives Chinese, Japanese and Korean names and romanised readings; collections, tags, saved searches and filters; "Selected in Zotero"; suggestions before typing.
- **Passages** ([#54](https://github.com/ibiscite/ibis/issues/54), [#359](https://github.com/ibiscite/ibis/issues/359)): Zotero highlights, notes and comments searchable from the picker, each with its reference and page ("PDF page 8 · cite p. 417"), the printed page taken from the PDF, the annotation or an offset the person confirms, never assumed. Notes and comments stay distinct from quotations.
- **Quotes and briefs** ([#1385](https://github.com/ibiscite/ibis/issues/1385), [#1517](https://github.com/ibiscite/ibis/issues/1517)): a PDF dropped on the window finds its reference; Copy Quote with Reference as plain text or Markdown; chosen passages exported as one Markdown brief.
- **A read-only PDF preview** ([#1252](https://github.com/ibiscite/ibis/issues/1252), [decision 0036](docs/decisions/0036-reader-trust-boundary.md)) for PDFs already in Zotero or a `.bib` `file` field, so checking a page never needs Zotero; Ibis never imports, edits or annotates them. EPUB and web snapshots follow.
- **Files outside a reference manager:** a folder of PDFs or of Markdown notes can be searched, previewed and quoted.
- **Agents and passages:** opt-in per library, off by default; private links never reach them.
- **Rules to settle before building 0.5** (from the removed source-workflow research):
  - **Presentation:** do passages appear as a References/Passages scope or unfolded under a reference, and does Enter open a passage? Decide with a comprehension walkthrough first.
  - **Open buttons** name their target ("Open annotation in Zotero", "Open PDF at page", "Open reference"). A failed open offers the others and never silently opens something else.
  - **Pages:** a Roman page label is never sent as a PDF index, and "Citation page unknown" is said plainly.
  - **Briefs:** never overwrite a file, repeat the reference for each passage, treat the question as literal text, and stay readable without Ibis or Zotero.
  - **Testing:** check with real Zotero users and bibliography-only users that they can tell a quote from a comment, and with someone who reads a brief without Ibis.
- **Add a note instead of a citation** ([#55](https://github.com/ibiscite/ibis/issues/55)), like Zotero's Add Note.
- **Changed-key warning** (maintainer, 2026-11-11: important; design the UX and edge cases first): warn when a key you cited before now belongs to a renamed or changed item. Warn only when the same item is provable (a plain `.bib` rename can't be tracked); remember a key only after its copy succeeded; offer a way to forget a remembered key.
- **Is this in my library?** for a pasted identifier or reference list; results say whether a reference has a PDF or a saved web page.

### 0.6 Your document

- Export what a document cites to `.bib`, Hayagriva or CSL-JSON.
- `ibis check` on a manuscript (`.md`, `.qmd`, `.tex`, `.typ`) as a build gate, with an optional lock file for co-authors.
- `ibis format` turns a `.docx` or `.odt` written with keys into a finished copy with formatted citations and a bibliography; running it again changes the style, and the original is never edited.
- A warning when a reference's citation key changed since it was last copied ([#1092](https://github.com/ibiscite/ibis/issues/1092)).
- **Opt-in online lookup** (maintainer, 2026-11-11): DOI lookup and open search for references not in the library, through Crossref, OpenAlex and doi.org. Off until the person turns it on, either when setup asks once or later in Settings; nothing goes online otherwise.

### 0.7 Everywhere you work

- Make the rules and file readers usable in WebAssembly, with native Zotero/SQLite readers optional; if requested, add thin Python/npm clients around `ibis` and data/pipe extensions rather than loading native code (from architecture).

- Pasting on Linux desktops, and per-app paste rules.
- A language server (`ibis lsp`), launcher extensions (Raycast and others) and a browser extension.
- A library on another computer over the person's own SSH ([#1542](https://github.com/ibiscite/ibis/issues/1542)).
- Adding a missing item to Zotero through its official local write API (opt-in, decided first).
- More translations; EndNote XML and Word Sources.xml files; ARM64 builds; search by meaning.
- **Annotations from other reference managers** (maintainer, 2026-11-11: support as many apps and systems as possible): Paperpile, JabRef, BibDesk/Skim, EndNote, Mendeley and Citavi, each read from the manager's own data, since `.bib` and RIS carry no annotations.

### 1.0 Stable

No new features. People test everything, then the maintainer moves to 1.0. Gates: VoiceOver, NVDA and Orca; a versioned engine contract proved by real clients; Flathub, Homebrew, Nix, winget and MCP Registry listings ([decision 0046](docs/decisions/0046-official-names-and-listings.md)), with Linux support stated per desktop; the documentation site; licence notices and supply-chain records. macOS keeps its self-signed identity and Windows installers stay unsigned; paid signing is revisited near 1.0 (decision 0032).

- Qualify minimum OS versions, real Wayland consent/focus and cloud/network file-provider recovery per platform; build and protocol fakes do not establish these (from compatibility and architecture).
- Extend the existing native editor journeys with LibreOffice rich-text readback on Mac and Windows, checking placement, replacement and refusal with replayable evidence (from testing).
- Add reviewed native visual-state comparisons for picker and Settings, including large text, contrast, reduced motion, scaling and mixed-direction text (from testing).
- Retain complete VoiceOver/NVDA journeys, real Chinese/Japanese/Korean input-method checks and an Orca pilot, recording unavailable coverage explicitly (from testing).
- Collect exact-release native latency, idle CPU/wakeups and whole-process memory on named Mac/Windows/Linux machines, including cycle trends and the existing validator's sample/settling requirements (from testing and architecture).
- Run bounded Codex exploration with existing access and independent assertions, plus free Google Docs/Overleaf readback in isolated synthetic sessions; arrange self-hosted Mac/Windows sessions, permissions, LibreOffice and dedicated free test accounts without paid runners or models (from testing).
- Before numbered releases, run a risk-ordered computer-use check of the nightly on Mac, Windows and Ubuntu, at most 45 scenarios, restoring settings and recording pass/fail/blocked/skipped with failure evidence (from testing).
- Publish release SBOMs, make stable releases immutable, and write the update-key loss/rotation procedure (from releasing).
- Qualify non-US keyboard layouts (Cyrillic and others fall back to the US key position today), sleep and wake, and Windows 10 (WebView2 is supported there to at least October 2028) (from compatibility).
- Before claiming support: replay the VS Code "Citation Picker for Zotero" requests against Ibis's editor endpoint, and claim Zotero 10 for the ODF Scan flow only after Ibis's own LibreOffice round trip passes on it (from compatibility).

## After 1.0, maybe

Research briefs beyond exported passages, decision memory, resuming research and an AI companion. These are a different product from finding and citing; agents already get Ibis's part through `ibis` and `ibis mcp` (maintainer, 2026-11-05).

## Needs paid signing

- Apple Developer ID and notarization, and Windows code signing, when there is money or a free programme accepts Ibis.
- Opt-in iCloud settings sync on macOS (network decision first) ([#1561](https://github.com/ibiscite/ibis/issues/1561)).
- Windows App Actions and the Windows agent registry.

## Not planned

Ibis is the fast front door to your references, not a reference manager: no organising, merging or attaching files (Zotero does this), no citation key generation, export translators or auto-export (Better BibTeX does this), no Ibis plugin for Word, Google Docs or Zotero and no live citation fields of Ibis's own (Zotero's plugins and ODF/DOCX Scan do this), no reading other managers' private databases or cloud accounts, no general file or web search, no AI writing and no legal citation short forms.
