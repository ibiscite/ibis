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

### Coverage ledger for the maturity pass

"Configured" below means a harness or build job exists. It does not mean its
hosted run passed. Preserve the commit SHA and runner details with each result;
the native smoke report must pass before claiming its configured cell verified.
Build-only evidence establishes compilation and packaging, not window behavior.
The final hand checklist in [testing.md](testing.md#manual-only-checks-final-pass-before-sharing)
is reserved for the remaining speech, physical-session and feel checks. Pending
scenarios that can run in automation still need that automation before handoff.

| Requested scenario | Automated scope and inspectable evidence | Remaining verification |
| --- | --- | --- |
| Windows 10, Windows 11, Windows Arm | Desktop CI configures native smoke on Windows Server 2025 x64; preview workflow targets x86_64 Windows. | No Windows 10/11 or Arm native run is established by that server image. Add matching runner/VM/architecture evidence before claiming support. |
| macOS 13+, Intel and Apple silicon | Desktop CI configures native smoke on macOS 15's runner architecture. Preview builds target aarch64 and x86_64. | An Intel cross-build is not an Intel UI test. macOS 13/14 and both actual CPU architectures need native execution evidence, including TCC outcomes. |
| Ubuntu LTS, Fedora, X11, Wayland | Desktop CI configures Ubuntu 24.04 Xvfb/Openbox X11 smoke; preview packages build on Ubuntu 22.04. Linux preview acceptance uses automation only. | Fedora, GNOME/KDE/wlroots, and Wayland portal/copy behavior have no native evidence from Xvfb. The optional future Linux checklist is not a maintainer handoff requirement. |
| High DPI and mixed-DPI displays | UI tests check structural accessibility and numeric theme contrast; Xvfb uses one 1280×800 screen. | Physical scaling, screen switching and text readability remain in the final feel/display checks. No mixed-DPI claim follows from screenshots. |
| Non-English layouts and IMEs | Core search/property fixtures cover multilingual data; interface keyboard handlers test synthetic `isComposing` guards. This is not an actual OS input-method session. | Native shortcut mapping, composed input and layout-specific labels need session evidence. Check the user's normal layout and composition at handoff; #196 remains a known layout-label limitation. |
| Paths with spaces and non-Latin characters | Native smoke creates `synthetic 引用.bib` inside a directory containing spaces; core tests cover ordinary/verbatim Windows identities. | A completed native run verifies that path cell only; it does not verify cloud-file providers or network shares. |
| OneDrive, iCloud and network drives | Core [source-watch integration tests](../crates/ibis-core/tests/source_watch.rs) exercise interrupted writes, replacements, deletion, symlinks and recovery with local synthetic files. | Provider placeholders, offline sync, share disconnects and real lock durations need controlled provider/share scenarios. No real accounts or personal libraries are used by current tests. |
| Two copies of the app | Native smoke launches a second process, requires successful exit and activation of the original picker's focused search, and checks the primary process stays alive. | Configured assertions need a passing native manifest. Simultaneous launch races and duplicate helper ownership still need dedicated evidence. |
| Sleep and wake | Watcher integration tests exercise settled event bursts, not suspension. | Add suspend/resume evidence where runner capabilities allow it; otherwise include one physical sleep/wake check at final handoff. |
| Display unplugged while running | Pure pointer/display placement tests can exercise geometry decisions; a one-display Xvfb run cannot unplug a display. | Add a virtual-display scenario or one final physical display-disconnect check. Check active picker location and recovery after reconnection. |
| Library deleted or renamed while open | Core watcher tests cover deletion, atomic replacement at the same path and recovery; native smoke checks a deleted file's visible error, retained references, empty reload and recovery. | Moving the selected file to a different filename while open is a separate native scenario; atomic replacement does not prove it. Cloud-drive and network-share rename semantics remain separate cells. |
| A 64 MiB library | The [exact-boundary core test](../crates/ibis-core/tests/source_size_boundary.rs) uses a filename with spaces and CJK, accepts exactly 64 MiB on initial load and reload, preserves both synthetic records without warnings, then rejects one extra byte. [CLI tests](../crates/ibis-cli/tests/cli.rs) also reject an oversized sparse file. Native smoke measures 100,000 generated references. | The generated 100k corpus is not guaranteed to be 64 MiB. The core boundary test does not establish native window responsiveness or memory use; an exact-boundary native load remains a separate gap. |
| Settings edited while running | Core [settings tests](../crates/ibis-core/src/settings.rs) prove updates retain external edits and refuse invalid/newer files. Native smoke changes a valid shortcut field on disk while open, copies a citation, checks the native format save preserved that field, and restores its owned settings/backup bytes. It also checks invalid/newer startup messages and preservation. | A passing native manifest is required for the command-path claim. The test does not claim live shortcut re-registration or automatic settings reload while running. |
| Quit during reload | Desktop unit tests reject stale loads after a newer request; core watchers stop through their lifetime handle. | Add a native subprocess scenario proving prompt shutdown during a large reload and valid preferences on the next launch. Stale-request tests do not establish process shutdown. |

On macOS, the Control+Command+R default introduced in #188 refers to the physical
US-keyboard R position. The displayed label is not yet derived from the active
keyboard layout (#196). Keep that limitation visible until a native layout test
and a matching label implementation establish otherwise.

The full target matrix is operating system/session × keyboard layout (US,
AZERTY, Dvorak, German, Japanese input method) × destination (word processors,
Google Docs, Overleaf, Markdown editors, Notion, email clients, chat apps,
terminals, code editors, spreadsheets, remote desktop) × output format.
Future insertion cells should paste a synthetic string containing accents,
CJK text, italics and brackets, then read it back through accessibility APIs,
application scripting or the browser DOM. That expanded destination matrix is
not implemented by the current copy-only smoke suite. Record each completed
cell here; users can report compatibility through the "App compatibility"
issue form.
