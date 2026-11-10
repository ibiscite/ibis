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
[0008](decisions/0008-offline-by-default.md): no telemetry; each network use is opt-in, listed and approved; the editor endpoint.

## 0009 Insertion
[0009](decisions/0009-insertion.md): every Enter ends pasted or copied with a reason; selected text as the search.

## 0029 Native calls
[0029](decisions/0029-native-calls.md): `unsafe` is forbidden except in scoped native modules; the macOS and Windows paste proofs.

## 0010 Search
[0010](decisions/0010-search.md): instant local keyword search; meaning-based search later, local only.

## 0016 Settings (with 0018, 0020, 0022, 0023 and 0027)
[0016](decisions/0016-settings.md): `settings.json`, atomic saves, no migrations before 1.0, backups and restore.

## 0031 What Ibis remembers per app
[0031](decisions/0031-what-ibis-remembers-per-app.md): writing projects, a format per app, `apps.json`, recent citations and app-ID evidence.

## 0019 Updates
[0019](decisions/0019-updates.md): Tauri's updater, opt-in, signed; offers never take Enter; release naming.

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
[0036](decisions/0036-reader-trust-boundary.md): PDF.js in the web view, bytes not paths, limits and page labels.

## 0037 One source per fact; everything else is generated, checked or a command
[0037](decisions/0037-one-source-per-fact.md): one place per fact; repeats are generated or checked; routine steps are commands.

## 0038 One window that unfolds
[0038](decisions/0038-one-window-that-unfolds.md): details and pages unfold beside the list in the same window.

## 0039 Extending Ibis from outside
[0039](decisions/0039-extending-ibis-from-outside.md): licence boundary, package names, `ibis-foo` plugins, the running app answers `ibis`.

## 0040 One owner for shared settings
[0040](decisions/0040-one-settings-owner.md): one engine owner orders every settings write by when it was asked for; formats per app move into `settings.json`.

## 0041 System appearance follows the Linux desktop's dark preference
[0041](decisions/0041-linux-system-appearance.md): on Linux, System follows the desktop portal's colour scheme or a dark GTK theme; Light and Dark still win.

## 0042 Formats, apps and writings
[0042](decisions/0042-formats-apps-and-writings.md): one visible format; a writing you chose beats this app's format beats your format; Ibis finds inside Zotero; Settings and a live settings file stay in step.

## 0044 The citation line is a token field
[0044](decisions/0044-citation-line-token-field.md): each reference in the shelf's line is a token whose details open in a popover above it; the side panel keeps the Reader and warnings.

## 0027 Process until 1.0 (with 0011, 0015 and 0017)
[0027](decisions/0027-process-until-1-0.md): agents' merge authority, auto-merge on green and the testing rhythm.
