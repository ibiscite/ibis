# 0029 Native calls

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
"Copied. Ibis couldn’t confirm where the text would go. Press ⌘V to paste."
Neither outcome claims password-field detection. The AX password/subrole guard
remains removed (#1280, #1360) as a reason to paste or not: after a paste is
already refused as `paste-unavailable` or `secure-input`, Ibis reads only the
`AXSubrole` of the system's focused control, when the target process owns it,
and says `secure-field` ("This field hides what you type") for
`AXSecureTextField`. It changes the words, never the outcome, and reads no
range, text or value (2026-11-07, a web password field copied with no reason). macOS auto-paste
requires two separate authorizations: PostEvent to send the shortcut and
Accessibility to prove its destination. The identity-only proof below adds an
Accessibility requirement compared with the old PostEvent-only contract.
Both checks are passive; they never request trust or show a permission prompt.
Without Accessibility, Enter copies with `accessibility-required`:
"Copied. Ibis needs Accessibility access to paste safely. Press ⌘V to paste."
Provider errors, unsupported attributes and timeouts remain `paste-unavailable`.
A timeout says nothing about the target, so while it returns, and at the final
check, every check runs again, fresh, for the rest of the 1 s return time
(2026-11-07: TextEdit taking focus back or handling a keystroke missed one read
and the citation was only copied). A final check that timed out never posts.
The final identity read also refuses a focused control other than the one
whose selection the final check just proved, so a page that moves focus
between the two cannot have a selection nobody checked replaced (0.3.0
pre-release review); that refusal is unproved and is asked again the same way.

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
