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
On Windows the closed reader's process, drive, attribute and ACL checks call
Win32 directly through `windows-sys`, never PowerShell (changed 2026-10-28 in
#1261: PowerShell was slow and fails under AppLocker and Constrained Language
Mode). That module and the macOS secure-input boundary are the scoped
`unsafe` exceptions described in decision 0029; their crates deny it
elsewhere and the workspace forbids it.

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
Ibis may serve Better BibTeX-compatible Cite as You Write to editor plugins:
`127.0.0.1` only, on its own port, refusing any request with a browser
`Origin`, and off until switched on in Settings (maintainer, 2026-10-27).

## 0009 Insertion

Every insertion ends in exactly one visible state: pasted, or copied with a
reason. Ibis captures the target app, writes every clipboard format, restores
focus and pastes with a layout-aware shortcut. The citation stays on the
clipboard: restoring the previous contents could paste them into a slow app, so
restoring and per-app rules wait until after 1.0. On macOS it refuses while
system-wide Secure Event Input is on, including when another app enables it;
this flag does not identify the destination's field or cover every password
control. On Windows, elevated targets and UI Automation password fields are
refused; an unavailable or timed-out field check only copies, without claiming
password detection. Terminals, remote desktops, VMs and lost targets are
refused on both platforms. Pasting is on by default (maintainer,
2026-10-25, #27); Settings › General › Pasting turns it off, saved as
`paste: false` (absent means on), and Enter then only copies.

When the shortcut opens the picker with short text selected in the target app,
that text becomes the search and the paste replaces it (macOS and Windows, on
by default; maintainer, 2026-10-28). The selection is read once, at the
shortcut, through accessibility access separate from paste's PostEvent permission; it is never read
from secure fields, never stored and never logged.

## 0029 Native calls

The workspace keeps `unsafe_code = "forbid"`. Native calls use maintained
safe bindings where available. Decision 0006's Windows closed-reader module
and `paste/macos/secure_input.rs` are scoped exceptions: their crates use
`deny`, with an allow only on the native module and a safety comment on each
block. Changes there need independent security-boundary review.

On macOS the boundary is just `IsSecureEventInputEnabled`, whose SDK result
is a Boolean (`u8`). It is read on the main thread, as Carbon requires. The
flag is system-wide and can be enabled by another app; it does not establish
that the destination is a password field. An enabled flag copies with the
`secure-input` reason: "Copied. Secure typing is on, possibly in another app.
Press ⌘V to paste." An unavailable or timed-out check uses `paste-unavailable`:
"Copied. Press ⌘V to paste." Neither outcome claims password-field detection.
The AX guard is removed: optional subroles, denied AX trust and dormant
providers must not disable ordinary paste (#1280, #1360). Paste requests
only PostEvent permission, never Accessibility trust; no AX attributes are
read or providers activated.

On macOS, Ibis snapshots the pasteboard change count immediately after writing
every format, checking that ownership did not change during publication. A
one-use in-memory proof binds that count to Ibis's citation, captured app,
picker presentation and write time. Reopening the picker retires that proof;
repeated hide/blur notifications do not. The final foreground, Secure Event Input,
change-count and deadline checks run with posting in one main-thread task,
after layout/event preparation. No clipboard contents are read back. The
task has at most 250 ms and delivery at most 1,500 ms from the write; expired
or cancelled queued tasks cannot post later. Cancellation and posting share
a lock so a completed paste is never reported as merely copied.

Other macOS native calls use objc2's generated AppKit and Core Graphics
bindings. On Windows, `winsafe` supplies safe Win32 calls (the foreground
window, its process, name and integrity level, and `SendInput`), and
`uiautomation` supplies UI Automation password-field checks. A new binding
crate needs `cargo deny` and a line here. Only when no safe binding exists
does one small module get a scoped `#[allow(unsafe_code)]`, with its crate's
lint relaxed to `deny`, a safety comment on every block and independent
security-boundary review (2026-10-27, #1133).

A Windows copy freezes its target, HWND and picker generation before writing.
Its one-use delivery ticket records the final clipboard sequence only after
closing Ibis's own successful write and proving that no foreign write took
over, including writes that keep the owner HWND. Neither a notice nor its
worker adopts a later capture or clipboard baseline, and delivery expires
1,500 ms after the write receipt. Final checks revalidate that sequence,
generation, hidden picker, HWND and process after the password query and
again after the injection lock and any key-release recovery waits. Windows
never reads clipboard contents to prove ownership.

On both systems, a notice carries a unique write token from the successful
copy. Only a matching token can consume its one-use proof; a delayed earlier
delivery leaves a newer proof available, even when the citation text or
clipboard sequence is the same. Tokens never wrap; exhaustion only copies.

UI Automation checks only the focused element's process and `IsPassword`.
An enabled property copies with `secure-field`; unavailable or timed-out
queries copy without that reason or macOS's `paste-unavailable` notice. The
caller waits at most 250 ms. A timed-out worker keeps its per-process permit
until it exits; no requests queue and at most four target processes can have
queries outstanding. A stuck provider in one app does not prevent another
app from pasting. Executable display names run on a separate worker, with a
50 ms wait and executable-name fallback. Partial `SendInput` releases only
keys left down by its accepted prefix, retries failed key-ups and retains
unreleased keys for recovery before another paste. Failed recovery copies
with `key-release-failed`, asking the person to press and release Ctrl and V.

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

## 0031 What Ibis remembers per app

The output chooser remembers a format picked for the captured app unless the
person selects "Just this once". `apps.json`, beside `settings.json`, keeps
only the operating system, normalised app ID and output for at most 200 apps;
the least recently chosen app is dropped first. Never a document title, URL,
window title, document text or citation. Browser and editor IDs cannot identify
a project's syntax, so the first paste in each browser or code editor opens
the output chooser once and remembers the pick (maintainer, 2026-10-28).
Remembered choices beat app-table suggestions, then the
global fallback; "Automatic" removes only the app override, and a deliberate
global choice never erases app choices.

The bounded file is written whole, flushed and swapped in atomically. A file
Ibis cannot read means automatic/default outputs, with a visible warning; the
next explicit app save replaces the damaged data. "Forget formats for apps"
clears it. A failed save leaves the previous durable choices intact and says
so. Preference writes from different windows apply only deliberate field
patches under the native owner lock, returning the authoritative saved state.

## 0019 Updates

Tauri's updater, run from Rust only. Opt-in, asked once; it sends one HTTPS
request to GitHub for a small feed file, and installs only after the person
agrees. Packages are signed with the Ibis update key, built into the app.
Nightly and stable keys are separate; the stable key is usable only in a
protected environment the maintainer approves. Ibis and Ibis Nightly are two
apps with their own identifier, settings, shortcut and feed. Update checks,
offers and failures never interrupt work: an available update never shows or
focuses the picker; it waits in More, the tray and Settings with one sentence
on what changed. An opt-in "Download quietly, install when I quit" downloads
and verifies in the background and installs only on an explicit quit; asking
first stays the default (maintainer, 2026-10-28).

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

## 0030 Links and launchers

`ibis://` links open the picker with a query, output or library filled in.
A link is untrusted input: it never copies, pastes, changes settings or reads
files; only the person's Enter acts. Launchers (Spotlight through Shortcuts,
Alfred, Stream Deck, notes apps) reach Ibis through these links and the `ibis`
command, with recipes in the docs. No native App Intents or launcher
extensions before 1.0: they would add a second language or codebase for what
the global shortcut already does (maintainer, 2026-10-28).

## 0032 Release gates

From 0.3 a release ships only when the whole citation (search, choose, page,
paste or copy as that platform supports, back in the document) works with
VoiceOver and NVDA and with Chinese,
Japanese and Korean input methods; Orca joins for 1.0. Interface text goes
through the translation layer from 0.3, with German and one right-to-left
language in 0.4. The speed and resource budgets in
[architecture](architecture.md#desktop-app) are limits: search and bundle
checks fail pull request CI. Open/refresh fails Preview's nightly/release build
because shared PR runners have too little timing margin. Native latency and idle
use are measured on the release machines before each release.
0.5 cites from Zotero's annotations and the PDFs people already open before
any reader inside Ibis (maintainer, 2026-10-28).

## 0027 Process until 1.0 (with 0011, 0015 and 0017)

Agents have standing engineering and merge authority and work by
[AGENTS.md](../AGENTS.md): no compatibility code, one independent review only
for risky areas, required "CI result" before every merge. Stable releases wait
for the maintainer's one-click approval. A plan or answer the maintainer
approves is recorded the same day in these files, the issues and the tracking
issue; nothing lives only in a chat, a tool's memory or a local file
(maintainer, 2026-10-28).
