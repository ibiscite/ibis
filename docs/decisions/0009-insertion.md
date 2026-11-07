# 0009 Insertion

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
