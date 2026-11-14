# 0009 Insertion

Every delivery ends visibly: pasted, or copied with a reason. Ibis captures the
target, writes every clipboard format, restores focus and sends the platform's
paste shortcut only while its one-use proof remains valid. The citation stays
on the clipboard. Native proof, timing and permission rules are in
[0029](0029-native-calls.md).

Pasting is on by default; Settings › General › Pasting turns it off. Terminals,
remote desktops, VMs, elevated apps, secure input and uncertain targets only
copy. Recognized editors whose paste key means something else also only copy.
An unknown app with a proven target asks for Paste or Copy only in the format
chooser. Either choice requires a successful save and a fresh Enter. A format
choice alone grants no paste permission. App identity and storage are defined
in [0031](0031-what-ibis-remembers-per-app.md).

Saving a delivery choice revokes pending paste proofs. A delayed successful
clipboard write can still report Copied for the current citation, but cannot
hide the picker or forward a revoked proof. Repeated Enter joins the current
copy operation without writing or pasting twice. A changed citation, output or
picker opening owns its own result and feedback. Failed status reads cancel
queued Enter; the next attempt needs a fresh activation.

The footer names the intended action, app and format using native delivery
policy. Enter waits for the current search, preview and library revision.
Adding or removing a reference cancels queued Enter. A prepared citation stays
usable through an empty search. More retains the newest successful delivery
outcome in memory; failed or unfinished attempts cannot erase it. The native
receipt is nonactivating and belongs to one request and clipboard write, so a
late refusal cannot replace a newer message.

Selected text can become the search on macOS. The read is separate from paste
authorization, bounded, never stored or logged, and disabled with pasting.
**Ibis replaces only text the person saw in search.** Long, multiline or blank
selections are not shown and only copy. A changed selection also only copies.
If opening could not read it, final delivery requires no selection. Controls
that do not report selection are an explicit qualification limit: Ibis cannot
prove what text a paste there replaces. Windows currently does not read the
selection.
