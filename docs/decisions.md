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
stable error codes; an error may add a stable reason, and front ends choose
their words from it rather than guessing from the code. A wrong style or
locale is a request error, not a refusal of each item (2026-11-02, #1618).
One citation request, one result, with typed losses and
refusals instead of plausible but wrong text. Records, attachments, notes,
annotations, collections, tags and saved searches are addressed by
`library + native key`; Ibis maps Zotero's raw data to CSL itself so Zotero
open and closed give identical results. Records keep only the normalised data
Ibis uses; the source stays the original, and an export that needs the original
entry reads it from the source (2026-10-27: in-memory raw copies dropped, which
nothing read and which doubled memory). Details: [architecture](architecture.md).

2026-11-02 (maintainer): the engine is the soul of Ibis. It stands on its own and
is developed in tandem with the app, with no compromises; the GUI is only a layer.
It must work with AI agents, run on every operating system and be there for anyone
to build on.

2026-11-04 (maintainer, planning only): correctness across the app, command
line and agents is the priority. Three rules apply to that shared contract:

- **Separate task contexts.** Saved project definitions and reference updates
  can be shared; each writing task keeps its selected project, source scope
  and output. Another client's project selection never redirects ongoing work.
  An explicit “use the current Ibis project” captures that choice when the task
  starts, rather than continuously following the picker.
- **Refuse conflicting project keys.** When a selected project's bibliography
  is known to bind a key to a different work, refuse that key output until the
  conflict is resolved. This replaces warn-and-copy for that case; copying
  instead of pasting does not make the conflicting key safe.
- **Keep a last-good snapshot usable.** An ordinary refresh failure permits
  continued citation from a coherent snapshot of the same established source,
  with a clear warning and the last successful read time. No repeated
  confirmation is needed. This does not override refusals for uncertain source
  identity, changed selected records or conflicting keys. The same request and
  explicit policy have the same outcome across clients; strict mode remains
  the explicit option planned in #1538.

These are planned rules, not a claim that implementation has landed. They do
not choose a shared engine process. Context and source ownership are described
in [architecture](architecture.md#task-contexts-planned); visible behavior is
in [design](design.md#source-checks-and-passage-reuse-planned). Follow-up belongs
to #1697, #1260 and #1538, with the work order on #1300 unchanged.

2026-11-04 (maintainer, planning only): unreadable project bibliographies use
the evidence available for the selected key and work, rather than a blanket
refusal or blanket warning:

- A retained, complete and coherent copy of the same selected bibliography
  may support the match, including for a newly selected reference. Normal
  Enter, copy and paste remain available with a clear last-checked warning
  and time; the result never claims a current successful check.
- Without usable evidence for the match, pause normal insertion, preserve
  the selection and preview, and offer Retry or locating the bibliography.
  A deliberate one-off unchecked copy or paste is available, honestly named
  and reported as unverified. It does not bypass a known wrong-work conflict
  or any source-identity refusal, and agents never choose it silently.
- A newer successful read supersedes older evidence, including conflicts;
  an older matching copy cannot hide a later detected conflict. A changed
  project or bibliography cannot inherit the previous selection's check.
  Absence in a retained copy is not claimed as absence in the current file.
- Strict freshness explicitly requires a successful current check. Retry
  recovery never triggers a delayed insertion; the person acts afresh.

Visible behavior is specified in design.md's Source checks section. These
rules supersede the unresolved bibliography question in the initial review;
shared-project edits during a task remain a separate UX question.

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
including as an inheritance parent (2026-10-30, #1519); none is kept, because
Ibis cannot know which one the person meant. The warning names the key at
every entry, the first included, and looking the key up says it is repeated
(2026-11-02, #1617). A library's id is an opaque token: nothing reads a kind from
it, and it is keyed by a per-person secret so that a seen id confirms no guessed
path. That key is the one file the `ibis` command writes. A CSL `id` is never a
citation key, and a repeated one refuses every record under it (2026-11-02, #1535).
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

2026-10-31 (maintainer): Remove the saved `.bib` cache (#1498, #1515); #1534 found no first-result gain at 10k and 2.7× slower citation readiness. Parse files fresh, clean obsolete private cache files under their lease, and retain Zotero's coherent-read guard.

2026-11-04 (#1645): A project bibliography is one more `.bib` the person
chooses, read only to check the keys a citation emits; it is never searched or
written. Its path is saved in settings as a `LOCAL` value, so a backup never
carries it and a restore keeps this computer's. It is read when chosen, at launch
and when the picker opens; nothing polls or watches it. Only its file name, each
key's state and line numbers cross to the web view, never the path or the file's
text. Copy Entry writes one library entry as its file has it, `file` and `note`
fields included, from native code straight to the clipboard (listed in
[PRIVACY.md](../PRIVACY.md)).

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

2026-10-31 (#1486): The shelf consumes engine-owned reference spans. Form
examples and support reasons are read-only alternatives against the prepared
ticket's immutable snapshot and original output family; they never replace its
copy ticket. Year only uses the engine's year-only form. Late previews and
detail replies cannot publish into a newer edit or picker presentation.
Output-only display leaves preserve passive emphasis, scientific scripts,
small caps and underline; adding a reference or confirming Copied cannot flatten
them. The citation request's affix vocabulary is unchanged. Full text is
selectable in the reference menu as well as the tooltip, including for one
long reference.

2026-11-02 (#19): every official CSL style and locale ships in the binary as one
compressed bundle (`crates/ibis-csl/vendor/csl-bundle.bin`), built from tagged
upstream releases pinned by commit, so styles change only with an Ibis release.
Each file stays byte for byte upstream's, under its CC BY-SA 3.0 licence and with
its authors. Files sit in 1 MiB zstd blocks: a request unpacks only what it names,
and nothing stays in memory but the last style used. A dependent style runs its
parent's rules in its own default language. What a style does comes from what it
declares (its class, its citation number, its year suffixes, its conjunction),
never from its id; upstream's renames keep old ids working. A result names its
language, which is the request's, else the style's, and never the interface's or
the operating system's. An unknown language is refused rather than approximated,
except that a bare language means its main dialect, as upstream's `locales.json`
defines it.

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

2026-11-02, the editor endpoint as built (#42): port 23124, never Zotero's 23119.
Following the architecture's rule for local endpoints, the address carries a
random 128-bit token in its path, because `Origin` alone cannot keep a web page out:
an `<img src>` sends none. Requests with `Origin`, `Referer`, `Cookie`, any `Sec-`
header or a CORS preflight header are refused, as is any `Host` but `127.0.0.1` or
`localhost` on that port (DNS rebinding), anything but `GET`, and any body. Answers
carry no CORS header, are `text/plain` with `nosniff`, and repeat nothing a client
sent. One request waits at a time. Switching off forgets the token and refuses
connections already open, so switching on again makes a new address. If the port is
taken at launch, the endpoint switches itself off, so whoever holds the port cannot
learn the token from an editor. The token is kept in `settings.json` (and its `.bak`
until the next save), never in exported backups, logs or the picker. It exposes only
whether Ibis is ready and the citation the person chooses.

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

"References used this session" (#356) holds record identities in memory, cleared by
quitting or a reset, and leaves memory only through a file the person saves. It is a new
data flow of references already read, with no network use (2026-11-02).

## 0009 Insertion

Every insertion ends in exactly one visible state: pasted, or copied with a
reason. Ibis captures the target app and window, writes every clipboard format, restores
focus and pastes with a layout-aware shortcut. The citation stays on the
clipboard: restoring the previous contents could paste them into a slow app, so
restoring waits until after 1.0. On macOS it refuses while
system-wide Secure Event Input is on, including when another app enables it;
this flag does not identify the destination's field or cover every password
control. On Windows, elevated targets and UI Automation password fields are
refused; an unavailable or timed-out field check only copies, without claiming
password detection. Terminals, remote desktops, VMs and lost targets are
refused on both platforms. Pasting is on by default (maintainer,
2026-10-25, #27); Settings › General › Pasting turns it off, saved as
`paste: false` (absent means on), and Enter then only copies.

The first Enter into an unrecognised app with a proven target opens the existing
output chooser (#1520; maintainer correction to #1529, 2026-10-31). It offers
"Paste into [app name] from now on" or "Copy only", remembered by OS app ID in
the existing destination settings. Either answer keeps the citation and requires
a fresh Enter; dismissal and a format pick alone grant no paste approval.
Failed saves do not approve an app. Unapproved unknown copies use `unknown-app`;
an explicit copy-only choice uses `copy-only`. Refused terminals, remote viewers,
VMs and editors whose normal platform paste key means something else never ask.
Neither an explicit choice nor a saved format can override their refusal, a
missing window/Accessibility proof, expiry or the one-paste limit. The copy
proof freezes app approval before clipboard I/O, so delayed approval cannot
authorize an earlier copy. Saving a delivery choice or reset revokes outstanding
native proof epochs under the same lock as proof publication and posting. Detached workers
check that epoch too. The picker gives delivery its own generation: fresh Enter
can join an outstanding citation only within that generation, so saving Copy
only cannot renew a flight that froze Paste. Delivery revocation alone does not
erase confirmation of a completed clipboard write: a delayed response for the
current citation shows Copied in the open picker, without hiding or forwarding
its revoked paste proof. A newer Enter (including one waiting for a save),
citation or opening retains its own selection, output and feedback ownership.

Final injection must never wait on the UI thread while holding destination or
key locks. Windows resolves the picker's known HWND before either lock and uses
native `IsWindow`/`IsWindowVisible` checks under them; a busy destination guard
refuses instead of queuing key-downs. Its captured-window and key locks wrap only
native calls; query admission locks are released before worker waits. macOS
dispatches its final task before taking its posting locks, uses a main-thread
marker for the final checks (Tauri's visibility getter runs inline there), and
tries destination ownership without waiting. Linux only copies and takes no
posting lock across a UI dispatch. Clipboard and destination-store locks add
no UI-thread calls; receipt publication follows delivery after those locks end.

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
the latest receipt. Every successful clipboard write gets that identity under
its platform's write lock, including Linux and copies without native paste
proofs. An unidentified notice cannot claim ownership (2026-10-31, #1494).

When the shortcut opens the picker with short text selected in the target app,
that text becomes the search and the paste replaces it (macOS and Windows, on
by default; maintainer, 2026-10-28). The selection is read at the
shortcut, and again just before the paste, through accessibility access separate from paste's PostEvent permission; it is never read
from secure fields, never stored and never logged. Windows follows; macOS ships
first (#1526).

The rule behind it: **Ibis replaces only text the person saw in search.** A long,
multi-line or blank selection is ignored for search, and Enter then only copies,
with `selection-not-shown`, because a paste would delete it unseen. The selection
Enter finds must be the one the picker opened on, so text selected or changed
since refuses the paste the same way. When the opening could not read (refused,
failed or too slow) the search is empty and the final read decides: the paste
goes ahead only if nothing is selected. An app whose control does not report a
selection at all, or does not implement the call, pastes as before: Ibis cannot
tell, and refusing would end pasting there, so a selection in such a control can
still be replaced. The paste switch also turns the read off; there is no separate
setting (2026-11-02).

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
their offset changes; unavailable or 500 µs-or-wider samples refuse. The
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
tree traversal, attribute enumeration or descriptions are allowed. The opt-in
diagnostics amendment below permits only numeric/boolean evidence and fixed stages.

One exception reads content, and only for "selected text as the search"
(#1526): `AXSubrole`, `AXSelectedTextRange` and `AXSelectedText` of the focused
control, three fixed names in their own closed set. It runs only for an app Ibis
may paste into (never copy-only, refused or unapproved, and never with paste
off), after a capture proved the focus, with Secure Event Input off and a focused
control the target process owns. A failed or unprovable subrole, or
`AXSecureTextField`, stops it before the range is read; a range over the limit
never copies the text across, and an app without a range sends it, then only
its length is used. The read has its own sleep-inclusive budget, 30 ms at opening
and 60 ms just before the paste, and a failure at opening means an empty search.
It uses no clipboard and posts no keys. The text is emitted once for the picker
window, the only one that listens; native code keeps a digest, whose `Debug` prints only
its kind, for the read just before the paste.
Single- and multiwindow apps follow the same focused-window identity contract.
At capture, Ibis first checks whether one of its own native windows is key.
A repeated show while its nonactivating picker owns the keyboard keeps the
previous external capture and AX identity bank, even if NSWorkspace still
names that external app. This reads only Ibis's own AppKit window state.

Each query has one 50 ms sleep-inclusive budget. Every queried AX object gets
the remaining messaging timeout; the system object's process-default timeout
is restored on exit. After the initial provider reads, notification, PostEvent,
Secure Event Input and clipboard-count checks are refreshed; the final adjacent
document/surface/global-focus reads share a tighter 15 ms budget, including late
successes or errors. Slow providers copy instead of extending stale evidence.
Missing Accessibility uses its own typed reason at capture and final checks.
Unsupported reads, errors, timeouts or unstable focus copy with
`paste-unavailable`; a proven changed/lost owner or window/surface uses
`target-changed`. A failed capture cannot invent a changed target.
Captured AX references stay on the main thread, retained only in memory with a
nonwrapping token; workers carry the token, never the reference. A final query
only compares and cannot replace the captured identity. There is no idle polling;
queries run only during capture and bounded pending delivery. Provider latency
adds at most the bounded capture wait to opening and bounded checks to delivery;
native latency remains to be measured.

Settings reports permission granted only when both passive preflights succeed,
including PostEvent=true/Accessibility=false. Its guide names Privacy &
Security's Accessibility list ("Device Control and Data Access" on macOS 27+).
The existing Open System Settings action requests/repairs each missing grant
only on that explicit click, including Accessibility. Working grants are never
reset; development builds never reset either grant. A missing
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
provider. After the final local AX identity comparison, Ibis checks the key/click
timing described below, then finishes the owned clock/epoch and deadline guards
and posts. This final tail takes no mutex and never dispatches to the UI thread.
There remains a small
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
idle polling. `paste/macos/focus/native.rs` is a scoped unsafe exception for the
public AX APIs; CF ownership/equality use the already-locked
`objc2-core-foundation` safe binding directly. `paste/macos/safety.rs` remains a
scoped unsafe exception for SDK clocks and observer registration. It uses already-locked
`block2` directly. Clock/notification logic has synthetic coverage; real OS
producers, focused-window providers and native keyboard placement still
require qualification.
Clock sampling has its stated sub-1 ms uncertainty limit.

2026-10-31 amendment: macOS paste readiness, diagnostics and app accessibility.

Follow-up decision: Ibis enables an eligible app's accessibility support when
needed, without an environment switch. Apps such as Claude should work through
the ordinary paste flow. This still needs Ibis's macOS Accessibility grant;
enabling the other app's support does not grant that permission. A debug build
without the grant could not complete the real-Mac test.

- **Bounds, chosen from the threat rather than unmeasured provider latency:**
  the whole AX query still gets 50 ms, including sleep. Its three final sequential
  relation/focus reads share 15 ms: 5 ms per read on average, accepting the
  synthetic 3 ms/read case (24 ms for all eight reads), while capping the AX wait
  after refreshed authorization/clipboard/security guards at 15 ms. Every identity
  comparison, guard refresh and expiry remains. Clock brackets must be shorter than 500 µs,
  allowing scheduler skew above the old 100 µs limit without making continuity
  unbounded. Intersecting intervals never widens established clock evidence.
  Two brackets contribute skew to an offset comparison, so this keeps their
  combined ambiguity below 1 ms (plus nanosecond rounding), matching the existing
  sub-deadline suspension threat. This is a declared qualification limit.
  Disjoint offsets still expire, and invalid/backwards/wider samples still refuse.
  These are conservative code/threat bounds, not native measurements.
- **Readiness:** after hiding the picker, wait at most 1,000 ms, checking at
  25 ms intervals (at most 40 waits), for the original process/app AND retained
  document/surface identities. Activation alone is insufficient for a
  nonactivating picker. Only focus mismatch is pending; failed AX calls refuse.
  A new presentation/write cancels the old owner. No query can recapture a
  replacement document, sheet or restarted process. Once ready, all final guards
  and identity checks run again in the existing at-most-250 ms posting task;
  the 1,500 ms write lifetime still applies throughout. This replaces macOS's
  fixed 50 ms settle sleep. The bounded pending-delivery checks are the only
  hidden polling exception, and stop on success, refusal, cancellation or expiry.
- **Input during delivery (review follow-up):** Chrome tabs and Claude
  conversations can share the same AX window and surface identities. Ibis does
  not require a per-tab or per-conversation identity, or read content to invent
  one. It records when delivery begins, before taking the copy proof or queuing
  work. This time stays fixed through readiness and the final posting task.
  Immediately before posting, after the final AX comparison, Ibis calls
  `CGEventSourceSecondsSinceLastEventType` on the combined session state for
  key-down, left-mouse-down, right-mouse-down and other-mouse-down (including
  middle-clicks that can close a Chrome tab). Any such input since delivery
  began refuses the paste with the existing `target-changed` copied notice.
  Mouse movement does not count. The Enter that started delivery happened
  before this boundary; held-Enter repeats after it may refuse, which is safe.
  Invalid event timing refuses with `paste-unavailable`. No keys, click targets,
  coordinates or event contents are read or logged, and there is no event tap
  or background monitoring. Local clock/epoch and deadline checks still run
  after these reads without taking locks or dispatching to the UI thread.
  A programmatic tab or conversation change with no input is not detected.
  The four input reads and posting are sequential OS operations, so an input
  in that last small interval is also not an atomic window/document binding.
- **Diagnostics:** `IBIS_MAC_PASTE_DIAGNOSTICS=1` enables one JSON line to stderr
  per delivery attempt, after its outcome. Otherwise it is off. The closed schema
  contains a fixed stage, first failing AX error code (or latest success), sleep-inclusive
  call/attempt durations, clock-bracket widths in microseconds, and owner/equality booleans;
  `manual_accessibility_ran` says whether capture attempted the activation write,
  including a failed write. It stays false when no write was attempted. Absent
  evidence is null. Capture failure evidence follows the immutable target
  into delivery. No native objects, titles, roles, values, text, paths or document
  identifiers are logged; even app IDs are omitted. The final focus-to-post tail
  updates only thread-local evidence, with no logging mutex or output before
  dispatch. Reentrant and cancelled work cannot consume another attempt's line.
- **Enable the app's accessibility support automatically:** some Electron and
  Chromium apps leave out window identities until this support is enabled.
  During capture, if `AXFocusedWindow` or `AXTopLevelUIElement` returns missing
  attribute error -25205 or -25212, Ibis sets `AXManualAccessibility=true` on the
  verified application element. Both the application and the missing relation
  must belong to the captured process. The app must pass the existing paste
  policy; an unknown app needs explicit paste approval. Refused apps, Copy only,
  paste-off, unreadable preferences and missing grants never trigger this write.
  Ibis attempts it once per live app instance during an Ibis run, even if the
  attempt fails. It retains the original `NSRunningApplication` before deciding
  eligibility. Before claiming the process and again immediately before the
  AX write, that original instance must not be terminated and must still equal
  the instance returned for the PID. Looking up a new app by a reused PID never
  supplies approval to that replacement instance. The once-per-process claim
  also uses the retained original instance, rather than the PID alone.
  After a successful write, Ibis discards the partial capture and repeats the
  full identity proof within the same 50 ms budget. Delivery never enables this
  support or captures a replacement target. Ibis reads no titles, roles, values
  or text, does not walk the accessibility tree, and does not set
  `AXEnhancedUserInterface`. Enabling support may use more CPU or memory in the
  other app and make its accessibility tree available to the OS. Ibis still reads
  only the allowed identities and process ownership. It leaves support enabled
  because another assistive app may need it. This is the only permitted foreign
  attribute write; the list of attributes Ibis reads stays the same.
- **Grant repair:** missing Accessibility uses the existing
  `accessibility-required` message and opens the existing guide, once per run,
  just like missing PostEvent. Only its explicit Open System Settings action may
  reset Ibis's nonworking grant in a release build and request it; it checks the
  two grants separately. Granting never revives a failed capture or delivery.

Synthetic scenarios cover success, refusal, AX failure, delayed document/surface
return with the app already active, close/reopen, target replacement, stale
ownership, one-use diagnostics and automatic app accessibility activation. Native provider timing under
load, Electron/Chromium activation behavior, OS grant repair and actual keyboard
placement still require qualification; PID-directed posting remains non-atomic.
Review regressions also cover input that switches an interior document without
changing either AX identity, input during the final AX read, ignored mouse
movement, invalid input timing and process replacement before the AX write.

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
per explicit action, with no timer. Exhausted retries discard the queued Enter
and show the existing copy-failure alert; another Enter can retry. Hide/reopen
retires old reads. Escape
can pass an unknown identity: the synchronous native IPC handler snapshots the
current owner before dispatch, so a subsequent reopen still rejects that
delayed hide. This capture cannot live in an async command body: Tauri schedules
that body after receipt. The handler replies after the native hide completes;
main-thread dispatch runs inline there, and an off-thread caller waits without
holding application state.
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
Meaning-based search (0.7) runs a small local model only while the picker is
open. No cloud.

## 0016 Settings (with 0018, 0020, 0022, 0023 and 0027)

`settings.json` in the platform configuration folder for `com.ibiscite.ibis`
(Nightly: `com.ibiscite.ibis.nightly`). Saves are atomic and flushed, keeping
the previous file as `settings.json.bak`. Until 1.0 there are no migrations:
settings Ibis cannot read are kept as a copy, Ibis starts from defaults and
says so, and it never silently overwrites a file it could not read. Existing
migration code is deleted, not extended, when its area is rebuilt. From 1.0 a
format change needs an upgrade path.

Primary, backup and damaged-file recovery reads are capped at 1 MiB, including
growth during a read; saves must fit that same limit. Atomic replacement keeps
the existing file permissions; new Unix files start owner-only. A load problem
blocks automatic paste and reports paste unavailable, even with a valid backup
(2026-10-31, #1525).

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

2026-11-02 (#1561): the settings file works on another computer. A bibliography
inside the home folder is written as `~/…` with `/` between names, and read back
under that computer's home; other paths stay as written. The opening shortcut is
kept per system and always written as `{"macos": …, "windows": …}`, so a file
shared live between systems never hands one system's shortcut to another; plain
text written by hand is this system's. Builds before this one cannot read the
per-system form or `~/…` and fall back as for any damaged file (0016, pre-1.0).
Saves through a symlink replace the linked file and keep the link. Copies a sync
service made (iCloud `settings 2.json`, OneDrive `settings-PC.json`, Dropbox,
Google Drive, Syncthing) are named in the settings notice, never merged. A
backup (`ibis_settings_backup: 1`, `made_on`, `made_by`, `settings`) holds the
settings without the ones that belong to one computer: window places, quiet
feedback and the update question, which each computer answers for itself because
it allows network use (0008). Paste approvals live in `apps.json` and never
travel, so a restored file cannot grant paste (0031). A restore checks the whole
backup first, shows every change, is refused if the settings changed since that
preview, and saves through the normal atomic write, so the replaced settings
become `settings.json.bak`; damaged settings are kept as `.invalid-N` and
replaced, settings from a newer Ibis never are. Placeholders a cloud folder has
not downloaded are reported without reading them. The app restarts after a
restore so every part starts from the same settings. `ibis settings import`
writes only Ibis's own settings file; the command line stays read-only for
libraries and sources (0033).

## 0031 What Ibis remembers per app

2026-11-02 (maintainer): one rule replaces the format scopes. Ibis remembers a
format per app, and the person changes it in the format menu. One default in
Settings › Citations applies to apps with no remembered format. "Just this
once", "Automatic", "Use as global default" and "Forget formats for apps" are
gone, along with their Undo; a later pick replaces the old one. A format is
remembered only for a captured app. When Ibis can't tell the app, the default
applies and a pick changes it, so no choice is lost; when the destination could
not be read, the app may be a real one, so a pick holds for that paste alone. A
pick that cannot be saved also holds for that paste, and the control says "This
paste". The app table's starting formats stay, as suggestions the menu names, so
a new install still pastes APA into Word.

`apps.json`, beside `settings.json`, keeps
only the operating system, normalised app ID, output, optional delivery choice
(`paste` or `copy-only`), the identity binding described below and whether the
one-time plugin note was shown, for at most 200 apps;
the least recently chosen app is dropped first. Never a document title, URL,
window title, document text or citation. Browser and editor IDs cannot identify
a project's syntax, so the first paste in each browser or code editor opens
the output chooser once and remembers the pick (maintainer, 2026-10-28).
Remembered choices beat app-table suggestions, then the
default; a format save and a delivery save each leave the other alone, except that
answering Paste or Copy only in an open browser or editor first-paste question
accepts the format it showed, unless a format is being held for that paste. A new default never erases app choices.

2026-10-31 (#1486): Recent citations keep only opaque reference identities and
known originating app IDs in the channel's WebView storage, bounded to 20 apps,
30 identities per app and 256 Ki characters. More can forget them. Unknown
destinations and sample references are not remembered. Rows resolve against the
current snapshot; no page, form, query or citation text is restored. Failed
history writes retain in-memory recents; a failed forget remains available for
retry. The Recent citations switch (`recent_citations`, written only when off)
forgets on off and records only while known to be on, never on an unread
setting; only a new receipt records, so Forget or turning it back on never
replays an older one. Each saved "off" forgets as it arrives, so off and
quickly on again still starts empty. BibTeX added dates come from `date-added`
(BibDesk), `creationdate` (JabRef), `timestamp` or `dateadded`, first valid
wins; values with an offset are shifted to UTC, and those without one (JabRef's
local time) are read as UTC, so cross-source order can be off by the offset. Recently added uses validated source-added dates, never publication years
or invented import times. These display dates do not change cite fingerprints.

The bounded file is written whole, flushed and swapped in atomically. A file
Ibis cannot read means suggested/default outputs and automatic paste is
unavailable: a lost Copy only rule must not become paste permission.
Subsequent writes preserve a recovery flag in `apps.json`, including
after restart. A format save never clears it. An explicit Paste or Copy only
choice resolves recovery for that app alone; every other app keeps the refusal
until its own delivery choice, with no global reset. The warning shows only for
an app that can answer, so it never sticks on a refused app or with no app. Its read failure
is frozen in the copy proof, so a later successful save cannot release that copy.
Unknown apps ask for delivery approval
separately from format, in the same chooser (0009); recognised apps also offer
Copy only there and in More for the current ordinary app. A failed save leaves the previous durable choices intact and says
so. Preference writes from different windows apply only deliberate field
patches under the native owner lock, returning the authoritative saved state.
Settings and the picker share one frontend patch queue for ordering, field
acknowledgments, stale reads and recovery. Settings restores the saved choice
after a failed save; the picker retains explicit choices for retry. Each window
keeps its own queue; the native lock coordinates windows (2026-10-28).

Unlisted-app Paste approval compares the normalised OS app ID and a SHA-256
fingerprint of that ID, platform and exact OS-reported app location. Windows
uses the full executable path already read by `QueryFullProcessImageName`;
macOS uses the bundle URL path on the same captured `NSRunningApplication`
record. Only the fingerprint is retained and sent through IPC; raw app paths
stay transient in capture. Identity binding adds no document, executable-content,
signing or filesystem reads. Path case
is preserved; paths are never canonicalised through filesystem reads. A missing
or changed saved binding requires approval again; unavailable capture identity
only copies. A delayed save must match the captured
fingerprint too. Format and Copy only remain keyed by app ID. Capturing no
signing team means replacement at exactly the same location remains
indistinguishable; normal updates at that location keep the choice.

App-ID evidence for the 2026-10-31 destination-table additions:

- Read `CFBundleIdentifier` and `CFBundleName` from the installed bundles for
  Claude (`com.anthropic.claudefordesktop`), ChatGPT (`com.openai.codex`, whose
  bundle name is ChatGPT), Notes (`com.apple.Notes`), Notion (`notion.id`), Craft
  (`com.lukilabs.lukiapp`), OneNote (`com.microsoft.onenote.mac`), Thunderbird
  (`org.mozilla.thunderbird`), Logseq (`com.logseq.logseq`), Jump Desktop
  (`com.p5sys.jump.mac.viewer`) and Termius (`com.termius-dmg.mac`). These are
  app metadata only, with no document or library reads.
- ChatGPT Classic's `com.openai.chat` is recorded in an
  [original application crash report](https://community.openai.com/t/chatgpt-app-for-macos-crashing-on-open-november-3/1002242).
  Parsec's `tv.parsec.www` is recorded in a
  [firsthand native-paste reproduction](https://github.com/Muesli-HQ/muesli/issues/493);
  its Windows `parsecd.exe` is in [Parsec's launch instructions](https://support.parsec.app/hc/en-us/articles/32381836182932-Run-Parsec-App-From-Command-Line).
  Both Jump Desktop Mac variants are named in
  [the vendor's configuration instructions](https://support.jumpdesktop.com/hc/en-us/articles/4408192384909-Mac-How-can-I-prevent-users-on-Jump-Desktop-for-Mac-from-saving-or-sending-computer-passwords).
- NoMachine's `com.nomachine.nxdock` comes from its
  [extracted 7.4.1 Info.plist](https://develop.doesitarm.com/app/nomachine/);
  Windows `nxplayer.exe` and `nxplayer.bin` appear in a
  [firsthand installed-file listing](https://forum.nomachine.com/archive/topic/trying-to-make-a-portable-version-of-nomachine-enterprise-client).
  Horizon's `com.omnissa.horizon.client.mac` is in its
  [recorded bundle ID and signing requirement](https://appcatalog.cloud/apps/omnissa-horizon-client);
  `vmware-view.exe` is in the [vendor's Windows client guide](https://docs-be.omnissa.com/bundle/Horizon-Client-for-Windows_2006_Horizon-Client-for-Windows-Installation-and-Setup-Guide/raw/resource/enus/Horizon_Client_for_Windows_2006_Horizon_Client_for_Windows_Installation_and_Setup_Guide.pdf?save_local=true).
- Windows Emacs and gVim use their real executable names from
  [Emacs's build source](https://github.com/emacs-mirror/emacs/blob/master/src/Makefile.in)
  and [Vim's build source](https://github.com/vim/vim/blob/master/src/Make_mvc.mak).
  Their default shortcuts differ: [Emacs clipboard commands](https://www.gnu.org/software/emacs/manual/html_node/emacs/Clipboard.html)
  and [Vim's Windows clipboard instructions](https://github.com/vim/vim/blob/master/runtime/doc/gui_w32.txt).
  Neovide's [bundle metadata](https://github.com/neovide/neovide/blob/main/Cargo.toml)
  and [Windows release workflow](https://github.com/neovide/neovide/blob/main/.github/workflows/nightly.yml)
  verify its IDs; its [FAQ](https://neovide.dev/faq.html) requires explicit
  paste mappings on macOS and Windows.

Not verified: legacy VMware Horizon Mac bundle IDs, separate NoMachine viewer
helper bundle IDs and Jump Desktop's Windows executable. No guessed entries
were added for these. The added table rows and per-app choice lookups are bounded;
no new file, network access, polling or hidden activity is introduced.

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

2026-11-01: Ibis uses its own outline icon set now, sourced in `brand/icons/`
and generated by `brand/build.py`. Later each OS uses its system icons, with
this set as the fallback. On macOS Ibis shows the installed Zotero.app icon,
resolved by bundle id `org.zotero.zotero` through NSWorkspace. Ibis shows the
icon the person already has and ships none of Zotero's artwork. Missing apps,
failed reads, the canvas, Windows and Linux use Ibis's books glyph.

## 0024 Window permissions

Every Ibis command is declared in Tauri's app manifest and granted per window;
remote origins get none. A test checks the grants through Tauri's IPC
dispatcher.

## 0030 Links and launchers

`ibis://` links open the picker with a query, output or library filled in.
A link is untrusted input: it never copies, pastes, changes settings or reads
files; only the person's Enter acts. Launchers (Spotlight through Shortcuts,
Alfred, Stream Deck, notes apps) reach Ibis through these links and the `ibis`
command, with recipes in the docs. A Raycast extension and other launcher
extensions follow in 0.7, built on the same links and command (maintainer,
2026-11-01, replacing the 2026-10-28 deferral).

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
The reader inside Ibis no longer waits for citing from Zotero's annotations
(maintainer, 2026-11-04, replacing the 2026-10-28 order; see 0036).

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

2026-11-01 (maintainer): every planned feature ships before 1.0, including ideas once
listed after it; the version numbers before 1.0 are working steps and may be
renumbered. 1.0 adds no features: people test everything and the maintainer
moves to 1.0 by hand. Until signing is decided, macOS keeps the stable
self-signed identity and Windows installers stay unsigned with first-launch
steps ([ROADMAP](../ROADMAP.md)).

2026-11-02 (maintainer): every planned milestone through 0.7 stays in 1.0. The
PDF opens in a panel inside the picker that widens, shared with the citation
editor. Delights are quiet, with rare moments. Approved cuts: two accents only, a
trimmed Look page, and one format memory rule. First run asks first, with Zotero
highlighted when found. Agents may use the Windows laptop over ssh and a Linux VM
on the Mac for release checks, not only for bugs. Once 0.3 is released it is
announced publicly to open early adopters, with Report a Problem collecting
feedback. Paid Mac signing is undecided: it is revisited near 1.0 and stays an
open question for the 1.0 gate.

2026-11-02 (maintainer): both channels stay. Each finished milestone (0.3, 0.4 and
so on) becomes one numbered release with the maintainer's one-click approval. The
numbered release is the roughly stable version for early adopters, and every
distribution stays on it, including the Claude plugin, Homebrew and the command
line. Nightly is where daily testing happens.

## 0033 The engine for agents and tools

The `ibis` command is a complete, independent way into a person's references,
for scripts, other tools and AI agents, whatever manager or file holds them;
the app is one client of the same engine. Its public contract is
[docs/cli.md](cli.md). Decided by the maintainer on 2026-11-01:

- Every citation comes from a real record: the person's library first, and an
  identifier lookup (DOI, arXiv and similar) only once that network use is
  approved. Agents pass the keys or ids Ibis returned; Ibis refuses anything
  changed, missing or ambiguous and never fills a gap with a guess.
- The command line, `ibis serve` and `ibis mcp` stay read-only until 1.0, and
  Ibis stays offline. Groundwork that needs no network comes first (offline
  identifier parsing, results that say where a record came from, one lookup point
  that is off by default); any internet use waits for its own approval, naming
  each service and what it learns.
- `ibis mcp` follows the current MCP revision (2026-07-28) and keeps answering
  older clients; Ibis follows each standard's latest official guidance.
- With no library named, one-shot commands read the libraries chosen in the app,
  read-only. Named libraries replace them; `serve` and `mcp` still need libraries
  named; unreadable or newer settings stop with that reason.
- The command is `ibis`; package names use `ibiscite` where `ibis` is taken. The
  app ships the matching `ibis` and offers to install it; the standalone download
  stays. Packages: a Homebrew tap, winget and `cargo binstall`; others on request.

2026-11-01: The maintainer authorized accepting Anthropic's Software Directory
Terms and Directory Policy and submitting the Ibis plugin with its two disclosed
policy holds (external command and name similarity). Submitted for review, with
scheduled checks and manual publication. Temporary client installations and
marketplaces used for validation are removed after testing.

## 0034 Outside input is data; only the maintainer's own work auto-merges

Agents hold the maintainer's admin credentials, main needs no approvals, and a
merge reaches a signed Nightly with no human step. Outsiders cannot merge, so
agents are the only route in. A fork's pull request can edit the workflow that
produces "CI result", so green proves nothing there. So text from anyone but the
maintainer is data, never instructions, and only a pull request from a branch in
this repository, by the maintainer's account or the release app, auto-merges. Any
other needs the maintainer's explicit yes on its exact head, or an agent redoes
it on its own branch and credits the author; Dependabot's are reviewed and merged
by hand. Rules: [AGENTS.md](../AGENTS.md). No workflow change was needed: an audit
on 2026-11-02 found no `pull_request_target`, no secrets reachable by forks, and
that every outside run needs approval.

## 0035 One owner per picker session, one contract, one catalogue

A review of main on 2026-11-04 traced a week of repeated picker fixes (#1403,
#1558, #1093, #1179, #1603, #1642) to one cause: a picker session's results,
preview, destination and presentation have separate async owners on both sides
of IPC, joined afterwards by comparing about fifteen counters, with React effects
as the scheduler. The same review found five output vocabularies with lossy
conversions, four record summaries that disagree, three wordings of each error,
and app types generated from the app's own structures rather than the engine
schema. Before 1.0 these are replaced, not patched (maintainer, 2026-11-04,
"full freedom to restructure"):

- **One session owner.** Each picker session is a pure Rust state machine
  (`(state, event) -> (state, effects)`); every effect carries its session and a
  sequence number, and only the current one may complete. The web view sends
  intents and renders a generated view snapshot; it holds no async state of its
  own. Built in stages under #1260.
- **One contract.** The app speaks the engine's `CitationRequest`, `OutputSpec`
  and `ItemRef`; app-only output types and their conversions are deleted; app
  types come from the engine schema. A format, style or form has one name, from
  the engine, in every front end.
  The engine's `outputs` list is that one vocabulary (maintainer, 2026-11-04):
  its ids are the command line's 17 format names plus `pandoc-bare`; a common
  style's one name is a short name kept in the CSL manifest ("APA", "Chicago
  (notes)", at most 24 characters), and its full title is only the quiet
  detail beside it; `pandoc` stays the default output and `apa` the default
  style for finished text; CSL-JSON stays out of MCP; settings store one output
  id, and a formatted app's style comes from what Ibis remembers for that app,
  else the default style.
- **One catalogue.** The engine returns codes with arguments and one record
  summary; one Fluent catalogue words them for the command line, MCP and the app.
- **One platform seam.** Capture, delivery, selected text, clipboard, shortcut,
  login item and window presentation sit behind one trait with a fake, and the
  app shows what each system can do from a capability report, not from
  `cfg` branches spread through the code.
- **Tests prove outcomes.** Tests that parse source, workflow YAML or CSS with
  regular expressions, or that fix call order or English wording, are replaced by
  behaviour checks, generated race orderings with a replayable seed and golden
  data; time comes from an injected clock, never a sleep.

Lessons taken from Zed (fake platform, seeded executor), FFmpeg (libraries never
print, golden data reviewed as diffs), VLC 4 (one player object to end races),
Pandoc (one model between readers and writers) and Better BibTeX (editors pull
citations instead of synthetic paste).
## 0036 The reader's trust boundary

2026-11-04 (maintainer): Ibis gets a full-quality, page-level PDF reader so that
nobody needs to open Zotero while writing. It comes forward and no longer waits
for citing from annotations. It is a read-only viewer for PDFs already in the
person's citation manager (a Zotero attachment or a `.bib` entry's `file` field),
to look at a source and choose a page. It does not replace Zotero: Ibis never
imports, stores, manages, downloads, edits or annotates a PDF, and Open in Zotero
stays. It opens in the shared inspector panel beside the list, and the picker
widens only to hold the PDF (0027, 2026-11-02). It is one priority among many;
design polish, shortcuts and configuration matter as much.

**Engine: PDF.js** (`pdfjs-dist`, legacy build, pinned exactly, behind one
adapter module), compared on 2026-11-04 with a spike on the branch
`claude/reader-boundary` (#1252):

| | PDF.js in the web view | pdfium (`pdfium-render`) | Each system's own (PDFKit, Windows.Data.Pdf, poppler) |
| --- | --- | --- | --- |
| Untrusted input | Memory-safe JavaScript in a worker, inside the web view's content process (sandboxed on macOS and Windows; not yet on Linux, see below). CVE-2024-4367, code execution through PDF.js's old `eval` font path, was fixed in 4.2.67 | A C++ parser inside the Ibis process, beside the engine, the clipboard and paste permissions; Chrome runs it only in a sandboxed renderer | Three C/C++ parsers (CoreGraphics' JBIG2 decoder was the FORCEDENTRY exploit, CVE-2021-30860); a native view inside the window also bypasses the one key listener and focus rules |
| Same on every system | One code base. Upstream supports the legacy build on Chrome 125+ (WebView2) and "mostly" on Safari 18+, and does not test WebKit, so Ibis tests it | Same pixels | Three renderers that differ |
| Text and screen readers | A DOM text layer that can be selected, found and read (the spike) | Text and glyph boxes; Ibis would rebuild the DOM layer | PDFKit is accessible; Windows.Data.Pdf has no text at all |
| Page labels | `getPageLabels()`: null for absent or malformed labels, empty for unlabelled pages, never guessed (the spike) | `FPDF_GetPageLabel` | PDFKit yes, Windows.Data.Pdf no, poppler yes |
| Size | A lazy chunk of 564 KiB gzipped (147 main, 382 worker, two fonts); about 1.8 MB in the installer with cMaps, image decoders and Foxit fonts. The picker grows by 0.7 KiB | About 5 to 7 MB of native library per system and architecture (not measured here) | Nothing on macOS or Windows; poppler and GLib on Linux |
| Memory | Worker and canvases only while open; ended on hide | In the app process | Managed by the system |
| Licence | Apache-2.0; bundled parts BSD (Foxit fonts, OpenJPEG, JBIG2 from PDFium, Adobe cMaps), MIT (qcms), CC0 (an ICC profile) | Crate MIT or Apache-2.0; the binary bundles FreeType, libjpeg-turbo, OpenJPEG and LCMS, which `cargo deny` cannot see | poppler is GPL (from Xpdf) and needs its own licence decision |

PDF.js is the only option that keeps untrusted parsing out of the Ibis process,
draws the same pages on every system and gives screen readers real text.

- **Who decided.** The first paragraph is the maintainer's direction, as are the
  Linux hold and the answer on `'wasm-unsafe-eval'` below (2026-11-04: take the
  security review's recommended options). The engine and the other rules are
  engineering choices, changed by pull request with a review of the final head.
- **Bytes, not paths.** One command, granted only to the picker window (0024),
  takes an opaque attachment id the engine issued for the current library
  generation (#1208) and a load id, and returns the bytes as a raw IPC response
  (`tauri::ipc::Response`, an `ArrayBuffer`), which PDF.js transfers to its
  worker. The engine finds the file; no path, file URL, `asset:` URL or
  `convertFileSrc` reaches the web view, an error or a log. The read runs on the
  bounded file-I/O budget (`ibis-sources/src/file/io.rs`, `bounded`), keyed by the
  load id. `invoke` takes no `AbortSignal`, so a second command cancels a load id
  on the Rust side, and a cancelled or overtaken read sends nothing; an `open()`
  stuck in the kernel holds one budget slot, never the picker, and the load
  fails at 15 s. The file is opened first and the handle checked: a regular file
  (`fstat`), at most 256 MiB, placeholder flags rechecked on the handle just
  before reading (a dataless file on macOS, read with
  `IOPOL_MATERIALIZE_DATALESS_FILES_OFF` on the reading thread so it never
  downloads; recall-on-open or recall-on-data-access on Windows), then `%PDF-` in
  the first 1,024 bytes. Network file systems and Linux FUSE mounts, where cloud
  clients live, are refused as not on this computer, since a read there can block
  or download. If the window's IPC has fallen back to `postMessage`, which would
  carry the bytes as a JSON array of numbers, the command refuses. Bytes stay in
  memory only while the PDF is open: never written, cached or sent anywhere.
- **Ibis owns the worker.** Ibis creates a dedicated `Worker` from a same-origin
  script and always hands it to PDF.js as `port`: never `workerSrc`, whose failure
  silently falls back to parsing on the main thread, and never a `blob:` worker.
  Tauri 2.12.5 attaches the CSP only to `.html` assets, so Ibis sets the app's CSP
  on the worker script's response itself (`on_web_resource_request` or a header
  hook), and a test on each web view shows `fetch()` and `WebAssembly.compile`
  refused inside the worker. Closing the reader or hiding the picker destroys the
  task and then calls `terminate()`, because PDF.js terminates only workers it
  made; a test shows no live worker after hiding.
- **A viewer of bytes.** `getDocument({ data })` only: no URL, range requests,
  streaming or auto-fetch. No scripting (neither `pdf.sandbox` nor the QuickJS
  wasm ships), no XFA and no annotation layer, so links, forms, JavaScript,
  launch, submit and embedded files do nothing; stored annotation appearances are
  drawn as pictures. Standard fonts and cMaps are bundled and served by an
  in-memory factory. `useWasm: false` is set explicitly; the two JavaScript image
  decoders (`openjpeg_nowasm_fallback.js`, `jbig2_nowasm_fallback.js`) ship as
  same-origin static files with `wasmUrl` pointing at them, because PDF.js loads
  them with `import()`. Nothing else is fetched. The CSP gains neither
  `unsafe-eval` nor `'wasm-unsafe-eval'` (maintainer: no), which would enable the
  wasm decoders and compiling PDF functions to wasm. PDF.js 6.4.299 has no
  reachable `eval` or `new Function`; the legacy build's core-js keeps an
  unreachable `Function('return this')` fallback. Hardening, kept if measurement
  allows: `disableFontFace: true` (glyphs drawn as paths) and
  `isImageDecoderSupported: false`.
- **What remains exposed.** Embedded fonts (unless drawn as paths) and JPEG
  images still reach the web view's native decoders. A compromised content
  process holds every grant the picker window has. A Flate bomb or a huge page
  can exhaust the content process and crash the picker. On Linux wry 0.57 never
  enables WebKitGTK's sandbox and the AppImage bundles its own WebKitGTK, so
  **the reader stays off on Linux until Ibis enables the WebKitGTK sandbox before
  the first web view** (maintainer). After a web-content crash with the reader
  open, Ibis does not reopen that file, by content hash, for the rest of the
  session. To evaluate: the reader in a child web view whose only grant is the
  bytes command.
- **Limits.** At most 256 MiB and 10,000 pages a file; opening, bytes to first
  page drawn, takes at most 15 s, then fails with Try Again and Open in Zotero. Images up to
  100 megapixels (`maxImageSize`) within a per-page budget of decoded image bytes;
  a page that loses an image says "Part of this page is too large to draw" and is
  never shown blank as if that were its content. Ibis caps each page canvas at 16
  megapixels and enlarges by CSS beyond it (`canvasMaxAreaInBytes` only governs
  downscaling inside the worker). The page list is virtualised with a minimum
  laid-out page height; only pages in view and one either side are drawn, and
  renders that leave the view are cancelled and their canvases and text layers
  removed.
- **One owner per load.** Each open belongs to the inspector's reader session and
  has an immutable identity: load id, attachment id and library generation.
  Opening another source, closing the reader or the panel, hiding the picker, a
  reload that changes the attachment, or a newer open cancels the read by load
  id and destroys the PDF.js task. A completion that is no longer current is
  destroyed and reports nothing; a page render carries its load id and page and
  is cancelled, never drawn, when stale. The Page field the reader fills belongs
  to the citation item that opened it; if that item has left the shelf, nothing
  is filled.
- **Memory.** The reader is a dynamic import loaded on first open;
  `scripts/picker-budget.mjs` counts only the picker's static graph, so startup
  stays at today's 222 KiB of 400 KiB (the spike measured +0.7 KiB for the import
  itself). Hiding the picker or closing the reader ends the document and the
  worker; only the parsed module code stays. A release check opens an invented
  300-page PDF, hides the picker and requires the hidden 200 MiB limit within 2 s.
- **Page numbers.** The physical index, the printed label and an annotation's
  label stay apart (architecture, **Reader**). A printed page comes from a
  non-empty `/PageLabels` entry, a Zotero annotation's label or an offset the
  person confirms; otherwise it is unknown, never the physical index. A label is
  citable only if, after NFC, it has at most 16 characters of digits, letters
  (Roman numerals included), `-` and `.`, and no control or format characters
  (Cc, Cf); any other label is shown but counts as unknown. Annotation labels pass
  the same test, and one that disagrees with `/PageLabels` makes the page
  ambiguous. An ambiguous page, or a label on several pages, asks. When the record
  has a page range and the label falls outside it, Page waits for the person to
  confirm and the indicator shows both. pypdf, a widely used parser, fills in
  "1, 2, 3" for a file with no labels and "3, 4" for pages a label range leaves
  blank: the silent guess Ibis refuses.
- **Notes HTML never reaches the DOM as HTML.** The engine parses Zotero note
  HTML with html5ever (already in `Cargo.lock` through Tauri) into a small typed
  tree: paragraphs, headings, lists, quotations, emphasis, strong, sub- and
  superscript, code and text, at most 1 MiB and 32 levels deep. Links keep their
  words and lose their target; images, styles, scripts, frames, forms and every
  URL are dropped. React renders the tree as elements and text nodes, never
  `innerHTML`. Annotation comments and PDF text are plain text (PDF.js's text
  layer sets `textContent`).
- **Still to prove.** The spike ran in Chromium 153, WebView2's engine. Before
  the reader ships: the same scenario in WKWebView on each supported macOS and
  WebView2 on Windows; the worker CSP test and the no-live-worker test on each web
  view; the hardening options measured. The legacy build needs Safari 18's engine
  or later, so the reader checks the web view's engine at run time and is
  otherwise unavailable with that reason; macOS 12 (last Safari 17.6) fails it.
- **Licences.** `cargo deny` does not see JavaScript; PDF.js and each bundled part
  are listed in `NOTICE.md` by hand and pinned in `pnpm-lock.yaml`. PDF.js also
  ships Liberation Sans (GPL-2.0 with a font exception, under Red Hat's terms) as
  its Helvetica substitute; Ibis leaves it out until the maintainer decides, and
  PDFs that do not embed Helvetica use the system's sans-serif.

## 0037 One source per fact; everything else is generated, checked or a command

Maintainer, 2026-11-04: Ibis is built mostly by agents for one maintainer, so the
cost that matters is what each change must read, retype and remember. A fact
lives in one place, in a form a program can read; everything that repeats it is
generated from it or checked against it; and every routine step is a command.
Before 1.0, anything that fights this is restructured, with no compatibility kept.

- **Contract outward.** The engine schema generates the app's types, the CLI
  reference, MCP's tool schemas and the golden cases (0035, #1260, #1687).
- **Specs as data.** A table in a design or architecture document that code must
  match (notices, keys, footer states, status words, budgets) becomes a small data
  file the code imports and the document renders or a test compares, as the
  Notices table already is. People change one row, not prose.
- **One catalogue** of words for the command line, MCP and the app (#1675).
- **One seam per platform concern** with a capability report (#1676), so each
  system is tuned in one place.
- **Pictures are the design baseline.** The canvas renders every state; a pull
  request that changes how something looks shows the pictures that changed, and
  judges look only at those.
- **Budgets as data:** one table of limits, each with its check; deterministic
  counts where possible, timings only reported on shared runners.
- **Routine steps as commands:** `scripts/cloud-setup.sh` sets up a session and
  `scripts/land.sh` lands a branch (merge main, check what changed, push, auto-merge).
- **Small documents:** a session reads the one file its task needs (#1690).

## 0027 Process until 1.0 (with 0011, 0015 and 0017)

Agents have standing engineering and merge authority over the maintainer's own
work and work by [AGENTS.md](../AGENTS.md): no compatibility code, required "CI result" before
every merge. Stable releases wait for the maintainer's one-click approval. A
plan or answer the maintainer approves is recorded the same day in these files,
the issues and the tracking issue; nothing lives only in a chat, a tool's memory
or a local file (maintainer, 2026-10-28). The website demo is critical and stays
functionally aligned with the released app (2026-10-29).

2026-11-02 (maintainer): the path from a finished change to a nightly stays
efficient, quick and smart, so changes ship directly and are tested in batches on
the nightly. So pull requests merge themselves on a green "CI result"; related
changes go together in one pull request; the limit on active pull requests is
gone; and a separate review of the final head is needed only for the updater,
release signing and publishing, CI permissions and security boundaries. Pull
requests run only what their paths can affect, main runs everything, and a
nightly never waits for CI or the release checks. A numbered release does.

The testing rhythm (maintainer, 2026-11-02): agents merge to main on green, a
testing session checks the real app on the Mac, and the maintainer tries one
nightly a day with a short list of what changed.

2026-11-02: One reference's details and, later, a source's PDF share one panel
inside the picker, beside the list as in Preview's sidebar; never an anchored
popover. The picker widens only to hold the PDF. Rules: design.md **Inspector**
[C19] (maintainer).
