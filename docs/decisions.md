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

The maintainer allows NCSA for `libfuzzer-sys` exactly `0.4.13` so the test-only
`ibis-fuzz` crate can use libFuzzer (2026-10-29). The exception applies only to
the fuzz crate; this dependency must never enter shipped artifacts.

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
as a fallback people can bind. The desktop owns the trigger; Ibis never saves
it to `settings.json` or registers an XWayland grab. Portal activation tokens
stay native and are consumed through GTK's safe window API (already linked by
Tauri), never sent to the WebView or stored (2026-10-28, #782).

2026-10-31 (maintainer): reopened before 1.0. Tauri stays unless it fails a release
gate that the shell itself causes (whole-process memory including WebView helpers,
summon and typing latency, idle use, screen readers, input methods, focus for
paste) and an alternative passes that gate for less than fixing the cause in Tauri.

## 0006 Sources

Ibis never writes to a library. BibTeX and BibLaTeX entries are parsed one by
one; broken entries, duplicate keys and undefined macros become warnings with
line numbers. A repeated BibTeX key makes every entry under that key unavailable,
including as an inheritance parent (2026-10-30, #1519).
Zotero is read through its local API while it runs and from a
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
generated author-date year suffixes wait for a document-level view. Source-supplied
literal dates and year letters are retained as data (2026-10-30, #1519).
Literal years retain numeric date order independently of their rendered letters.
Ordinary ALM/LRM/RLM direction marks retain their text meaning; embedding,
override and isolate controls are neutralized before output (2026-10-30, #1522).
BibTeX imports retain stored title capitalization, including unprotected proper
nouns. Styles and citeproc own presentation; APA uses the pinned processor's
capitalization at explicit subtitle boundaries, preserving abbreviation periods
and protected spans, and reports that titles need sentence case. Fixed-code,
item-indexed citation warnings carry source conversion uncertainty without field
text or paths through engine, MCP, desktop preview and copy feedback; that
uncertainty participates in citation fingerprints. MCP omits `warnings` when
empty, preserving clean response shapes; warned responses include the same
codes as the engine and command line
(2026-10-31, #1530).

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

2026-10-30 (maintainer-approved): A weekly GitHub workflow reads only the rolling
`stable` release's public asset names, aggregate download counters and asset
`updated_at` timestamps, appending rows of UTC date, asset name, `download_count`
and `updated_at` to `counts.csv` on the orphan `stats` branch. The asset timestamp
identifies the upload for each counter; no version is inferred or `latest.json`
downloaded. Timestamp order cannot establish publication completeness, so it
does not gate snapshots. These support rough download/update estimates, not
counts of people or active installs; replacements reset counters and downloads
between snapshots can be lost. No telemetry, identifiers, app changes or server:
nothing leaves the app.

## 0009 Insertion

Every insertion ends in exactly one visible state: pasted, or copied with a
reason. Ibis captures the target app and window, writes every clipboard format, restores
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

Unrecognised apps only copy with `unknown-app` (#1520). Recognising an output
syntax or remembering a format does not establish that an app's paste key is
safe. This deliberately refuses unlisted remote viewers, terminals and editors
whose shortcuts are unverified, without growing a guessed app-ID denylist.

The footer names the intended action and output together, from that same
native policy. Enter during search or preview loading waits for its current
query and citation; repeated activation of that unchanged citation is guarded
through hiding and delivery, so it cannot paste twice (#1363). A deliberately
changed citation can supersede an outstanding reply without accepting its
obsolete feedback. The nonactivating receipt
reports actual delivery, including why pasting was off or unavailable. More
keeps that outcome in memory for this run, without citation text or a new
file. A prepared shelf remains actionable through empty searches.
Queued Enter also waits for result/status revision reconciliation, and every
add/remove activation cancels it, even with no highlighted result. Outstanding
copies are guarded by citation, output and picker opening independently of the
search query. More follows the newest successful copy; a later failed or
unfinished attempt cannot discard its delivery receipt (#1401 review).
Fresh activations share one ready-citation owner: they either start a copy
or join its outstanding operation, renewing completion ownership without
another clipboard write or delivery. Status-read failures end reconciliation
and cancel queued Enter; recovery requires fresh activation (#1401 round 2).

The native receipt also owns an immutable request generation (#1520 review).
It rechecks ownership under the notice publication lock before changing text,
showing the window or starting a timer. A refused A finishing after B cannot
replace B's delivery message; delayed or duplicate write tokens cannot claim
the latest receipt. Copies without native paste proofs use request ordering.

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
block. `paste/permission/drag.rs` is also scoped to allow AppKit's drag-source
protocol implementation, NSObject initialization and the image-contents setter:
the generated bindings offer no safe source constructor or typed image setter.
It uses the existing objc2 bindings, with no new dependency (2026-10-28, #1390).
The Settings-only command accepts no file path; it publishes only
`NSBundle.mainBundle.bundleURL` as a file URL from a live Settings mouse drag,
refuses unbundled executables, and allows only copy even with modifier keys.
Its stateless source stays on the main thread for the session's lifetime.
Changes to these modules need independent security-boundary review.

`sizing/display_events.rs` is also a scoped exception (2026-10-31): the locked
bindings provide only unsafe registration for AppKit screen-parameter
notifications and Win32 window subclasses. It uses the existing objc2/block2
and winsafe bindings (enabling winsafe's `comctl` feature), retains only an app
scheduling handle and the sizing owner, reads no native notification payload, and forwards
Windows messages unchanged, removing its subclass at native destruction.
These events request one settled geometry read; they add no timer or second
geometry owner. GDK monitor signals use safe bindings. Failed registration
defers work-area refresh to the next show; it never falls back to polling.
Work-area changes that produce no platform notification also wait for the next
geometry event or show; live constraints cannot track an unannounced change.
In locked tauri-runtime-wry 2.12.2/tao 0.37.1 on macOS, cloning window/webview
handles clones Tao's proxy, which adds a CFRunLoop source and wakes the loop.
An unconditional `get_webview_window` from `MainEventsCleared` therefore
perpetuates itself even while hidden. The sizing owner must consume its pending
read before that lookup, independently of whether the read succeeds, so a
getter's wakeup cannot schedule another read.

Display notifications also request one coalesced app-queue task. Tao 0.37.1 on
Linux emits `MainEventsCleared` only after its own event queue becomes nonempty;
a GDK signal alone does not advance it. Windows display/work-area messages can
also leave Tao's event runner idle when no theme/window event is emitted.
macOS's before-waiting observer already emits cleared events, but all three
adapters use the same bounded task. Locked Wry executes `run_on_main_thread`
inline on its main thread, so dispatch starts on Tauri's existing blocking pool
to enqueue a real Tao event. The sizing owner coalesces notifications until that
task drains, even if an earlier cleared event, hide or show consumes the read.
The task reads current geometry only when still pending and never marks a new
change. Failed enqueue releases coalescing without scheduling a retry; a later
notification may try again. No timer or per-notification thread is added.

The Windows-only qualification module `paste/windows_bstr_test.rs` is a
cfg(test) exception (2026-10-30, #1452). Safe wrappers either decode lossily or
truncate embedded NUL through PROPVARIANT conversion (actual Windows proof in
#1446). After an exact owned VT_BSTR tag check, it borrows only that active
member while its immutable VARIANT owner lives, checks the SDK byte count
against the bounded UTF16 length, and returns strictly decoded owned text.
It never moves/frees the borrowed BSTR or exposes a reference; odd binary byte
counts and malformed UTF16 refuse. The SDK-owned odd-byte fixture uses the
same scoped module. No production allowance or native-input policy changes.

For secure typing on macOS, the boundary is `IsSecureEventInputEnabled`,
whose SDK result is a Boolean (`u8`). It is read on the main thread, as Carbon requires. The
flag is system-wide and can be enabled by another app; it does not establish
that the destination is a password field. An enabled flag copies with the
`secure-input` reason: "Copied. Secure typing is on, possibly in another app.
Press ⌘V to paste." An unavailable or timed-out check uses `paste-unavailable`:
"Copied. Press ⌘V to paste." Neither outcome claims password-field detection.
The AX password/subrole guard remains removed (#1280, #1360). macOS auto-paste
requires two separate authorizations: PostEvent to send the shortcut and
Accessibility to prove its destination. The identity-only proof below adds an
Accessibility requirement compared with the old PostEvent-only contract.
Both checks are passive; they never request trust or show a permission prompt.
Without Accessibility, Enter copies with `accessibility-required`:
"Copied. Ibis needs Accessibility access to paste safely. Press ⌘V to paste."
Provider errors, unsupported attributes and timeouts remain `paste-unavailable`.

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

For #1520, the proof also freezes a lifecycle epoch before the clipboard write
and a sleep-inclusive monotonic timestamp. Workspace sleep, display sleep,
wake and session changes, and screen lock/unlock notifications retire the epoch.
Synchronous libnotify check tokens cover lock/unlock and Core Graphics session
changes before delayed observer callbacks. A bracketed comparison of the SDK's
sleep-inclusive and awake-only monotonic clocks also retires the epoch when
their offset changes; unavailable or wider-than-100 µs samples refuse. The
1,500 ms lifetime is checked against both active and sleep-inclusive time.
A measured timeout or known lifecycle change uses `paste-expired`; missing,
failed, backwards or exhausted safety evidence uses `paste-unavailable` (#1529).
Own writes and the final checks/post share the target-state lock, with a unique
current write token, so superseded work cannot post even with reused counters.

The WindowServer window-count heuristic is removed after #1529 review. Neither
the active app nor front-to-back ordering proves keyboard ownership: a foreign
nonactivating text panel can be key while Word stays active. Refusing every
multiwindow app is also not an acceptable focused-window implementation.
On 2026-10-30 the maintainer chose option A: narrowly amend this decision and
#1241's PostEvent-only contract to require Accessibility for a focused-window
identity proof. Round 2 of #1529 explicitly distinguishes the two grants and
extends surface identity to sheets/drawers. The complete
AX attribute allowlist is `AXFocusedApplication`, its `AXFocusedWindow`, the
system-wide `AXFocusedUIElement`, and that element's `AXWindow` and
`AXTopLevelUIElement` relations.
Only opaque AX object equality and their PIDs are used. The element read
establishes the actual global focus owner, including a foreign nonactivating
panel; it reads no field roles, values or secure-field attributes. The app,
focused window and containing window must agree. Both the document window and
top-level input surface are retained and compared to the capture. `AXWindow`
excludes attached sheets/drawers; `AXTopLevelUIElement` distinguishes those
surfaces even while the parent document stays focused. Both relations are
reread before the last global-focus read, including when a control keeps its
identity while moving to a different surface. Missing top-level evidence never
falls back to the document alone. No titles, roles/subroles, document text, values,
tree traversal, attribute enumeration, descriptions or logs are allowed.
Single- and multiwindow apps follow the same focused-window identity contract.
At capture, Ibis first checks whether one of its own native windows is key.
A repeated show while its nonactivating picker owns the keyboard keeps the
previous external capture and AX identity bank, even if NSWorkspace still
names that external app. This reads only Ibis's own AppKit window state.

Each query has one 50 ms sleep-inclusive budget. Every queried AX object gets
the remaining messaging timeout; the system object's process-default timeout
is restored on exit. After the initial provider reads, notification, PostEvent,
Secure Event Input and clipboard-count checks are refreshed; the final adjacent
document/surface/global-focus reads share a tighter 5 ms budget, including late
successes or errors. Slow providers copy instead of extending stale evidence.
Missing Accessibility uses its own typed reason at capture and final checks.
Unsupported reads, errors, timeouts or unstable focus copy with
`paste-unavailable`; a proven changed/lost owner or window/surface uses
`target-changed`. A failed capture cannot invent a changed target.
Captured AX references stay on the main thread, retained only in memory with a
nonwrapping token; workers carry the token, never the reference. A final query
only compares and cannot replace the captured identity. There is no idle or
hidden polling; queries run only during capture and delivery. Provider latency
adds at most the bounded capture wait to opening and bounded checks to delivery;
native latency remains to be measured.

Settings reports permission granted only when both passive preflights succeed,
including PostEvent=true/Accessibility=false. Its guide names Privacy &
Security's Accessibility list ("Device Control and Data Access" on macOS 27+).
The existing Open System Settings action can request/repair PostEvent only on
that explicit click; it never requests or resets Accessibility. A missing
capture authorization stays in its one-use proof after a later grant. Granting
does not replay delivery or summon the picker: the guide closes on a granted
return, and the person returns to the external document, opens Ibis again and
presses Enter. Only that fresh external capture can establish a new proof.

Even with this focused-window proof, public `CGEventPostToPid` addresses
a process, not a window. Public APIs do not provide an atomic foreign-window
binding for this shortcut. All ownership locks and shared lifecycle state are
acquired before the final AX identity reads; synchronous notification checks
also precede those reads. Completion checks use only sleep-inclusive time,
a locally owned awake-clock correlation and the atomic epoch, so suspension
during the read also expires delivery without taking a mutex or querying a
provider. The final local identity comparison immediately precedes posting;
Ibis takes no lock or extra provider query after it. There remains a small
interval between the last OS focus read and PID-directed dispatch, including
local validation. Public APIs
cannot eliminate a focus change in that interval. AX also samples global focus
and its document/surface relations sequentially, not in an atomic OS snapshot;
movement during those final adjacent reads remains part of the gap. This is not
atomic binding.

The focus, authorizations, clipboard and session state are separate OS observations.
A complete lock/unlock/session cycle wholly inside that final tail can evade
the notification fence if main-thread callbacks have not run and focus returns
unchanged; local clock correlation independently catches sleep. These narrow
intervals remain native qualification limits, not guarantees of atomic input.

There is no window-name lookup, screen capture, automatic permission prompt or
hidden polling. `paste/macos/focus/native.rs` is a scoped unsafe exception for the
public AX APIs; CF ownership/equality use the already-locked
`objc2-core-foundation` safe binding directly. `paste/macos/safety.rs` remains a
scoped unsafe exception for SDK clocks and observer registration. It uses already-locked
`block2` directly. Clock/notification logic has synthetic coverage; real OS
producers, focused-window providers and native keyboard placement still
require qualification.
Clock sampling has its stated 100 µs resolution limit.

The process-lifetime notification registration in `macos_notifications.rs` is
also a scoped unsafe exception (2026-10-31). It uses the locked Foundation and
block2 bindings with no object filter or operation queue, ignores notification
payloads, and dispatches fresh reads to the native thread. Keyboard-layout
notifications keep the opening shortcut current while hidden; workspace
accessibility and distributed color/theme notifications replace the appearance
timer. Appearance is also read synchronously before showing the picker.
The webview's library-age timer exists only while the picker is shown. These
changes remove periodic hidden wakeups; capped unavailable-file retries and
the updater's six-hour check are unchanged.

A reloaded picker's presentation starts unknown. Enter waits for a native
identity before starting a copy; initialization failures get one recovery read
per explicit action, with no timer, and hide/reopen retires old reads. Escape
can pass an unknown identity: native command entry snapshots the current owner
before dispatch, so a subsequent reopen still rejects that delayed hide.
Windows show requests from workers dispatch preparation, appearance refresh
and presentation together to the main thread. No worker may hold the appearance
cache while synchronously asking that thread for an HWND (2026-10-31, #1558).

Other macOS native calls use objc2's generated AppKit and Core Graphics
bindings. On Windows, `winsafe` supplies safe Win32 calls (the foreground
window, its process, name and integrity level, and `SendInput`), and
`uiautomation` supplies UI Automation password-field checks. A new binding
crate needs `cargo deny` and a line here. Only when no safe binding exists
does one small module get a scoped `#[allow(unsafe_code)]`, with its crate's
lint relaxed to `deny`, a safety comment on every block and independent
security-boundary review (2026-10-27, #1133).

A Windows copy freezes its target, HWND, picker generation and both safety
timestamps at native request entry, before waiting on target state or clipboard
I/O (#1529). Clipboard publication only adds the proved final sequence and write
token; it never mints a new timestamp after a suspension or delay.
Its one-use delivery ticket records the final clipboard sequence only after
closing Ibis's own successful write and proving that no foreign write took
over, including writes that keep the owner HWND. Neither a notice nor its
worker adopts a later capture or clipboard baseline, and delivery expires
1,500 ms after the frozen pre-I/O start. Final checks revalidate that sequence,
generation, hidden picker, HWND and process after the password query and
again after the injection lock and any key-release recovery waits. Windows
never reads clipboard contents to prove ownership.
Changed clipboard sequence uses `clipboard-changed`: "Copied. The clipboard
changed; copy the citation again". Unavailable sequence evidence uses
`paste-unavailable`, with the platform's manual-paste key. The failure that
refused injection is retained rather than re-querying mutable state to word it.

The #1520 audit also moves Windows expiry before foreground restoration and
refuses invalid, hidden or minimised HWNDs. A sleep-inclusive `GetTickCount64`
age check runs before restoration and after UI Automation and injection-lock
waits. Windows still lacks a short lock/unlock/session epoch; its targetless
`SendInput` has an unavoidable final check-to-send gap, and HWND/PID reuse and
focused-field changes remain qualification/follow-up limits. This change does
not certify those cases. Linux has no automatic paste/input adapter and always
copies with `unsupported-system`, so no pending native paste can survive there.

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

Planned copied-key warnings (#1092; maintainer-approved, 2026-10-30) may keep a
bounded private local record of library/item identity and its last successfully
copied key. This is not document or research history: no queries, document text,
quotations or personal comments. Failed or superseded copies cannot update it;
failed storage keeps the previous durable record and reports that it was not
updated. The implementation defines the capacity and file format under the
existing atomic-write and single-owner rules. Users can forget the record.
Only native identities that survive a key rename qualify; never infer continuity
from titles/DOIs when a `.bib` entry key changes its identity. A successful new
key copy replaces the baseline, not the keys in any manuscript. Clipboard success
is the commit event; diagnostic old/new-key copies, previews, CLI stdout and MCP
formatting do not count. Batch history writes occur outside the source/clipboard
barrier and preserve successful copy order rather than disk-completion order.
Stable and Nightly stay isolated.
This permission does not change MCP's exclusion of notes, annotations and private links.

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
Settings and the picker share one frontend patch queue for ordering, field
acknowledgments, stale reads and recovery. Settings restores the saved choice
after a failed save; the picker retains explicit choices for retry. Each window
keeps its own queue; the native lock coordinates windows (2026-10-28).

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

2026-10-30: PR #1504 review found that an automatic dialog could steal Enter
from Search and install. Automatic offers now wait for empty, idle input; all
native defaults decline, and only deliberate Install activation accepts. Idle
reports carry one-use interaction ownership and cannot revive after hide/reopen.
No input text is sent to the updater. See the native-source audit in
[testing](testing.md#update-consent-keyboard-safety).

The maintainer approved publishing the current engineering closeout through
Release Please as 0.2.1 after its PRs pass review and CI (2026-10-29).
Stable deployment retains the maintainer's one-click GitHub approval.

2026-10-31: Numbered releases are full GitHub releases titled “Ibis X.Y.Z”, tagged
`vX.Y.Z`, with the highest version carrying Latest; this replaces marking every
numbered release pre-release so GitHub shows one Latest and `/releases/latest`
works. Before 1.0, preview status stays in the notes and website. “Ibis Nightly”
uses the rolling `nightly` tag, with its version in the notes; “Ibis update feed”
uses the fixed `stable` tag, whose release's files are replaced. Both are
pre-releases and never Latest.

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
checks fail pull request CI; checks with too little margin on shared runners
report there and are enforced on the release machines before each release.
0.5 cites from Zotero's annotations and the PDFs people already open before
any reader inside Ibis (maintainer, 2026-10-28).

2026-10-31 (maintainer): limits come from what a person notices, at the size real
libraries reach. They block a release at 10,000 generated realistic records
(abstracts, long author lists, Chinese, Japanese and Korean text): shortcut to a
typeable picker p95 ≤ 100 ms; keystroke to results p95 ≤ 50 ms (engine search
≤ 20 ms); warm formatted preview p95 ≤ 100 ms and the first after launch ≤ 1 s;
first results ≤ 1 s after launch; citable again about 2 s after a source edit, with
search never interrupted; hidden whole-app memory ≤ 200 MiB, web view included;
hidden idle ≤ 0.2 % of one core and ≤ 1 wakeup a second; picker JS and CSS ≤ 400 KiB
gzipped. A 50,000-record check reports on nightly against the same numbers as a stretch
goal and must not grow faster than the library; a miss is studied, not blocking. The 100k limits and the 400-entry bibliography timing are dropped; document
tools set their own later. Where each check runs: #1536.

## 0027 Process until 1.0 (with 0011, 0015 and 0017)

Agents have standing engineering and merge authority and work by
[AGENTS.md](../AGENTS.md): no compatibility code, one independent review only
for risky areas, required "CI result" before every merge. Stable releases wait
for the maintainer's one-click approval. A plan or answer the maintainer
approves is recorded the same day in these files, the issues and the tracking
issue; nothing lives only in a chat, a tool's memory or a local file
(maintainer, 2026-10-28).


2026-10-29: The maintainer authorizes incremental merging and shipping of
completed fixes, including the picker fade, logo and compact-window work,
after required review, CI and budget gates pass; stable deployment keeps its
one-click approval. The website demo is critical and stays functionally
aligned with the released app; Settings #776 remains future polish.
