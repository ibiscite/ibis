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
| `@string` macros (including macros built from other macros), `crossref`/`xdata` inheritance across several levels, parenthesis-delimited entries | Supported. Inheritance is resolved as a graph: cycles and missing targets are reported and ignored. |
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

## Test matrix

Operating system and session (Windows 11; macOS 15 and later; Linux GNOME, KDE and wlroots on Wayland, and X11) × keyboard layout (US, AZERTY, Dvorak, German, Japanese input method) × destination (word processors, Google Docs, Overleaf, Markdown editors, Notion, email clients, chat apps, terminals, code editors, spreadsheets, remote desktop) × output format. Automated cells insert a test string containing accents, CJK text, italics and brackets, then read it back through accessibility APIs, application scripting or the browser DOM. Results will be published as a compatibility table and can be reported by users through the "App compatibility" issue form.
