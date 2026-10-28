# Decisions

Lasting, hard-to-reverse decisions: licence, persisted formats, the public
engine contract, network use, security boundaries and platforms. Everything
else is decided in the pull request. Each entry is the current decision; to
change one, edit it in place with the date and the reason, and the old text
stays in git history. Numbers match the ADRs these entries replaced, so older
references still find them.

## 0002 Licence

AGPL-3.0-or-later for the whole repository. Contributions come under the
Developer Certificate of Origin (`git commit -s`); no contributor licence
agreement. CSL styles and locales ship unmodified, under their own licence,
credited to citationstyles.org. `cargo deny` keeps dependency licences
compatible.

## 0028 Engine first (replaces 0003 and 0005)

Every capability lives in the Rust crates (`ibis-core`, `ibis-sources`,
`ibis-csl`, `ibis-engine`) as typed methods. The `ibis` command line, `ibis
serve --stdio` (JSON-RPC 2.0), `ibis mcp` and the desktop app are clients; the
app links the engine in-process. The command line exposes everything; a test
fails when two front ends give different results for the same request. Methods
take and return JSON described by `ibis schema`, in a versioned envelope with
stable error codes. One citation request, one result, with typed losses and
refusals instead of plausible but wrong text. Records, attachments, notes,
annotations, collections, tags and saved searches are addressed by
`library + native key`; Ibis maps Zotero's raw data to CSL itself so Zotero
open and closed give identical results. Records keep only the normalised data
Ibis uses; the source stays the original, and an export that needs the original
entry reads it from the source (2026-10-27: in-memory raw copies dropped, which
nothing read and which doubled memory). Details: [architecture](architecture.md).

## 0004 Desktop shell (with 0012)

Tauri 2 with a React interface, in one process: the picker window is hidden
between uses, and the shortcut and library live in the same process. On
Wayland the shortcut uses the XDG GlobalShortcuts portal, with `ibis toggle`
as a fallback people can bind.

## 0006 Sources

Ibis never writes to a library. BibTeX and BibLaTeX entries are parsed one by
one; broken entries, duplicate keys and undefined macros become warnings with
line numbers. Zotero is read through its local API while it runs and from a
private read-only copy of `zotero.sqlite` (with `-wal` and `-shm`) while it is
closed, tested on Zotero 7 to 10, leaving out trashed items. Citation keys
come from Zotero's native field, Extra and Better BibTeX; when they disagree,
key output for that item is refused until it is fixed in Zotero. Local API requests send
no browser `Origin`, bypass proxies and check `Zotero-API-Version`.

## 0007 Formatting (with 0028)

Formatted citations use Zotero's maintained build of citeproc-js, pinned by
commit and hash, running in embedded QuickJS with ICU4X collation inside
`ibis-csl`. All official CSL styles and locales ship by 0.4. Key outputs
(Pandoc, BibLaTeX, LaTeX, Typst and others) come from `ibis-core`, escaped per
syntax, with a warning when a syntax cannot say something. Outside a document,
Ibis copies only what one citation can get right: numeric styles copy the
entry without its number, note styles the full first note (never ibid.), and
author-date year suffixes wait for a document-level view.

## 0008 Offline by default

No telemetry, ever. Everything works offline. Update checks, style updates,
metadata lookup and reference checks are each off until the person turns them
on, say exactly what is sent, and are listed in [PRIVACY.md](../PRIVACY.md).
A new network use needs an entry here and the maintainer's yes. Requests to
Zotero on `127.0.0.1` are local reads, not network use: Zotero's local API and,
read-only, Better BibTeX's `user.groups` and `item.citationkey` on the same
port, with no proxy and no redirect (maintainer, 2026-10-27).

## 0009 Insertion

Every insertion ends in exactly one visible state: pasted, or copied with a
reason. Ibis captures the target app, writes every clipboard format, restores
focus and pastes with a layout-aware shortcut. The citation stays on the
clipboard: restoring the previous contents could paste them into a slow app, so
restoring and per-app rules wait until after 1.0. It refuses secure input,
elevated windows and lost targets. Pasting is on by default (maintainer,
2026-10-25, #27); Settings › General › Pasting turns it off, saved as
`paste: false` (absent means on), and Enter then only copies.

## 0010 Search

Proposed, awaiting measurements. Keyword search is instant, ignores accents and case, works in every script and
says why each result matched; exact key and title matches rank first.
Meaning-based search, after 1.0, runs a small local model only while the picker
is open. No cloud.

## 0016 Settings (with 0018, 0020, 0022, 0023 and 0027)

`settings.json` in the platform configuration folder for `com.ibiscite.ibis`
(Nightly: `com.ibiscite.ibis.nightly`). Saves are atomic and flushed, keeping
the previous file as `settings.json.bak`. Until 1.0 there are no migrations:
settings Ibis cannot read are kept as a copy, Ibis starts from defaults and
says so, and it never silently overwrites a file it could not read. Existing
migration code is deleted, not extended, when its area is rebuilt. From 1.0 a
format change needs an upgrade path.

## 0019 Updates

Tauri's updater, run from Rust only. Opt-in, asked once; it sends one HTTPS
request to GitHub for a small feed file, and installs only after the person
agrees. Packages are signed with the Ibis update key, built into the app.
Nightly and stable keys are separate; the stable key is usable only in a
protected environment the maintainer approves. Ibis and Ibis Nightly are two
apps with their own identifier, settings, shortcut and feed. Update failures
never interrupt work.

## 0021 Interface foundation

React Aria Components for interactive parts, pinned exactly, imported only by
Ibis's own components in `apps/desktop/src/ui/`; no other component library and
no CSS framework. Tokens are hand-written CSS in `tokens.css`, in three layers
(base, role, platform look); components read role tokens only. The menu bar,
tray and context menus are the operating system's own.

## 0024 Window permissions

Every Ibis command is declared in Tauri's app manifest and granted per window;
remote origins get none. A test checks the grants through Tauri's IPC
dispatcher.

## 0027 Process until 1.0 (with 0011, 0015 and 0017)

Agents have standing engineering and merge authority and work by
[AGENTS.md](../AGENTS.md): no compatibility code, one independent review only
for risky areas, required "CI result" before every merge. Stable releases wait
for the maintainer's one-click approval.
