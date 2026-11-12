# Copying, pasting and notices

## Copying and pasting

- Cancelled work is not a failure and never reaches the person: a preview stopped by a newer choice or a reload says nothing. A preview that fails belongs to the opening that saw it: reopening or choosing the citation again prepares it afresh, and the failure is gone from the first frame.
- Enter does what the footer says. Pressed while results or the preview load, it waits and runs once. Repeated activations on the same citation never copy twice; a changed query, reference, page or format, arrows, Esc or hiding cancel a waiting Enter. A stale reply never replaces newer shelf or feedback.
- A copy waiting for formatting belongs to its picker presentation and clipboard. Hiding or reopening the picker, or a newer clipboard write, cancels it before it can write or create a delivery.
- **One format rule** (decision 0042): a writing explicitly selected for this app wins, then the app's remembered format, then your format for this kind of app, then its suggestion. Documents and key-based apps have separate usual formats. The engine returns the format and its source; the interface shows that answer.
- **The format control says why:** “MLA · for Word” or “Chicago notes · Thesis”. Its palette offers “Use APA in Word again” or “Stop using Thesis in Word”. A person who never changes anything sees only the suggested format and “New writing…”. First run asks nothing; the first successful suggested paste per kind says “Pasted in APA · Change” once.
- **Writings** are optional plain objects in `settings.json`: a name, format and optional read-only `.bib`. New writing starts from the current format. Selecting, editing or removing one saves before changing the active context. Failed saves preserve the old writing, format and bibliography. Switching keeps the search and references, retires earlier Enter and delivery work, and prepares the citation afresh. Explicit editor or CLI context may select one; window titles, URLs, document names and nearby files never do.
- The format palette previews chosen references for key formats and a labelled example for styles. Its footer says whose choice changes. The writings sheet uses the shared frame and rows, with a check on the selected writing and actions for its bibliography, name and removal.
- Refused apps and unproved targets only copy, with the reason. Returning from a permission detour keeps the citation and requires a fresh Enter.
- **Selected text as the search** (macOS): short text on one line, selected in the target app, opens as the search, all selected, so typing replaces it; Enter pastes over it. It lasts one opening: it never returns as a recent search unless edited. Long or multi-line selections, secure typing, password fields, copy-only, refused or unapproved apps and failed reads open an empty search and change nothing. **Ibis replaces only text the person saw**: with a long selection the footer offers Copy, and any selection that changed, or was never read, stays as it is, with the reason. Enter still pastes at the caret when nothing is selected.
- **Change Last Citation** (More, ⌘E): reopens the last citation for this app on the shelf, with its pages and options; the footer says "Change Citation". Enter replaces text only when the old citation is selected in the document, proved just before the paste; otherwise it copies the changed citation and says so. After such a copy the person may paste it by hand or not, so the next change replaces whichever of the two is selected, and nothing else. Ibis never deletes text it cannot see. The last citation lives in memory only, for this run of Ibis and at most eight apps. Forget Recent Citations clears it, and the shortcut that opens Ibis never reopens it: hiding Ibis abandons the change. A failed Forget stays visible until cleanup succeeds, including after a failed retry.
- **With Zotero in front** (maintainer, 2026-11-05): the same picker opens, and Enter opens the PDF or shows the item in Zotero instead of pasting; the footer says which.
- A successful copy hides Ibis, returns focus to the previous window and shows the notice. Pinned, Ibis stays.

## Notices

- "Pasted" or "Copied" shows after the picker hides, in a window that never takes focus: one card near the top centre on every system, ignoring the pointer. What was copied is the hero: the citation in the text colour at body size, monospaced. Above it a quiet secondary line says what happened ("Copied", "Pasted into Word" or the reason). A green check in a soft green circle, centred on both lines, confirms success. A long reason wraps and is never clipped; the citation stays one line, ending in "…" with "+4 more" in view. The notice follows the chosen appearance.
- **Plugin note**: the first paste into a word processor (Word, LibreOffice, Pages) adds a line under the citation and stays nine seconds: if this document uses a reference manager's plugin, insert with the plugin. Once per app, remembered in the per-app file, reset by Clear. It depends only on the app ID; Ibis cannot see the document, and browsers are skipped because Google Docs looks like any page.
- No notice carries a side bar or a one-sided border. Errors are one sentence beside their cause, never a modal.
- **One notice slot per window** (`ui/NoticeSlot`): notices never float. Everything that passes takes the window's status area: in the picker the footer's left side, standing in for the library status, which returns after 8 s; in Settings one line at the foot of the content area. The slot is in the layout, so it never covers content, never moves anything and opens no layer: it can sit behind any open menu or panel. One notice shows: the newest Undo offer, which is also what ⌘Z acts on, then a remark, then a running update. A notice's time runs only while it shows, so one that waits returns with the rest of its time and none passes unseen, including a failed update.
- A notice's words start exactly where the status words start, from the shared layout tokens (`--title-start` in the picker, `--heading-offset` in Settings), whatever the dot or the button around it.
- A notice is one line ending in an ellipsis, with short names ("Removed Vale 2021", first author and year as on the tile). An Undo offer has a quiet **Undo ⌘Z** right beside its words; it lasts 8 s, pauses on hover and focus, and ⌘Z does the same while it shows. A remark needing nothing (tip, milestone, the size after ⌘+, why Enter did nothing) is words alone, and leaves when you type. Screen readers hear both through the status role.

Every "copied, not pasted" sentence says why, then ends with its fix, with full stops; the fix is usually "Press ⌘V to paste." (Ctrl+V on Windows and Linux). Every copy has a reason: none is a bare "Copied." These are the exact words; a test compares them with the catalogue's `copied-notice-reason`.

| Reason | Code | Words |
| --- | --- | --- |
| Pasting off | `paste-off` | Copied. Pasting is off in Settings. Press ⌘V to paste. |
| Paste check unavailable or too slow | `paste-unavailable` | Copied. Ibis couldn’t confirm where the text would go. Press ⌘V to paste. |
| No paste on this system | `unsupported-system` | Copied. Ibis can’t paste on this system yet. Press ⌘V to paste. |
| No app to paste into | `no-target` | Copied. There was no app to paste into. Press ⌘V to paste. |
| Unapproved app | `unknown-app` | Copied. Automatic paste is not approved for this app. Press ⌘V to paste. |
| Copy Only chosen | `copy-only` | Copied. Copy Only is set for this app. Press ⌘V to paste. |
| App with another paste key | `nonstandard-shortcut` | Copied. This app’s paste key does something else. Use its own paste command. |
| Target changed | `target-changed` | Copied. The target app or window changed. Press ⌘V to paste. |
| Clipboard replaced | `clipboard-changed` | Copied. The clipboard changed since. Copy the citation again. |
| Expired | `paste-expired` | Copied. The paste expired. Press ⌘V to paste. |
| Paste permission missing | `no-permission` | Copied. Ibis needs your OK to paste. Press ⌘V to paste. |
| Accessibility missing | `accessibility-required` | Copied. Ibis needs Accessibility access to paste safely. Press ⌘V to paste. |
| Terminal | `terminal` | Copied. Not pasted into a terminal. Press ⌘V to paste. |
| Remote desktop | `remote-desktop` | Copied. Not pasted into a remote desktop. Press ⌘V to paste. |
| Virtual machine | `virtual-machine` | Copied. Not pasted into a virtual machine. Press ⌘V to paste. |
| A field that hides typing | `secure-field` | Copied. This field hides what you type. Press ⌘V to paste. |
| Secure Event Input on | `secure-input` | Copied. Secure typing is on, possibly in another app. Press ⌘V to paste. |
| Administrator app (Windows) | `elevated-app` | Copied. Not pasted into an administrator app. Press ⌘V to paste. |
| Windows keys stuck | `key-release-failed` | Copied. Ibis couldn’t release the keys it pressed. Press and release Ctrl and V to paste. |
| Selected text never shown | `selection-not-shown` | Copied. Ibis replaces only text it showed you. Press ⌘V to replace it. |
| Old citation not selected | `citation-not-selected` | Copied. Ibis replaces a citation only when it is selected. Select it, then press ⌘V. |

Never call the destination a password field. A field that hides typing is named only when its subrole says so (a web password field included); secure typing alone says "possibly in another app". The right paste key is shown per system.
