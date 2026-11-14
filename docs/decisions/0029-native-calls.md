# 0029 Native calls

Use maintained safe bindings: objc2 for macOS, winsafe for Windows window/input
calls and uiautomation for password checks. The workspace forbids unsafe code.
A boundary with no safe binding gets one scoped module allowance, a crate-level
`deny`, safety comments on each block and independent review of its final head.

## Scoped boundaries

- The Windows closed-Zotero reader uses Win32 process, drive, attribute and ACL
  calls. It opens only its disposable database copy.
- `paste/macos/secure_input.rs` reads Carbon's `IsSecureEventInputEnabled` on
  the main thread. It is a system-wide flag, not proof of a password field.
- `paste/macos/focus/native.rs` owns public Accessibility calls and retained
  object identities. `paste/macos/safety.rs` owns SDK clocks and notifications.
- `paste/permission/drag.rs` implements the AppKit drag source and image setter.
  A live Settings drag publishes only the bundled app's file URL, accepts no
  path and permits only copy. The stateless source stays on the main thread.
- `placement/macos.rs` reads only `AXFocusedWindow`, `AXPosition` and `AXSize`
  with an existing grant and one 40 ms deadline. It also owns the retained
  display UUID used for local position memory. Missing evidence uses the
  pointer. Safe-area insets are read only when supported.
- `sizing/display_events.rs` registers AppKit notifications and Win32 subclasses.
  It ignores payloads, forwards Windows messages and removes subclasses at
  destruction. GDK monitor signals use safe bindings. Notifications coalesce
  into one app-queue geometry read; failed registration waits for the next show,
  never polls. Pending reads are consumed before looking up a Tauri window,
  because cloning Tao's proxy wakes the run loop. Dispatch starts on the
  blocking pool so `run_on_main_thread` enqueues rather than running inline.
- `macos_notifications.rs` registers process-lifetime layout and appearance
  notifications, ignores payloads and requests fresh native-thread reads.
- Test-only `paste/windows_bstr_test.rs` reads a VARIANT's checked VT_BSTR member
  while its immutable owner lives. SDK byte counts and bounded UTF-16 length
  must agree; malformed UTF-16 and odd byte counts refuse. No borrowed reference
  escapes or production allowance follows from this fixture.

## macOS paste proof

PostEvent and Accessibility are separate, passive requirements. Missing
Accessibility reports `accessibility-required`; secure input reports
`secure-input`; unavailable evidence reports `paste-unavailable`. Explicit
permission-guide actions alone may request or repair a missing grant. Working
grants and development-build grants are never reset. Granting permission
requires a fresh summon and Enter; it cannot revive delivery.

Focus proof reads a closed set: `AXFocusedApplication`, `AXFocusedWindow`,
`AXFocusedUIElement`, `AXWindow` and `AXTopLevelUIElement`. Process ownership,
document and input surface must agree with capture. Retained objects remain
on the main thread; workers carry a nonwrapping token. Final queries compare
and never recapture. No title, role, value, description, attribute enumeration
or tree traversal is allowed for this proof.

Selected-text search separately permits `AXSubrole`, `AXSelectedTextRange` and
`AXSelectedText`, only for an eligible approved app with secure input off.
An unproved or secure subrole stops before text reads. Bounds apply before text
crosses IPC: 30 ms at opening and 60 ms before paste, including sleep. The
picker receives text once; native code retains only a digest with redacted
Debug output. After an already refused paste, an owned focused control's
`AXSecureTextField` subrole may refine the reason to `secure-field`, never
authorize paste.

Eligible Electron/Chromium apps missing focused-window or top-level evidence
with -25205/-25212 may receive `AXManualAccessibility=true` once per retained
live app instance per run. The process must still be that original instance
immediately before writing. No refused, unapproved, Copy only or paste-off app
is touched. A successful write restarts full capture within the same budget;
delivery never performs this write. Support stays enabled for other assistive
apps. This is the only foreign attribute write; `AXEnhancedUserInterface` is
never set.

One query gets 50 ms including sleep; final adjacent relation/focus reads share
15 ms. Each object receives the remaining messaging timeout, restored on exit.
After hiding, readiness waits at most 1 s in 25 ms intervals for the original
process, document and surface. Final posting has at most 250 ms and the write
proof at most 1,500 ms. Unproved focus can be checked again within readiness;
changed identity refuses. No idle polling runs beyond pending delivery.

The proof binds target, picker opening, write token, pasteboard change count
and active/sleep-inclusive times. Every format must be published without a
foreign write; no clipboard content is read back. Sleep, wake, lock and session
changes retire a lifecycle epoch, including synchronous notification fences.
Clock brackets must be shorter than 500 µs and cannot widen retained evidence.
Disjoint offsets expire; invalid, backwards or wider samples refuse.

All locks and notification checks precede final focus reads. The final tail
checks key-down and all mouse-button-down times since delivery began, then
local clock/epoch/deadline evidence and posts without a mutex or UI dispatch.
Intervening input reports `target-changed`; invalid timing refuses. It records
no key, click target, coordinates or event contents. The Enter that began
delivery is outside that interval.

`IBIS_MAC_PASTE_DIAGNOSTICS=1` emits one fixed-schema JSON line after an attempt:
stage, AX error, durations, clock widths and equality/ownership booleans.
Absent evidence is null. No text, paths, app IDs or native objects are logged.

## Windows paste proof

Request entry freezes target HWND/process, picker opening and active/sleep-
inclusive timestamps before clipboard I/O. Successful publication adds only
the final clipboard sequence and unique write token. A foreign write, including
one retaining the owner HWND, refuses. No clipboard contents are read back.
Delivery expires 1,500 ms from entry; expiry is checked before foreground
restoration and after password checks, injection locks and key-release waits.
Invalid, hidden or minimized windows refuse.

UI Automation reads only the focused element's process and `IsPassword`, with
a 250 ms wait. Unavailable checks only copy. A timed-out worker retains its
permit until exit; at most four processes have outstanding queries, with no
queue. Display-name reads have a separate 50 ms worker and executable-name
fallback. Final injection resolves Ibis's HWND before destination/key locks
and never waits for the UI thread under them. Partial `SendInput` releases
only accepted keys; failed releases remain for recovery before another paste.

## Shared ownership and qualification limits

Every successful write receives a unique nonwrapping token. Only that token
consumes its one-use proof; late notices cannot steal newer proofs. Cancellation
and posting share ownership, so a completed paste cannot be reported as copied.
A picker reload begins with unknown presentation identity: Enter waits for a
native read; failures need another explicit action. Synchronous IPC captures
hide ownership before async work. Native hide completes before its reply.

macOS posts to a PID, Windows uses targetless `SendInput`: neither atomically
binds a shortcut to a foreign window. Focus reads and posting are sequential.
Programmatic changes within one surface, complete session cycles inside the
final tail, Windows HWND/PID reuse and focused-field changes remain native
qualification limits. Windows has no short lock/session epoch. Synthetic tests
do not establish real provider timing or keyboard placement. Linux only copies.
