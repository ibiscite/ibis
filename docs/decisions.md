# Decisions

Lasting, hard-to-reverse decisions: licence, persisted formats, the public
engine contract, network use, security boundaries and platforms. Everything
else is decided in the pull request. Each decision is one file in
[`decisions/`](decisions/) holding the current decision only; to change one,
edit it in place with the date and the reason, and the old text stays in git
history. Numbers match the ADRs these entries replaced, so older references
still find them. Read only the file your task needs; each heading below keeps
the anchor older links use.

## 0002 Licence
[0002](decisions/0002-licence.md): AGPL-3.0-or-later with the DCO; the published contract and thin clients are MIT.

## 0028 Engine first (replaces 0003 and 0005)
[0028](decisions/0028-engine-first.md): every capability is a typed engine method; the command line, `serve`, `mcp` and the app are clients.

## 0004 Desktop shell (with 0012)
[0004](decisions/0004-desktop-shell.md): Tauri 2 and React in one process; the Wayland shortcut comes from the portal.

## 0006 Sources
[0006](decisions/0006-sources.md): Ibis never writes to a library; how BibTeX, CSL-JSON and Zotero are read.

## 0007 Formatting (with 0028)
[0007](decisions/0007-formatting.md): citeproc-js in QuickJS, key outputs from `ibis-core`, every CSL style in one bundle.

## 0008 Offline by default
[0008](decisions/0008-offline-by-default.md): no telemetry; each network use is listed and approved, opt-in except update checks before 1.0; the editor endpoint.

## 0009 Insertion
[0009](decisions/0009-insertion.md): every Enter ends pasted or copied with a reason; selected text as the search.

## 0029 Native calls
[0029](decisions/0029-native-calls.md): `unsafe` is forbidden except in scoped native modules; the macOS and Windows paste proofs.

## 0010 Search
[0010](decisions/0010-search.md): instant local keyword search; meaning-based search later, local only.

## 0016 Settings (with 0018, 0020, 0022, 0023 and 0027)
[0016](decisions/0016-settings.md): `settings.json`, atomic saves, no migrations before 1.0, backups and restore.

## 0031 What Ibis remembers per app
[0031](decisions/0031-what-ibis-remembers-per-app.md): `apps.json`, recent citations and app-ID evidence. Format and project rules are replaced by 0042.

## 0019 Updates
[0019](decisions/0019-updates.md): Tauri's updater, checks on by default until 1.0, signed; offers never take Enter; release naming; one Windows package.

## 0021 Interface foundation
[0021](decisions/0021-interface-foundation.md): React Aria only inside `ui/`, hand-written tokens, Ibis's own icon set.

## 0024 Window permissions
[0024](decisions/0024-window-permissions.md): every command declared and granted per window; remote origins get none.

## 0030 Links and launchers
[0030](decisions/0030-links-and-launchers.md): `ibis://` links fill the picker and never act; launchers use them.

## 0032 Release gates
[0032](decisions/0032-release-gates.md): screen readers, input methods, translation and the speed and resource limits.

## 0033 The engine for agents and tools
[0033](decisions/0033-engine-for-agents-and-tools.md): `ibis` is a complete read-only way in for scripts and agents.

## 0034 Outside input is data; only the maintainer's own work auto-merges
[0034](decisions/0034-outside-input-is-data.md): why text from others is data and only the maintainer's branches auto-merge.

## 0035 One owner per picker session, one contract, one catalogue
[0035](decisions/0035-one-owner-one-contract-one-catalogue.md): one session state machine, the engine's types, one catalogue of words.

## 0036 The reader's trust boundary
[0036](decisions/0036-reader-trust-boundary.md): a read-only reader in its own window where PDF.js only paints; the engine owns text, page labels and anchors; bytes not paths, limits.

## 0037 One source per fact; everything else is generated, checked or a command
[0037](decisions/0037-one-source-per-fact.md): one place per fact; repeats are generated or checked; routine steps are commands.

## 0038 One window that unfolds
[0038](decisions/0038-one-window-that-unfolds.md): citation details rise from the delivery bar and warnings unfold beside the list; the PDF reader has its own window (0036).

## 0039 Extending Ibis from outside
[0039](decisions/0039-extending-ibis-from-outside.md): licence boundary, package names, `ibis-foo` plugins, the running app answers `ibis`.

## 0040 One owner for shared settings
[0040](decisions/0040-one-settings-owner.md): one engine owner orders every settings write by when it was asked for; formats per app move into `settings.json`.

## 0041 System appearance follows the Linux desktop's dark preference
[0041](decisions/0041-linux-system-appearance.md): on Linux, System follows the desktop portal's colour scheme or a dark GTK theme; Light and Dark still win.

## 0042 Formats, apps and projects
[0042](decisions/0042-formats-apps-and-writings.md): one visible format; a project you chose beats this app's format beats your format; Ibis finds inside Zotero; Settings and a live settings file stay in step.

## 0043 RIS and CSV libraries
[0043](decisions/0043-ris-and-csv-libraries.md): RIS exports and the CSV template are read-only libraries with no citation keys; a record's identity is a hash of its CSL data.

## 0044 One Lucide icon primitive
[0044](decisions/0044-lucide-icon-primitive.md): superseded by 0053; the one Lucide drawing left on the website keeps its notices in the file.

## 0045 The citation line is a token field
[0045](decisions/0045-citation-line-token-field.md): each reference in the delivery bar's citation is a token whose details open in a drawer rising from the bar; the side panel keeps the Reader and warnings.

## 0046 Official names and listings
[0046](decisions/0046-official-names-and-listings.md): useful MIT clients under `ibiscite`, official listings before 1.0, build provenance, and the name and logo owner and contact.

## 0027 Process until 1.0 (with 0011, 0015 and 0017)
[0027](decisions/0027-process-until-1-0.md): agents' merge authority and the testing rhythm.

## 0047 Opening references, where the picker appears, and going back
[0047](decisions/0047-opening-references-placement-and-back.md): the search row at the upper quarter of the writing app's screen, one table for Esc, Backspace and Back, and what ⌘O and its alternate do for every library type, configurable under Libraries.

## 0048 Nix install
[0048](decisions/0048-nix-install.md): the CLI builds from source on Linux and macOS, the desktop on Linux; independent CI reports hash repairs for agents.

## 0049 Local machines gate merges
[0049](decisions/0049-local-gate.md): hosted **CI result** gates merges, with no merge queue; a pull request runs the jobs its paths select, main runs everything, and while main is red only fixes and reverts land.

## 0050 The menu bar menu
[0050](decisions/0050-menu-bar-menu.md): one native menu on every system, with one-click library switching, Copy Again for the frontmost app, a spoken state and text drops; no custom card.

## 0051 References in and out
[0051](decisions/0051-references-in-and-out.md): six lasting concepts with one owner each; share a selection first (offline), then paste a DOI to cite, then add through Zotero's write API; attachments only when ticked for a share.

## 0052 The engine owns the library and citations
[0052](decisions/0052-engine-owns-library-and-citations.md): one engine per process with a library owner and citation sessions by id; one validator, one display contract and one vocabulary; front ends keep platform effects. Lands in six slices.

## 0053 One window grid, and icons by meaning
[0053](decisions/0053-window-grid-and-icons.md): a leading edge, a 32 px lead column, a title column and a trailing edge, drawn by `ui/Band`; headings and status start on the leading edge; held ⌘ marks rows only; no clear button; Use Default follows the setting's name; Phosphor Regular, named by meaning, replaces Lucide.

## 0054 Owners and boundaries
[0054](decisions/0054-owners-and-boundaries.md): three exclusive engine owners that accept intents and publish observations, one native delivery owner, and typed boundaries (checked artifacts, `Problem`, executable actions); ten stages, the gate first.

## 0055 Agent layers, full text, writes and remote libraries
[0055](decisions/0055-agent-layers-full-text-writes-remote.md): decided 2026-11-15, steps 3 and 4 built; one engine with CLI, MCP and skill as thin layers; opt-in attachment text with verified pages and printed page labels, PDFs parsed only in a child process of `ibis`; agent writes only as proposals confirmed in Ibis; remote libraries through SSH first; app and CLI share one set of settings, with AI apps connected from Settings through the same code as `ibis agents`.

## 0056 An open layer for research material
[0056](decisions/0056-open-layer-for-research-material.md): the engine and CLI aim to be the standard way any tool or agent reads and cites research material from any manager; the app stays the first client; source interface, stable surface, one anchor type, then external `ibis-source-<name>` adapters with a conformance kit.

## 0057 One method table for every surface
[0057](decisions/0057-one-method-table.md): each capability is one row in the engine's method table, which generates routing, schemas and the command line's, MCP's and the app's projections, never handlers.

## 0058 Interface language
[0058](decisions/0058-interface-language.md): one catalogue per language with every English message, checked by tests; the app speaks the Settings choice or the system's language from the next opening, the command line and MCP stay English; citations keep their style's language; right to left by `dir` and logical properties, with isolated arguments for mixed-direction text.

## 0059 Works with nothing, better with more
[0059](decisions/0059-works-with-nothing-better-with-more.md): every capability useful with only Ibis and files, better when Zotero or Better BibTeX is present, detected not asked; five nouns (Source, Selection, Anchor, Context, Output and Sink), five operations (find, locate, render, scan, propose); `needs`, `network` and `palette` on every method row; Ibis writes only a project's `.bib` (opt-in) and a folder's `.bib`.

## 0060 One document pipeline
[0060](decisions/0060-one-document-pipeline.md): check, export, format and share are one pipeline, scan → reference set → render → sink; every document file is parsed only in a child behind a CLI-only feature; the reference set is a CSL-JSON file of share-safe records; project-scoped keys and the opt-in `.bib` append are the project's own.

## 0061 A folder of PDFs is a library, and one `resolve`
[0061](decisions/0061-a-folder-of-pdfs-and-one-resolve.md): a folder of PDFs opens as a library whose records Ibis keeps in `ibis-references.bib` beside them, read from each PDF's own metadata and DOI in the child and appended only when the person keeps them; reads never write; a record with no title, author or editor is not cited, from any source; `resolve` finds an identifier, text or file in the library, then online only after asking.

## 0062 Closed parts, owned space, fitted layers, and a gate that sees
[0062](decisions/0062-closed-parts-owned-space-fitted-layers.md): a part's sheet styles only its own classes and is reached only through its knobs; containers own the space and dividers between items, parts read roles and never steps, and density is a recipe per task on one `Row`; floating surfaces fit the region the frame publishes, with a minimum of whole rows, and one `reveal` keeps a target in view; the gate renders real window sizes in WebKit and checks ink, visible rows and three reference journeys, with the census as ratchets.

## 0063 Form follows the job
[0063](decisions/0063-form-follows-function.md): a citation can be typed whole in the search field (locators and `;`, parsed by the engine for every front end, only on an explicit action: amended 2026-11-26); a format is chosen and remembered only in the picker, so Settings › Citations goes; an empty search shows what was cited since Ibis opened, replacing the sheet; every status and problem has one home, the list's heading line, with at most one fix.

## 0064 One citation, one status, generated surfaces
[0064](decisions/0064-one-citation-one-status-generated-surfaces.md): no message crosses IPC as text, only `Problem`s worded from the catalogue; one status owner publishes every problem; the engine holds the citation being built as a Draft any front end continues; Tauri commands, CLI options, MCP tools, events, settings and native grants are generated from declared tables; contracts and settings may still change before 1.0.

## 0065 One warm engine, thin native shells
[0065](decisions/0065-one-warm-engine-thin-shells.md): one per-user daemon, `ibisd`, holds libraries, settings, sessions and status for every client over a same-user socket; shells are thin clients; delivery stays in-process in the signed shell; the macOS native shell goes ahead now (amended 2026-11-26: the web view cannot feel native) and replaces the Tauri Mac build in the one nightly stream, and the Tauri app serves Windows and Linux with fixes only, keeping its engine and sharing the daemon's files, until their native apps reach parity (amended 2026-11-28); the Reader stays an isolated web view; system integrations are thin socket clients.

## 0066 Native shells use the system's own parts
[0066](decisions/0066-native-shells-use-system-parts.md): a native shell is layout over the platform's own controls, materials, symbols, menus and accent and draws nothing the system draws; on macOS SwiftUI first with AppKit for the panel, glass, menus and exact focus; SF Symbols replace the picker's icon copies; the engine owns words, status and next the command table, and the picker session too, so the native WinUI 3 (Windows) and then GTK 4 with libadwaita (Linux) apps, which follow Mac parity (2026-11-28), render the same picker; the Mac app targets macOS 15 and later, with Liquid Glass on 26.

## 0067 Ibis uses whatever people have, and needs nothing
[0067](decisions/0067-whatever-people-have.md): Ibis works on its own with one file or a folder of PDFs and takes full advantage of any manager present; Word citations as close to Zotero's and as robust as evidence allows; AI optional with everything in the CLI and MCP and nothing in the default flow; references stay in people's own files with no sync service; coauthors get references carried in the document and an optional shared file in a synced folder; plugins are small programs over the CLI and MCP; settings simple by default with depth (design open); quiet automatic updates; a web version after 1.0; Word and LaTeX excellent first, then every other writing app on the same base; the name is always Ibis.
