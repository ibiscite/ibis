# Compatibility and edge cases

Citation tools usually fail at the edges: an unusual editor, a keyboard layout, a sandbox, a malformed record. This document lists the cases Ibis is designed for and how each is handled. Every item should eventually have an automated test, a fixture or an entry in the compatibility matrix.

## Principles

1. **Two outcomes only.** An insertion either succeeds or the citation is left on the clipboard with a visible reason. There is no silent failure.
2. **Detect before acting.** Check the destination's capabilities (secure input, elevation, input method, session type) before attempting insertion.
3. **Behaviour as data.** Per-application differences are expressed in destination profiles, not code.
4. **Never guess data.** Missing or malformed metadata produces a warning, never invented values.

## Insertion

| Case | Platforms | Handling |
| --- | --- | --- |
| Paste timing races (old clipboard pasted, clipboard restored too early) | all | Per-profile delays; restore only if the clipboard still holds our content (change-count check). |
| Clipboard locked by another process | Windows | Bounded retries, then copy fallback with reason. |
| Rich formats: CF_HTML byte offsets, RTF fragments, HTML in Linux selections | all | Always write plain text plus HTML and RTF; wrap RTF fragments in a full document; compute CF_HTML offsets in bytes. |
| Clipboard history and cloud clipboard | macOS, Windows | Mark transient content (`org.nspasteboard.TransientType`, Windows history-exclusion formats). |
| Paste shortcut differs by keyboard layout (AZERTY, Dvorak, "Dvorak – QWERTY ⌘") | all | Resolve the key that produces "v" in the active layout. |
| Applications with a different paste shortcut (terminals: Ctrl+Shift+V, Shift+Insert) | Linux, Windows | Destination profile `paste_keys`. |
| Code editors and browser editors auto-close brackets (`\cite{`, `[@`) | all | Paste, never type, in editors; profile default `plain`. |
| Terminals and modal editors (Vim, Emacs) | all | Paste only (bracketed paste); warn for modal editors. |
| Input method composition active (Chinese, Japanese, Korean) | all | Detect and force paste. |
| Secure input (password fields, apps holding Secure Event Input) | macOS | Detect `IsSecureEventInputEnabled`; copy fallback naming the cause. |
| Elevated target window (administrator applications) | Windows | Compare integrity levels; copy fallback. |
| Focus cannot be restored (cooperative activation, foreground lock) | macOS, Windows | Non-activating panel on macOS; record and verify the target window on Windows; otherwise copy fallback. |
| Full-screen spaces, multiple monitors, virtual desktops | all | Show the picker on the active screen and space; exclusive full-screen apps receive copy fallback. |
| Remote desktop, Citrix, virtual machine windows | all | Profile: plain text, longer delays. |
| Canvas-based editors (Google Docs) | browsers | Clipboard paste only; no accessibility-API insertion. |
| Applications that transform pasted text (Slack mentions, Notion links, smart quotes) | all | Per-profile escaping and output flavour. |
| Wayland: no global focus information or input injection on some compositors | Linux | Capability tiers: GlobalShortcuts portal or `ibis pick` bound to a desktop shortcut; RemoteDesktop portal with persisted consent; wlroots virtual keyboard; otherwise copy only. |
| Flatpak or Snap sandbox | Linux | Native packages are primary; sandboxed builds run in copy-and-portal mode and say so. |
| Antivirus or device management restrictions on input simulation | Windows, macOS | Signed builds; documented permissions; `RegisterHotKey` rather than low-level hooks. |

## Activation

| Case | Handling |
| --- | --- |
| Shortcut already taken | Report the conflict and suggest alternatives. |
| AltGr equals Ctrl+Alt on many European layouts | Avoid Ctrl+Alt defaults. |
| Shortcut pressed while the picker is open | Toggle the picker; never open a second instance. |
| Permission required (macOS Accessibility, Wayland portals) | Guided onboarding with a self-test button. |

## Reference data

| Case | Handling |
| --- | --- |
| Corporate authors, particles ("van der"), suffixes ("Jr."), mononyms | Preserved through parsing; formatting delegated to the CSL processor. |
| CJK family-name-first names; Arabic and other right-to-left text | Never reorder names; bidi isolation in plain-text output. |
| LaTeX-encoded text (`{\"u}`, `{\'\i}`), protected case (`{NASA}`), nested braces | Decoded to Unicode for display and search; raw record preserved. |
| `@string` macros (including macros built from other macros), `crossref`/`xdata` inheritance across several levels, parenthesis-delimited entries | Supported. Macro definitions capture earlier definitions; later redefinitions affect only subsequent uses. Definitions must precede use: a forward reference produces a warning and contributes an empty value. Inherited fields retain their parent's definitions. Inheritance is resolved as a graph: cycles and missing targets are reported and ignored. |
| Title case in BibTeX versus sentence case in CSL | English titles are converted to sentence case; braced text, acronyms and initials keep their case and are marked `nocase`; `\emph{}` becomes italics. |
| "and others" in author lists; organizations in braces | Exported as CSL `others` (rendered "et al.") and as literal names. |
| Babel language names (`american`, `ngerman`) | Converted to BCP 47 tags (`en-US`, `de-DE`); unknown names are reported. |
| Dates in other scripts, invalid dates, ranges, approximate dates | Digits in any common script are read; impossible dates are reported with what was kept; ranges and `~` are preserved. |
| Duplicate keys, unclosed entries, undefined macros, missing keys | Warning with line number; other entries still load (fixture `damaged.bib`). |
| Duplicate keys across libraries | Identity is source plus key; conflicts are shown. |
| Missing date, date ranges, uncertain dates | No invented values; "n.d." from the locale. |
| Trashed Zotero items and children of trashed items | Excluded, matching Zotero's own behaviour. |
| Retracted items | Flag shown when the source provides it. |

## Output

| Case | Handling |
| --- | --- |
| Styles that depend on the whole document (numeric, ibid, disambiguation) | Rendered standalone and labelled provisional. |
| Note styles (footnotes cannot be created by pasting) | Offer note text or key-based output. |
| Separate locators for several references | Supported in Pandoc, BibLaTeX multicite and Typst; plain `\cite` uses one command per reference and says so. |
| Characters special to the target syntax (`%`, `&`, `_`, `@`, `[`) | Escaped per syntax. |
| Locale-specific terms ("p.", "S.", "et al.") | Follow the chosen locale, not the operating system. |

## Linux preview shortcut capability

The preview's shortcut backend requires a reachable X11 server with XKB. Ibis
checks that connection and protocol before registering the shortcut. If they
are unavailable, the picker explains that global shortcuts need X11 and can
still be opened from the application launcher.

A session label such as `XDG_SESSION_TYPE=wayland` does not decide this check:
a usable X11/XWayland connection remains usable for X11 shortcut registration.
GTK's window backend and the login session are separate choices. Registering
an X11 grab through XWayland does not provide a native Wayland global shortcut
or establish activation from native Wayland applications. This preview has no
GlobalShortcuts portal integration. The Wayland capability tiers above remain
design goals; a successful X11 check or Xvfb run does not verify them.

## Test matrix

`scripts/smoke-native.py` supplies one narrow automated cell on each hosted OS:
the runner's default keyboard layout, one display, synthetic BibTeX at a path
with spaces and non-Latin characters, Pandoc clipboard output, and an ordinary
prior editor window. Linux runs X11 in Xvfb/Openbox; it provides no Wayland
evidence. Windows uses UI Automation/SendInput; macOS requires granted System
Events Accessibility and Automation access; Linux uses AT-SPI/xdotool.
Only completed runs with a passed `smoke.json` establish those cells. Missing
permissions and unavailable trees fail explicitly. See `docs/testing.md` for
assertions, measurement limits and the remaining hand checks.

The tables above describe design goals as well as existing implementation.
They are not a claim that every insertion tier or destination profile exists in
the preview. In particular, the preview copies citations for users to paste;
the smoke suite checks clipboard text and focus return, not automatic insertion.
Hosted Windows 2025, macOS 15 and Ubuntu 24.04 results do not certify Windows
10/11 or Arm, macOS 13/Intel, Fedora, Wayland, cloud folders, network drives,
mixed-DPI displays, sleep/wake, or non-English keyboards. Those require separate
recorded evidence before marking their cells supported.

The detailed coverage ledger and its remaining verification cells are tracked in
[#330](https://github.com/ibiscite/ibis/issues/330#issuecomment-6282580538).
The final manual-only checklist lives in [#79](https://github.com/ibiscite/ibis/issues/79#issuecomment-6282579792).

## Picker shortcuts

The picker shortcut defaults to Control+Command+R on macOS and Ctrl+Shift+Y on Windows and Linux. Cmd+Shift+Space is not used on macOS because it opens the Siri typing field, which takes the keystrokes meant for the picker; Ctrl+Shift+Space is not used elsewhere because it types a non-breaking space in Word and LibreOffice Writer. Ctrl+Shift+Y takes only rarely used commands (the Google Docs dictionary, the Firefox Downloads window, the VS Code Debug Console). On Windows and X11 it follows the key labelled Y, so on German layouts it is not Redo. A shortcut is modifiers and one key joined by `+`; `CommandOrControl` means Cmd on macOS and Ctrl elsewhere. An invalid shortcut is reported and the default is used. On Windows, avoid Ctrl+Alt combinations: many keyboard layouts use them as AltGr to type characters.

On macOS the default refers to the physical US-keyboard R position; its label
is not derived from the active layout (#196). Keep that limitation visible until
native layout tests and a matching label implementation establish otherwise.

The full target matrix is operating system/session × keyboard layout ×
destination × output format. Future insertion cells should paste a synthetic
string with accents, CJK, italics and brackets, then read it back through
accessibility APIs, application scripting or the browser DOM. This expanded
matrix is not implemented by the current copy-only smoke suite. Record completed
cells and exact source/runner details in the compatibility issues; users can use
the "App compatibility" issue form.
