# macOS design guide

Use the Mac’s own controls and interaction patterns as the reference for Ibis.
Spotlight is the reference for the picker, System Settings for searchable
grouped forms, and Finder for window-modal tasks. The reference measurements
on this page come from macOS 27.2.1 on 2026-11-26. Use the current system when
reviewing a change. Ibis’s product rules and recorded departures are in the
[design index](../design.md). Apple’s platform guidance is in
[Designing for macOS](https://developer.apple.com/design/human-interface-guidelines/designing-for-macos).

On the Mac, Ibis is becoming a native AppKit/SwiftUI app
([0065](../decisions/0065-one-warm-engine-thin-shells.md), amended
2026-11-26): there, use Apple's standard controls at their own metrics rather
than measuring and copying them. Windows (WinUI 3) and Linux (GTK 4 with
libadwaita) follow in that order ([0066](../decisions/0066-native-shells-use-system-parts.md));
each system keeps the Tauri app, with fixes only, until its native app reaches
parity.

### The native recipe

The [native app](../../native/macos/README.md) is layout over the system's own
parts ([0066](../decisions/0066-native-shells-use-system-parts.md)). It draws
nothing that macOS already draws.

- **Window:** a non-activating `NSPanel` on Liquid Glass (`NSGlassEffectView`)
  on macOS 26, the HUD material (`NSVisualEffectView`) on macOS 15, and an
  opaque window under Reduce Transparency. Its corners are 26 pt. Increase
  Contrast adds a separator outline. The app targets macOS 15 and later: any
  API newer than 15 goes through a shared part with an availability check
  (`SystemGlass`, `.systemGlass(corner:)`), never a screen alone.
- **Content:** SwiftUI. It uses standard buttons (`.borderedProminent`,
  `.borderless` and `.link`), `Menu`, `Picker`, popovers, `ProgressView`,
  `ContentUnavailableView`, `Form` with the grouped style and
  `NavigationSplitView`. Text fields that need exact focus and caret control
  are `NSTextField`s.
- **Colour and symbols:** semantic system colours and the person's accent.
  Icons are SF Symbols. The bird is the brand's.
- **Menus:** Actions (⌘K), the format menu and row menus are `NSMenu`s, opened
  at the control that owns them, with system section headers.
- **Grid:** one scale multiplies every size, so Larger Text and ⌘+ scale the
  whole picker. The lead column (48 pt) holds the bird and kind symbols. Text
  starts after it.
- **Review:** `scripts/native-macos.sh shots` saves every review state in
  light and dark from the invented [review library](../../native/macos/Review.csljson).

### Native app direction

The maintainer chose these from A/B mock-ups on 2026-11-28 (round four); they
are built in this order of need.

- **Settings are pages for what you have:** General holds the few global
  choices; then one page per library and one per app you have cited into,
  found as you use them. An app's page holds its format, paste and text
  rules, a library's page its location and keys. Depth sits behind each
  row's **Details…**, and search finds every option.
- **Your clipboard comes back:** after a paste Ibis restores what you had
  copied, marks the citation as transient so clipboard managers skip it, and
  leaves the clipboard alone if you copied something in between.
- **The opening key is a chord** (as ⌥ Space is), offered with an
  alternative when it is taken.
- **References in Spotlight and Shortcuts, on from the start**, on this Mac
  only, with one switch to turn it off.
- **Checking a document is a line in the picker:** with an empty query, the
  first row reports the document in front (“2 citations aren’t in
  thesis.bib · Fix”); ↩ fixes, ⌘↩ opens the full report.
- **Agent suggestions wait in one review list** (optional AI): each with
  before and after, Accept, Reject and Undo; nothing changes until accepted.
- **More ways in:** a Cite control in Control Center, a Services item for
  selected text, and a short glow at the caret after a paste, beside the menu
  bar item.
- **Three places, three jobs** (finding and reading, the same day): the
  picker cites, Peek glances, and the Library and reader open only when asked
  for. Ibis stays in the menu bar until the Library or reader opens; then it
  shows in the Dock and ⌘-Tab, and leaves again when they close.
- **Peek shows everything useful at once:** the first page, the passage that
  matched with its page, the person's own notes and highlights, and where it
  is already cited, read by VoiceOver as one summary.
- **Passages are found in the same search:** matches inside PDFs appear as
  “In the text” with the sentence and page; ↩ cites with that page.
- **Ibis has its own quick reader:** a normal Mac window that opens at the
  page, with contents and highlights; selected text cites with its page or
  quote. Pages are drawn in the isolated renderer
  ([0036](../decisions/0036-reader-trust-boundary.md)), never by a PDF parser
  beside the library; Open in Preview stays one click away.
- **The Library window is a workspace for writing:** lists by job first
  (This document, Reading now, Added this week, Needs a fix, Suggestions),
  then each manager's collections and files. Agent suggestions and document
  reports live there.

## What the interface does

- **The picker is a launcher.** Keep the field, results and delivery in that
  order. Show results while typing, keep keyboard focus visible and keep the
  search and first result in place as details open. A result always shows its
  title and the metadata that distinguishes similar works. Selection changes
  paint only. Compact and Comfortable change the density of every row in the
  list. See [Picker](picker.md) and [0062](../decisions/0062-closed-parts-owned-space-fitted-layers.md).
- **Content leads.** Let type, alignment and space establish hierarchy. Give
  each surface one paint owner and each container ownership of the gaps and
  dividers between its children. Keep a single floating layer open and fit it
  to the region it belongs to. The result of a Settings search stays visible
  when its form settles. See [0062](../decisions/0062-closed-parts-owned-space-fitted-layers.md).
- **Make commands discoverable from the keyboard and pointer.** Show a
  command’s key beside its control and include the command in the appropriate
  menu. Preserve the Mac’s standard key equivalents and modifier order
  (⌃⌥⇧⌘). Support Full Keyboard Access and give controls a visible press
  state and meaningful accessible names. See [Keyboards](https://developer.apple.com/design/human-interface-guidelines/keyboards).
- **Follow the person’s appearance and accessibility settings.** Follow the
  system accent, contrast, transparency and motion by default. Ibis offers its
  own System, Light and Dark appearance choice. **Larger Text scales the whole
  interface proportionally**, including words, rows, controls, icons and
  spacing, in the picker and Settings. Apply one scale at the root. Check
  scrolling, focus and control access at 200% text as well. See the amended
  answer in [0062](../decisions/0062-closed-parts-owned-space-fitted-layers.md)
  and Apple’s [Accessibility guidance](https://developer.apple.com/design/human-interface-guidelines/accessibility).

## Platform building blocks

| Block | Rule | Current Tauri owner |
| --- | --- | --- |
| Windows and toolbars | Use the system window controls and keep working content clear of them. Group related toolbar commands and give the primary action its own place. Keep the window title meaningful to the current view. [Windows](https://developer.apple.com/design/human-interface-guidelines/windows), [Toolbars](https://developer.apple.com/design/human-interface-guidelines/toolbars) | `panel.rs`, `settings_window.rs`, `ui/Frame`, `ui/Window.css` |
| Settings and sidebar | Open Settings with `⌘,`. Keep its leading sidebar with search. Use short labels, grouped rows, inset dividers and a detail line where the label needs it. Separate navigation groups with space. [Settings](https://developer.apple.com/design/human-interface-guidelines/settings), [Sidebars](https://developer.apple.com/design/human-interface-guidelines/sidebars) | `ui/SideNavigation`, `ui/SettingsSearch`, `Section`, `Group` and `Row` in `ui/Layout.tsx` |
| Search and lists | Name what the field finds, show results on input and make clearing discoverable. Keep row alignment and geometry through hover, focus and selection. [Search fields](https://developer.apple.com/design/human-interface-guidelines/search-fields), [Focus and selection](https://developer.apple.com/design/human-interface-guidelines/focus-and-selection) | `ui/SearchBar`, `ui/Option`, `ui/Band` |
| Menus and pop-ups | Use short command names and an ellipsis when the command needs more input. Dim unavailable menu commands and use checkmarks for their state. A pop-up shows the current choice from a list of mutually exclusive values. [Menus](https://developer.apple.com/design/human-interface-guidelines/menus), [Pop-up buttons](https://developer.apple.com/design/human-interface-guidelines/pop-up-buttons) | `ui/Menu`, `ui/PopupButton`, `ui/Layer`, native `menu_bar.rs` |
| Popovers and sheets | Keep brief detail adjacent to the control that opened it. Use a window-modal sheet for a scoped task. Use a panel when the person needs to watch the results while editing. Give modal tasks one clear route to finish and one to cancel. [Popovers](https://developer.apple.com/design/human-interface-guidelines/popovers), [Sheets](https://developer.apple.com/design/human-interface-guidelines/sheets) | `ui/Drawer`, `ui/Sheet`, `ui/ViewFrame`, `ui/layers` |
| Buttons and selection controls | Use a distinct style for the primary action. Use switches for prominent on/off settings, checkboxes for independent choices and radio buttons when a few exclusive options benefit from being shown together. [Buttons](https://developer.apple.com/design/human-interface-guidelines/buttons), [Toggles](https://developer.apple.com/design/human-interface-guidelines/toggles) | `ui/Button`, `ui/IconButton`, `ui/Switch`, `ui/Segmented` |
| Status and empty states | Use inline status for ordinary feedback and reserve alerts for critical information that needs an answer. Give a blank screen one clear next step. Use words or symbols as well as colour. [Feedback](https://developer.apple.com/design/human-interface-guidelines/feedback), [Writing](https://developer.apple.com/design/human-interface-guidelines/writing) | `ui/NoticeSlot`, `ui/Status`, `ui/ChoiceList`, `ui/UndoNotice` |
| Materials and colour | Place system material at the window or floating control layer. Keep content legible over it. Use semantic system colours and preserve their meaning in each appearance. [Materials](https://developer.apple.com/design/human-interface-guidelines/materials), [Color](https://developer.apple.com/design/human-interface-guidelines/color), [Meet Liquid Glass](https://developer.apple.com/videos/play/wwdc2025/219/) | `native_feel.rs`, `nativeAccent.ts`, `--layer-base`, `--layer-overlay`, `--layer-view` |
| Motion | Make transitions brief and interruptible. Keep focus and selection instant. Under Reduce Motion, use a short fade or show the end state immediately. [Motion](https://developer.apple.com/design/human-interface-guidelines/motion) | `--dur-*`, `--ease-*`, `ui/Overlay`, `ui/ViewFrame` |

In the Tauri shell, the picker’s window uses native material. `native_feel`
uses Liquid Glass on supported systems, falls back to popover vibrancy where
appropriate and uses an opaque surface with Increase Contrast or Reduce
Transparency. The WKWebView uses a surface tint for readable text and its
overlay blur samples the app’s own content. Settings is opaque. In an
AppKit/SwiftUI shell, use the platform’s control styles, materials and scroll
edge effects directly. The same hierarchy and accessibility rules apply.

## Accepted changes to the picker and shell

[0063](../decisions/0063-form-follows-function.md) puts each choice and message
where the person needs it. As that work lands, the search field will accept
locators and semicolon-separated works through the shared citation grammar.
Keep the drawer for less frequent details and for people who prefer locator
menus. An empty search will show works cited since opening, followed by
recents. The picker’s format palette will own format choices, including
remembered choices. The list heading will show status with at most one fix.
The delivery bar will show the citation and its action.

[0064](../decisions/0064-one-citation-one-status-generated-surfaces.md) assigns
the citation Draft, format resolution and typed status to the engine. Shells
display that state, word problems from the shared catalogue and send intents.
The same Draft can continue in another client. Focus, scrolling and open
controls are shell state. The planned Settings row recipe in 0062 takes its
rows from the settings schema at 0064 step 8. A shared rendering primitive
owns each row recipe and displayed shortcut where the shell needs custom UI.

[0065](../decisions/0065-one-warm-engine-thin-shells.md) starts by
extracting the warm engine into `ibisd` and measuring Tauri as its client.
With 10,000 generated references, Tauri must meet both the hidden whole-process
budget of 200 MiB and the shortcut-to-typeable budget of 100 ms at p95. The
AppKit/SwiftUI shell goes ahead if Tauri cannot meet the budgets together.
Record idle wake-ups alongside memory and latency on the 10k and 100k
fixtures. Either shell owns windows and controls, keeps native paste and focus
in the signed app and opens the Reader in an isolated web view. See
[release gates](../decisions/0032-release-gates.md) and
[Reader isolation](../decisions/0036-reader-trust-boundary.md).

## macOS reference measurements

These are Apple reference values, measured at 1× on macOS 27.2.1. AppKit
values come from [`macos-metrics.swift`](../macos-metrics.swift) and
[`macos-controls.swift`](../macos-controls.swift). Capture values come from
System Settings. Ibis’s implemented values live in
[`tokens.css`](../../apps/desktop/src/tokens.css). For native controls, let
AppKit or SwiftUI supply the metrics for the running system.

### Type

| macOS style | Size / line (pt) | Weight | Ibis role where applicable |
| --- | --- | --- | --- |
| Large Title | 26 / 32 | Regular | |
| Title 1 | 22 / 26 | Regular | Search |
| Title 2 | 17 / 22 | Regular | Lead |
| Title 3 | 15 / 20 | Regular or emphasised Semibold | Title |
| Headline | 13 / 16 | Bold | |
| Body | 13 / 16 | Regular | Body |
| Callout | 12 / 15 | Regular | Label |
| Subheadline | 11 / 14 | Regular | |
| Footnote and captions | 10 / 13 | Regular or Medium | |

Apple recommends 13 pt by default and a 10 pt minimum for custom type on
macOS. Both Ibis shells follow Ibis’s readability floor, target sizes and
contrast rules.
See [Typography](https://developer.apple.com/design/human-interface-guidelines/typography)
and [Ibis accessibility](foundations.md#accessibility).

### Spacing and controls

| Reference | Measurement (pt) | Source |
| --- | --- | --- |
| Push button and pop-up heights | 16 mini, 20 small, 24 regular, 28 large | AppKit |
| Search field heights | 24 small, 25 regular, 28 large | AppKit |
| Settings switch | 36 × 16, thumb about 22 × 14 | Capture |
| Menu | 24 row, 10 extra padding total, 11 separator | AppKit |
| Plain window title bar | 32 high, close button 14 × 14 at (9, 9) | AppKit |
| System Settings toolbar | 52 high, title 15 bold, window buttons centred at 25 | Capture |
| Sidebar | About 232 wide including 10 inset. Row pitch 32 medium, 24 small, 40 large. Icon 20 with an 8 gap. Groups add about 12. | Capture and AppKit source list |
| Settings content and cards | 20 margin, radius about 10, quaternary grouped fill, 10 between cards | Capture |
| Settings section heading | 13 semibold primary, about 8 above the card and 30 below the preceding card | Capture |
| Settings row | 10 side inset, inset hairline divider, about 38 for one line, 52–58 with detail, 49 with icon and chevron | Capture |

Spotlight’s result row pitch, the menu’s padding split, and popover and menu
corners and shadows still need measurements. `NSSwitch.sizeToFit` reported
54 × 24 across control sizes, so the mini Settings switch uses the capture as
its reference.

### Semantic colours

These AppKit readings describe the reference appearances. Use the semantic
colour in native controls and the matching role in the web interface.

| AppKit colour | Light | Dark |
| --- | --- | --- |
| Label | Black 84.7% | White 84.7% |
| Secondary label | Black 49.8% | White 54.9% |
| Tertiary label | Black 25.9% | White 24.7% |
| Separator | Black 10% | White 12% |
| Quaternary system fill | Black 2.7% | White 2.7% |
| Window background | `#FFFFFF` | `#1E1E1E` |
| Default accent | `#007AFF` | `#007AFF` |
| Keyboard focus indicator | `#0067F4` at 49.8% | `#1AA9FF` at 49.8% |
| System green, yellow, red | `#34C759`, `#FFCC00`, `#FF383C` | `#30D158`, `#FFD600`, `#FF4245` |
| System orange and blue | `#FF8D28`, `#0088FF` | `#FF9230`, `#0091FF` |

Ulysses and System Settings were also observed on the reference Mac. Ulysses
uses a strong title and quiet preview to organise its list, groups related
toolbar controls, and separates sidebar groups with space. System Settings
uses that same sidebar grouping and gives each form row a standard control.
Those patterns transfer to the picker’s bylines, the Settings sidebar and the
grouping of controls around the search field.

## Review a change against the Mac

1. Name the system counterpart and read its HIG page. Check its behaviour in
   the running system at the relevant control size and appearance.
2. Use a shared part or semantic token in Tauri and a standard control in
   AppKit/SwiftUI where it fits. Measure a missing reference before choosing
   new metrics. Record the source and macOS version alongside the reference.
3. Review the smallest window with an invented long German title, many authors
   and Arabic text. Include light and dark appearances, right-to-left layout,
   Larger Text, 200% text, Increase Contrast, Reduce Transparency and Reduce
   Motion. Check that keyboard focus stays visible and the next key works as
   soon as the control appears.
4. Use the Tauri design and ownership tests for shared tokens and the canvas
   for behaviour it can observe. Exercise the same visible outcomes with
   platform UI checks and VoiceOver in a native shell. Record any deliberate
   departure in [Design](../design.md#where-ibis-departs-from-macos).
5. On a major macOS release and its first point release, rerun the AppKit
   measurement scripts, recapture System Settings, Spotlight and a system menu,
   and reread the linked HIG pages. Update changed references alongside the
   corresponding shared part or token.
