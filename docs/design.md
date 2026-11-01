# Design

Ibis follows Apple's macOS Human Interface Guidelines: when in doubt, do what a
stock macOS app does (Spotlight, Finder, System Settings). Every value lives
once in [`apps/desktop/src/tokens.css`](../apps/desktop/src/tokens.css), in three
layers (primitives → semantic roles → component values), and every part lives
once as a shared component in [`apps/desktop/src/ui/`](../apps/desktop/src/ui/);
screens never restyle a part. The design canvas is generated from the app
(`apps/desktop/gallery/canvas.mjs`), so this page names a token or component
instead of repeating its pixels. [Decisions](decisions.md) win over this page.
Tags such as [C12] are cited by code and tests; keep each beside its rule.

## Principles

1. **One anatomy everywhere.** Search, list, shelf and footer, in that order, at every size; layouts fold, never reorder.
2. **One design, macOS first.** Every system uses the macOS design; only correctness follows the system (see **Per platform**).
3. **Readable first.** Nothing smaller than 12 px; with Increase Contrast, secondary text passes 4.5:1.
4. **Easy to hit.** Every target is at least 32 px, 44 px under `pointer: coarse`, whatever the drawn size.
5. **Show less, reveal more.** Things appear when useful and fold one key away; no modes, no "advanced" switch.
6. **Say what happened.** Every action ends in a visible result: copied, pasted, or why not. Colour is never the only signal. Nothing asks "are you sure?"; destructive actions apply at once with an 8 s Undo [D16].
7. **Calm motion.** Movement explains where something came from and stops under Reduce Motion.
8. **No glow, no edge rings.** Selection is a fill, never an outline. Exceptions: picture choices (see **Selection is a fill**) and a dragged library file, which rings the whole content area.
9. **Great defaults, then choice.** Most people never open Settings; no setting exists because a choice was hard.

## Where Ibis departs from macOS

Anything not listed follows Apple. A new departure is added here in the same change, or it is a bug.

| Where | Apple | Ibis |
| --- | --- | --- |
| Hit areas | Regular controls are 24 pt | Drawn at Apple's size, hit area at least 32 px (menu rows keep 24) |
| Smallest text | 10 and 11 pt captions | Nothing under 12 px |
| Accent | System accent | Follow the Mac by default, plus Apple's hues and **Ibis scarlet**, as a fixed tuned set |
| Citation colour | None | Scarlet marks the citation everywhere [C14] |
| Large buttons | Capsules on macOS 26 | Rounded rectangles at every size |
| Search icon | Magnifier | The Ibis mark by default (as Alfred shows its hat); Magnifier or None in Look |
| In-window menus | `NSMenu` | Ibis's own, matching native metrics; menu bar, tray and right-click stay native |
| Window size | Content decides | One steady size while open [C10] |
| Key caps | Shortcuts only in menus | Caps in the footer and key lists |

## Foundations

### Tokens

- **Primitives:** raw values named by what they are (the 4 px space scale, ink, measured hues such as `--scarlet`). Only roles read them.
- **Semantic roles:** what a value is for, per mode (light, dark, Increase Contrast, forced colours): labels, fills, separator, surfaces, accent, selection, status, the corner roots (`--r-win`, `--r-menu`, `--r-control`, `--r-card`), `--inset`, `--action-h`, `--key`.
- **Component values:** derived from roles by a written rule, never typed as a second number (`--r-row`, `--r-key`, `--r-mi`, `--key-gap`, `--footer-h`, `--key-fill`).
- Components read roles and component values only: no raw colours, `GrayText`, literal radii or literal shadows [D1]. A new need picks an existing step before adding a token.
- Light and dark come from `color-scheme` and `light-dark()`; components never read `prefers-color-scheme`. Roles default to CSS system colours so an unknown webview still follows dark mode and high contrast.
- `data-system` records the OS for fonts, modifier labels and native material; it never chooses a layout.
- **Exact macOS values:** colours, type styles, control heights and menu metrics are AppKit's own, measured by `docs/macos-metrics.swift` and written into `tokens.css`. Rerun it on each new macOS.

### Colour

- **Labels** [D5]: `--text`, `--text-secondary`, `--text-tertiary` (placeholders and resting glyphs, never sentences), `--text-disabled`; Apple's label ladder at AppKit alphas.
- **Fills**: Apple's fill ladder `--fill-1` … `--fill-5`. Control rest, hover and pressed are tertiary, secondary and system fill (`--fill-control` and its states); quiet hover and grouped content are quaternary.
- **Separators**: one `--separator` for hairlines and dividers; a field edge that meets 3:1. Never a coloured border on a control. **One edge per shape**: a border, an inset shadow or an overlay's shadow ring, never two; a divider belongs to one side.
- **Accent and selection** [D6]: every Accent choice resolves to three roles with light and dark values: `--accent` (controls, switches, progress, focus), `--selected` with `--selected-text` (selected rows, menu highlight, focused citation; text 4.5:1, never brighter in dark than light) and accent text on surfaces (4.5:1). The default button is `--accent` with `--text-on-accent`. Hover and pressed selection darken toward black.
- **Focus** [D8]: `--ring`, Apple's keyboard focus indicator, only on `:focus-visible`; solid accent with Increase Contrast.
- **Status** [D10]: `--status-ok`, `--status-warning`, `--status-error`, the Mac's system hues. A dot or symbol, optionally on a wash; status words stay in a label colour.
- **The one Ibis red** [D9]: `--scarlet` is the flat mark colour for the search-bar mark, the shelf beak, the added tile, the citation and the logo; it is never recoloured for a material. `--scarlet-text` is scarlet set as text, lightened over translucent materials. `--on-scarlet` is what sits on a scarlet fill. On a selected row the added tile becomes `--selected-text` with a scarlet check.
- **Ibis scarlet as accent:** offered in Look after a divider; it pairs dark ink in dark mode so white never vanishes into the brighter scarlet.
- **Contrast:** text, placeholders and key-cap labels 4.5:1 against rendered pixels on every material and wallpaper (#774); focus rings, meaningful icons and only-cue boundaries 3:1.

### Materials and layers

- **Surfaces** [D4]: `--surface` (window), `--surface-grouped` (grouped content), `--surface-overlay` (every overlay), `--surface-field`; `--raised` only for cards and the segmented thumb. No other surface colours.
- **Window glass** [C13]: on macOS one glass sheet for the whole panel (Liquid Glass, popover vibrancy fallback) under a surface veil, so colour shows through but never readable shapes. Never glass on glass or CSS blur posing as the desktop. Elsewhere an opaque surface.
- **Readable overlays** [C13]: menus, the keys sheet, dialogs and the shelf's reference menu use the panel's glass, denser than the window, so content behind reads as soft colour only.
- **Elevation** [D11]: one overlay shadow for every menu; none on rows, chips or inside an overlay. In dark mode fills and hairlines separate layers first.
- **Layers** [D15]: one z-order (`--z-*`); one overlay of each kind at a time; overlays keep `--inset` from the window edge and never leave it.
- **Increase Contrast** uses the Mac's stronger set: stronger labels and marks, a solid ring, no scroll fades.
- **Reduce Transparency** makes every material opaque.
- **Forced colours** collapse roles to system pairs (`Canvas`, `CanvasText`, `Highlight`, `ButtonText`, `GrayText`): opaque materials, no shadows, a 1 px control edge, scarlet becomes `CanvasText`.

### Type

- The system UI font (SF Pro on macOS), never another platform's; `--mono` only for citation keys and key-format output.
- The Mac's text styles as `--type-*` roles: search (title 1, frameless like Spotlight), title (row titles), body (text and menus), meta (descriptions, footer, shelf), label (the smallest), display (Settings headings and the welcome).
- Weights 400, 500, 600; no uppercase transforms; italics only for "why it matched"; `tabular-nums` on years, counts and pages.
- Type and vertical sizes are rem; radii and hairlines stay px. Larger text multiplies the root by `--larger-text-scale` and grows the window by the same factor; system scaling multiplies with it, capped at 2.

### Spacing and sizes

- **Spacing** [C1]: S = `--inset` on every OS. Every fill keeps S from the window edge and from any divider it touches; the list's viewport starts S below the header divider and ends S above the shelf. 4 px grid; 2 px only as an optical nudge.
- **Control sizes**: the picker uses Apple's large controls and Settings the regular ones, as Spotlight and System Settings do. Targets grow to 32 px without changing the drawn size.
- **Two columns** [C3]: icons, the search glyph, the status dot and type tiles centre on the tile column (`--tile`); text (row titles, the query, status text, the shelf citation) starts on the title column. The trailing edge is where row text ends; the last header control, footer key cap and year column end there. Distances run from the logical start edge, also in RTL.
- **Rows** [C4]: three densities (decision 0020): Compact (one line), Comfortable (two, the default), Spacious (three). Density changes rows only. Rows are 2 px apart in results.
- **One steady window** [C10]: the picker keeps one size the whole time it is open, in every state; nothing measures content to choose a height and nothing animates a resize. Medium 720 × 480 is the default, minimum 480 × 360; Look › **Window size** offers Small, Medium, Large and **Compact** (a short bar that grows once to Medium on the first key, then never shrinks). A dragged size shows as Custom and is kept per display.
- **Nothing clipped, one scroller**: no state is ever cut off; only the middle area or an open menu scrolls, never two at once. In a short frame with large text the header gaps and footer hint labels give way first.
- **Width classes** [C17]: below 28.75 em footer hints show keys only; at 60 em the preview sits beside the list, below it replaces it.

### Corners

- **Corners** [C15][D3]: rounded rectangles on one concentric chain. A shape near its container's edge keeps equal gaps and takes the container's radius minus that gap; every derived radius is computed in `tokens.css`, never typed. Window (`--r-win`, 16 on every macOS) → rows, fields, shelf (`--r-row`) → footer actions (`--r-action`) → key caps (`--r-key`); menus (`--r-menu`) → menu items (`--r-mi`).
- **Floors**: `--r-floor` for surfaces, `--r-floor-mark` for text-sized marks.
- **Neighbours share a radius**: shapes side by side at one depth use one radius and one gap.
- Overlays float on their own layer and keep `--r-menu`; Settings cards use `--r-card`.
- **Capsules** (`--r-full`) only for switches, count and tag pills, progress tracks and toasts. **Badges** over content (the shelf's remove ×) are circles.
- The miniature pictures in Settings are the real picker scaled and keep its radii.

### Icons

- macOS: SF Symbols and the system's own app and file icons where the system provides them, such as the installed Zotero icon (bundle id `org.zotero.zotero`, read through NSWorkspace).
- Fallback, and on other systems: Ibis's own set in `brand/icons/`, built by `python3 brand/build.py`; a 20 pt grid, 16 pt live area, one stroke weight matched to SF Symbols Regular, one stroked path each. The icon guard rejects anything else. A missing app or failed icon read shows the books glyph.
- Ibis never ships Zotero's logo or other apps' artwork.
- Seven type icons (article, book, chapter, thesis, web page, report, other) stay distinct from attachment drawings. Toggled-on keeps the same outline; the control's fill carries state.
- **The Ibis mark** in the search bar is the flat `brand/` bird in `--scarlet`, eye open; flat `CanvasText` in contrast modes. It never appears on a control and never mirrors.
- Settings page icons use Apple's icon shape; Mac icon layers are flat, with no added bevels or shadows.

### Key caps

- One cap per shortcut, written as menus write it: ⌘K, ⇧⌘F, ↵ on macOS; Ctrl+K on Windows and Linux (`ui/KeyCaps`).
- Caps appear only in the footer and in key lists. Menus write shortcuts as plain trailing text in native order (⇧⌘C) [C8].
- A cap is `--key-fill` with `--r-key`, label type, never an outline or scarlet. Inside a quiet button it keeps its fill; on the primary button it is a veil of `--text-on-accent`. A cap in a footer action keeps `--key-gap` on every side.
- "or" alternatives keep two caps. Decorative caps are hidden from screen readers; `aria-keyshortcuts` names the keys.

### Motion

- Durations come from `--dur` and its siblings; entrances use `--ease`, exits `--ease-in`. No bounce; chevrons never rotate; macOS menus appear at once.
- **Never** animate anything longer than 280 ms. Never animated: list content while typing, row height, the caret, layout on hover, the citation text, ticking counts, focus rings, anything that loops except a busy indicator.
- Reduce Motion: no translate, scale or slide; fades of at most 100 ms; every effect shows its end state.

### Accessibility

- Every action has a key and a visible, named control; hover is never the only way to see something; visible labels match accessible names.
- Focus order: search, header pop-ups, pin, browsing tabs, footer actions, More. The list is not a Tab stop (↑ ↓ move it from search). Nothing takes focus except search on open.
- Screen readers hear a row as title, first author, year, container, type and why it matched. "Copied" is announced politely; a Rows change is announced once.
- An input method's confirming Enter never copies or pastes. Every release passes the whole citation with VoiceOver and NVDA and with CJK input methods (decision 0032).
- **Capitalisation:** on macOS, menu items, buttons, window, sheet titles and choice rows use title case ("Keep Window Open"); everything else is sentence case. One string never appears in both styles.
- **Long text:** containers grow to fit wrapped text with the same padding; in Settings the control moves below the label. Data truncates with a tooltip; file names shorten in the middle. Budget +35 % for German and Greek. RTL uses logical properties; citation keys stay LTR. Check at normal, Larger and 200 % text.

## Components and their states

Each component in `apps/desktop/src/ui/` wraps one React Aria component (decision 0021), owns one stylesheet that reads tokens only, and is the only way a screen draws that part. A screen needing a new control adds it to `ui/` with its line here first. Shared rules every window uses live in `ui/Foundation.css`.

**States.** Hover (`@media (hover: hover)` only) never moves or resizes anything. Disabled uses `--text-disabled` and stays focusable in menus. Busy shows a spinner after 400 ms with `aria-busy`. Invalid fields show words and an icon, never a red ring.

| | Pointer over it | Chosen or current | Keyboard focus |
| --- | --- | --- | --- |
| Result row | Quiet tint | `--selected` fill | The same fill; never a ring |
| Menu item | Menu highlight | Check in the leading column | Menu highlight |
| Segmented | Quiet tint | Darker thumb on a quiet track | Ring around the group |
| Buttons, pop-ups | One step darker | Primary: accent fill | Ring |
| Switch | No change | Accent track | Ring |
| Search field | None | None | The caret only |

- **Selection is a fill** [D7]: result rows use the full `--selected` fill with paired text, no side pill; a kept-open picker without focus dims it. Picture choices select as System Settings does: an accent ring outside the picture, following its corner, and a semibold caption.
- **Focus ring** [D8] only on keyboard focus: outside on controls, inside on rows (results, choice rows, sidebar), so no scroll area clips it.

| Component | Rule |
| --- | --- |
| Button (`ui/Button`) | Primary (accent, one per view), secondary (`--fill-control`) and quiet (no fill at rest, shared hover). Rounded rectangle, `--r-control`; `--action-h` large, regular in Settings, small (`--control-small-h`) in the shelf and footer; trailing key caps. |
| IconButton | Square, one size and glyph size per context, `--r-control`; rest transparent with secondary ink, toggled-on is the pressed fill with the same outline. Header ×, pin, sheet close, chip × and Settings search clear all use it. |
| Pop-up button (`ui/PopupButton`, `ToolbarMenu`) | Shows its value and a chevron that never turns; Space, Enter or ↓ opens it; a pick or Esc returns to search. Named by what it sets ("Copy as Pandoc"). |
| Menu (`ui/Menu`) | See **Menus**. |
| Segmented | `--r-seg` thumb on a quiet track. |
| Switch (`ui/Switch`) | The one on/off control: accent track when on, `--text-on-accent` thumb, size from `--switch-*`. |
| Search field | Frameless and large in the picker; a hairline field with no fill in Settings, glyph on the icon column. |
| Settings group and row (`ui/SettingsGroup`) | See **Settings**. |
| Shortcut table (`ui/ShortcutTable`) | Small secondary heading, one card of rows, action at the start and keys at the end. |
| Key caps (`ui/KeyCaps`) | See **Key caps**. |
| Shelf (`ChipGroup`, `LocatorToken`, `CitationDetails`) | See **Shelf**. |
| Footer and status line (`LibraryStatus`, `SourceAction`) | See **Footer**. A status line is words, never a filled band. |
| Toast and copied notice (`UndoNotice`, `CopyReceipt`) | Capsule toast, 8 s, pauses on hover and focus; see **Notices**. |
| Progress (`ui/Progress`) | Accent fill on a quiet track; known totals show a percentage; unknown totals a moving segment; Reduce Motion shows a static dashed bar. |
| Choice cards (`ui/ChoiceList`) | See **First run and choice screens**. |
| Picture choice (`ui/PictureChoice`) | Equal columns, one per choice, captions on one baseline; never wrap three as two plus one. |
| Sidebar (`ui/SideNavigation`) | Single-selection list with `aria-current="page"`, the Mac's medium sidebar row, selected fill and paired text. |
| Scroll area (`ui/ScrollArea`) | The system's own scroll bars; the side inset sits inside the bar; `--scroll-fade` only at an edge with more beyond it. |
| File drag (`ui/FileDrag`) | A draggable file icon with button states. |

**Tables** size columns to content and never break or hyphenate a word. **No browser disclosure controls**: never `details`/`summary` or raw triangles; use always-open groups.

### Menus

- [C9] In-window menus (format, library, More, a reference) use `--r-menu`, `--menu-pad`, AppKit's item height, separators and the overlay material.
- A menu with any check keeps a leading check column for every row, so labels share one start and shortcuts line up at the end.
- **Menu heading**: small bold secondary type that never reads as an item.
- Sections keep one order: what it is about, what you can do, Ibis itself. Labels name the result; a quiet example of the output may trail.
- Menus over seven items get a filter field aligned with the labels. Header menus open `bottom end`, footer menus `top end`, flipping and shifting to stay within the window.
- Menus size to their labels up to the viewport, then wrap; a scrolling menu ends on a whole row. Any pointer movement highlights an item.
- **More holds actions, not preferences**: Show in Zotero or Open thesis.bib (⌘O); Rows › (Compact, Comfortable, Spacious); Keep Window Open; Show What Will Be Pasted; Keyboard Shortcuts (⌘/); Settings…; Quit Ibis. Each thing appears in one place.

## Picker

### Search bar

- Order: search glyph, query, clear ×, format pop-up, pin. The header is its tallest control plus 2S, with a divider below [C2].
- Look › **Search icon**: Ibis mark (default), Magnifier or None. With None the query starts at the leading inset.
- `#tag`, `y:2019-2022` and `a:name` plus a space become removable chips; Backspace in an empty box removes the last.
- **Pin** [C6]: a neutral IconButton at the header's end; on, it shows the pressed fill, never the accent. Esc never unpins.
- The format pop-up names the exact output and the captured app (`APA · Word`); hidden until a library is chosen.

### Results

- **List and row**: type tile, title, meta line, trailing year (or "No date") level with the title's first line in every density. Comfortable's meta line: authors as in a citation, then the container; Spacious adds the type in `--text-tertiary`; Compact puts the first author after the title. The citation key is in the accessible name, not the row.
- On hover or selection the add button replaces the tile (+, or a check when added) without reflow; a row in the citation shows the scarlet tile with a check.
- Titles wrap to two lines then end in an ellipsis; the byline stays one line.
- **Hover card**: resting half a second on a cut-off row shows its full details beside it; it never takes focus. ⌘Y opens Peek for the selected row.
- **Hover** [C5] only tints a row under a moving pointer; a click selects, so reaching across rows for Copy never changes what is copied. Double-click copies.
- **List scrolling**: a clicked row stays under the pointer and only that row consumes the click, even if results re-render first. Arrow keys take back ownership.
- **Empty search** [C11]: a new library shows the hint ("Type to find a reference") centred in the middle area, with no keys. With history it shows **Cited here recently** (up to three, for the app Ibis came from) then **Recently added** (three), as ordinary rows navigated as one list.
- **Typing keeps the picker still**: rows, headings, shelf and footer stay until the next reply; nothing blanks. A click on a row from an earlier query does nothing.
- No matches: "No matching references", "Try a surname, year or fewer words.", then Clear Search. When nothing matches every word, closest matches show under **Closest matches** and keep a "Closest match" pill on the shelf.

### Search rules

Built in `ibis-core` so every front end shares them (#1204).

- Any word order; case, accents and letter variants folded (`mueller` finds Müller); "et al." and "and" ignored; every word must match.
- Per word: whole word, then word start, then inside a word (three or more characters, or CJK), then one typo in words of five or more letters. Numbers, years, identifiers and tags are never guessed.
- Field hints: `a:` people, `t:` title, `j:` journal, `k:` key, `#` tag, `y:` year or range; quotes keep a phrase. An exact citation key always matches.
- Ranking: exact key, exact title, matches as typed, then typos, then abstracts; field weight; newest year; stable order.
- **Pasted lists** (#1326): keys in any supported citation syntax, or identifiers, in paste order under "Pasted list"; missing ones under "Not in your library: …". Up to 1,000 entries or 64 KiB; otherwise ordinary search.

### Shelf

- **Shelf** [C7]: one line above the footer showing what Enter pastes: the beak at the tile column's end, the citation on the title column. Shown while there are results or a citation is being built; hidden on choice screens. Results never move when a reference is added.
- Several references are segments of one citation; a pointed segment becomes a soft pill with a round × badge outside its text. ← → select a segment; Delete removes it with Undo.
- **One line, details on demand**: a segment opens an ordinary menu: the reference's name as a heading; **Name in brackets**, **Name in the sentence**, **Year only** with quiet examples; **Page…** editing in place; the source action; **Remove from Citation ⌫**.
- On copy a closing beak mirrors the opening one and a "Copied" capsule draws its check; Reduce Motion shows the end state. Each copy is confirmed once, in one place.
- Pending output dims, then "Formatting…"; Enter waits. A failed format says why and offers another style; refusals stay visible until corrected.
- **Show What Will Be Pasted** (⌘I, on by default): off hides only the line.
- The shelf keeps the last citation through a running search and when the next search finds nothing.

### Footer

- **Footer** [C8]: status dot and text at the leading edge (dot on the tiles' column), the source action, "More ⌘K", and last, on the rows' trailing edge, the one primary action: "Paste into Word · APA ↵" or "Copy N as <format> ↵".
- Every hint is label first, then keys, on one baseline. When the footer does not fit, shorten the status, then drop hint labels to keys.
- **Quiet bands**: inside the shelf and footer only the main action (Copy or Paste) is filled. The library status, the source action, More, the reference count, notes and Details are quiet. A band that is itself a fill never holds another filled control.
- Never show an action twice on one screen. With ⌥ held: "Add to citation ⇧↵", the Esc step, then More.
- **One place for each status**: library state lives only in the footer status; citation refusals and settings notices use the middle area.

| Case | Footer |
| --- | --- |
| Starting, first run | "Checking library…", "No library chosen" |
| Loaded | "Zotero · 1,204 references" or "thesis.bib · 1,204 references" |
| Changed while open | "… · updated 2 minutes ago" (only when references changed) |
| Reloading | "… · reloading…", count kept |
| Problem, old copy kept | "Zotero not updated · 2 h", then one fix button ("Open Zotero", "Find thesis.bib…") |

- Counts include bibliographic records only. Background work shows a small progress ring before the words.
- The status opens the **library menu**: the library with count and age; with several Zotero libraries, a **Search in** heading, All Libraries, then one checked row per library; a library that could not load stays dimmed with "Couldn't load"; Choose Library… ⇧⌘O; Reload Library.
- **Open the source** (⌘O): the words always name the target. **Show in Zotero** selects the reference in Zotero (starting it if closed); **Open thesis.bib** opens the file; **Open Zotero** only when Zotero is the source and not running. ⌥ turns "Open thesis.bib" into "Show in Finder". If the source cannot open, offer Choose Library… or Find….

### Keys

`KEY_TABLE` in [`apps/desktop/src/keys.ts`](../apps/desktop/src/keys.ts) is the one source of shortcuts; the keys sheet, footer and More read it. A key is added only with the code that handles it.

| Input | Does |
| --- | --- |
| ⇧⌘Y (Ctrl+Shift+Y elsewhere) | Opens Ibis from any app (the default; follows the active layout's letter) |
| Click / double-click a row | Selects [C5] / pastes or copies what the shelf shows |
| Enter, or the footer action | Pastes or copies what the shelf shows; success hides an unpinned window |
| ⌘-click, the tile's +, ⇧Enter | Adds to the citation, in order |
| Esc | Steps back one layer: menu or dialog, preview, then clears search and filters (the shelf stays), then hides |
| ⌘O / ⇧⌘O | Opens the source / Choose Library… |
| ⌘K | More |
| ⌘/, or `?` outside a field | Keyboard shortcuts |
| ⇧⌘P | Keep window open |
| ⇧⌘F | The format chooser |
| ⌘I / ⌘Y | Show what will be pasted / Peek |
| ⌘+ / ⌘− | Text size |
| Typing outside a field | Types into search |

### Copying and pasting

- Enter does what the footer says. Pressed while results or the preview load, it waits and runs once. Repeated activations on the same citation never copy twice; a changed query, reference, page or format, arrows, Esc or hiding cancel a waiting Enter. A stale reply never replaces newer shelf or feedback.
- **First paste in an app**: a browser, a code editor or an unrecognised app opens the format chooser once; the pick is remembered for that app (decision 0031). An unrecognised app also asks **Paste into [app name] from now on** or **Copy only**; answering never replays the held Enter.
- Chooser items read as plain names with a quiet example ("BibLaTeX brackets   \parencite{key}"). "Just this once" applies to one copy; "Automatic" forgets the app's choices; "Use as global default" changes the fallback. A pick says its scope ("APA for this citation").
- Refused apps (terminals, remote viewers, virtual machines) and targets Ibis cannot prove only copy, with the reason. A permission detour keeps the prepared citation and waits for a fresh Enter.
- A successful copy hides Ibis, returns focus to the previous window and shows the notice. Pinned, Ibis stays.

### Notices

- "Pasted" or "Copied" shows after the picker hides, in a window that never takes focus: one card near the top centre on every system, ignoring the pointer. Pasted shows a `--status-ok` check; copied without a paste shows a neutral check and the reason. It never clips: a long citation moves to a second line, ending "+4 more".
- No notice carries a side bar or a one-sided border. Errors are one sentence beside their cause, never a modal.
- In-window toasts ("Removed · Undo") last 8 s, name their action and accept ⌘Z.

| Reason | Words |
| --- | --- |
| Pasting off or unavailable | Copied. Press ⌘V to paste. |
| Secure Event Input on | Copied. Secure typing is on, possibly in another app. Press ⌘V to paste. |
| Accessibility missing | Copied. Ibis needs Accessibility access to paste safely. Press ⌘V to paste. |
| Unapproved app | Copied. Automatic paste is not approved for this app |
| Copy only chosen | Copied. Copy only is set for this app |
| Target changed | Copied. The target app or window changed |
| Clipboard replaced | Copied. The clipboard changed; copy the citation again |
| Expired | Copied. The paste expired; paste it yourself |
| Windows keys stuck | Copied. Press and release Ctrl and V |

Never call the destination a password field. The right paste key is shown per system.

### Window behaviour

- **Opening**: on the focused window's display, at its last place there (first time: centred, top in the upper third), with a short fade and focus in search. A search from under a minute ago returns selected; text typed while opening is kept. The window is current in its first frame.
- **Moving and resizing**: drag the empty header; snap to centre, upper third and corners. Settings › Advanced › **Where Ibis opens** offers Centre and the four corners; **Reset Window Position** restores the default.
- **Clicking outside** hides Ibis and the click reaches its target, unless pinned or a dialog, drag or file chooser is open.
- Overlays close when the picker hides and do not reopen. Tooltips appear after 600 ms and never take focus.
- **Never** take keys people type with (so not Ctrl+Space or Ctrl+Alt), open by itself, appear in the Dock, taskbar or Alt+Tab (only Settings and a pinned picker do), or animate past 280 ms. On X11 the first press over a full-screen app is ignored and a second within two seconds opens Ibis.
- **Platform limits**: macOS a non-activating floating panel, also over full-screen apps; Windows copies instead of typing into elevated apps; Wayland copies and says "Press Ctrl+V" ([compatibility](compatibility.md#linux-shortcuts-and-tray)).
- Hidden Ibis waits for events; nothing polls while it is hidden.

## First run and choice screens

[C12] First run, no library, an empty library and a library that could not be opened share one pattern: one calm group centred in the steady frame, a title and one line, then result-style rows (tile, title, one line of help) in a column, and any note below.

- One row per goal: label the outcome, start with a verb, never two rows to the same place. At most one suggested row gets an accent tile.
- Rows are a list box: ↓ from search enters, arrows move, Enter or a click chooses. Choice screens hide the result heading, shelf and Copy.
- A superseded source search or file request reports nothing.
- **Welcome**: the whole window, with no search header. The mark, a title, the line "Ibis finds a reference and pastes its citation where you're typing. It only reads your library, never changes it.", then four starts as a 2 × 2 grid of tiles (Zotero, a .bib file, Find them on this computer, Try sample references), each with its own centred glyph. The note names the registered shortcut ("After this, open Ibis from any app with ⇧⌘Y."), or says **No shortcut is set** with how to open Ibis instead. Esc dismisses a menu, then hides.
- **Opening a library**: nothing for 400 ms, then the mark, "Opening example.bib…", "Reading your library. Zotero can stay closed." and a thin progress bar. Rows kept from before stay instead.
- **No library**: "Where do your references live?" with the welcome grid; nothing is searched until Find is chosen.
- **Empty library** (a successful load with zero references): "No references in thesis.bib", "Choose a library with references.", then **Use Zotero**, **Choose a Library File…**, and for a file **Open <file> ⌘O**; below, "Or drag a .bib or CSL-JSON file into this window."
- **Could not open, nothing kept**: the title names the library and the fix; "Use another library" shows the no-library screen. A failure with references kept stays a notice above them.

| Problem | Title | Rows |
| --- | --- | --- |
| `missing` | Can't find thesis.bib | Find thesis.bib, Use another library |
| `unreadable`, `other` | Can't read thesis.bib / Can't open thesis.bib | Try again, Use another library |
| `zotero_closed` | Zotero isn't open | Try again, Use another library |
| `zotero_access_off` | Zotero isn't sharing your library | Try again, Use another library |

- Setup finishes itself: Ibis never asks "What do you write in?"; when Zotero sharing turns on, the screen continues on its own; with Zotero closed Ibis reads the library anyway.
- **Try sample references** loads three invented references (lantern gardens, clockwork clouds, the invented sea) for this run only, marked by the footer status "Sample references"; every desktop sample citation ends with ` [Sample]`. The website demo and help screenshots use the same set without the suffix. The practice task is identical on every surface: Search for “lantern”, choose the lantern gardens reference, then press Enter.

## Settings

[C18] Its own window (in the Dock and Alt+Tab while open), opened with **Settings…** (⌘,) from More, the menu bar or tray. A sidebar of pages runs under the window buttons, as in System Settings. Every change applies at once and reaches an open picker.

- **Window**: 920 × 560 by default, minimum 800 × 400. A two-column shell with a reserved `--titlebar-height`; the sidebar background runs full height. Nothing slides behind the title bar; only the page and the sidebar list scroll.
- **Pages** (in this order): **General** (opening shortcut, Open at Login, pasting), **Look** (Appearance, Accent, Rows, text size, Window size, Search icon, Show What Will Be Pasted), **Libraries**, **Citations** (default format, Forget formats for apps, Recent citations), **Shortcuts**, then after a gap **Advanced** and **About Ibis**. A page with no built controls stays out of the sidebar.
- **Only built settings appear**: no disabled promises or "Coming soon" rows, and nothing unbuilt in the search index.
- Each setting is one entry in one registry (key, page, group, label, description, control, default, search words); pages and search are drawn from it.
- **Groups and rows** [D17]: `ui/SettingsGroup` cards (`--r-card`, `--surface-grouped`) of inset rows: a label, an optional description, one control at the end. Cards keep their natural height; rows grow with text; controls move below a label that does not fit. No page opens with more than eight rows; rarer rows go behind **Details…**.
- A search field heads the sidebar, matching labels, descriptions and common words ("dark", "startup"); it opens the page and tints the row. "ibis" or "thoth" opens About.
- A page opens at its top with a `--type-display-*` heading; clicking the current page in the sidebar returns it to the top.
- Visual choices are rows of real pictures of the picker (sample references, current appearance), picked by clicking: Appearance (System, Light, Dark; System split diagonally), Rows, Search icon, Window size.
- **Accent**: the Mac's row of colour dots, **Follow the Mac** first (the default), Apple's hues in Apple's order, a divider, then **Ibis scarlet**. No free colour picker, custom fonts, themes or CSS.
- A failed save shows an alert and restores the saved choice; queued choices finish saving (at most two seconds) before the window closes. Preference resets offer Undo.

**Shortcuts.** Settings › Shortcuts has a search field, then grouped cards (Open Ibis, While searching, Your library, Anywhere in Ibis, In a menu). Only the opening-shortcut recorder edits a key; fixed keys are plain secondary text. General and Shortcuts share `ui/ShortcutSetting`. The Keyboard Shortcuts sheet (⌘/) is a readable overlay of two equal columns that hugs its content and closes with the square IconButton [C13].

- The recorder shows the shortcut in system notation; clicking says **Type a shortcut…**; Esc, Tab or leaving cancels.
- A shortcut needs Control, Option or Command; **⌘ + one key is refused**, as are picker commands and, on Windows, Ctrl+Alt with a printable key (AltGr). Refusals name the owner and keep the working shortcut.
- **Use Default** appears only for a different shortcut. On Wayland the desktop owns the shortcut: show its trigger text verbatim or **Not assigned**, with **Change…** where the portal supports it.

**Permissions.** A guide in Settings › General › Pasting says why in one sentence, names the system's list in its current words, shows a picture of the switch, opens the pane with one button, notices the change by itself and confirms in words. Mac auto-paste needs PostEvent and Accessibility; Settings says allowed only after both read back. The Ibis icon beside **Open System Settings** can be dragged into the list ("If Ibis is not in the list, drag it there."). Granting never replays a paste: return to the document, open Ibis again and press Enter.

**General.** **Open at Login** is off until turned on and read from the system, never `settings.json`. **Show in Menu Bar** is planned (on by default; off, opening Ibis from Applications opens Settings). The menu bar icon is a one-colour bird from `brand/` opening a menu: Open Ibis, the library line, Reset Window Position, Settings…, Check for Updates…, Quit Ibis.

**About.** A centred hero with the actual app icon, the name, a **Version x** button that copies it (⌥ shows and copies the full build), and the mission line; then Updates (automatic checks, Check now), links and the free-software note. It fits 920 × 560 without scrolling. Automatic update offers never show or focus the picker; they wait for empty, idle input, and **Later** is the default for every key (decision 0019).

## Source checks and passage reuse (planned)

Approved directions from ROADMAP.md; planned behaviour. No new notebook, source writes, cloud access or agent permission; MCP gains no notes, annotations or private links.

**Check the bibliography.** Does the exact key occur in one chosen read-only `.bib`? Name that file, never "the whole document". Kept separate from loaded search libraries; an unknown result is never "missing", and Ibis never silently swaps a key.

| Checked-file state | What the person sees |
| --- | --- |
| One exact occurrence | In thesis.bib; inspect the project entry |
| No occurrence after a complete scan | Missing from thesis.bib; copy an available entry or re-export |
| Unreadable, incomplete or uncertain | Couldn't check thesis.bib; retry or find the file |
| Several occurrences | Key appears more than once; inspect its locations |
| Conflicting fields | Key may name another reference; compare the entries |

- **Copy bibliography entry** keeps the key, never edits a source, and supplies a self-contained entry or explains how to export one.
- **Find a passage**: search Zotero highlights, notes and comments, labelled Highlight, Comment or Note; a note is never treated as a quotation from a guessed reference; image-only annotations show "No text quotation".
- **Open passage** opens a verified Zotero annotation, or says it is page-only. Physical and printed pages show apart ("PDF page 8 · cite p. 417"); an unknown printed page shows "Citation page unknown".
- **Copy quote with reference**: the stored text, printed page, reference and link as plain text or escaped Markdown; comments stay separate; nothing summarised or truncated.
- **Export evidence brief**: chosen passages in order, an optional question, one new Markdown file; local Zotero links labelled apart; no private paths.
- **Changed copied key**: "You previously copied oldKey; this item's current key is newKey", only for sources whose identity survives a rename (Zotero, CSL-JSON with a stable `id`).

## Small delights

The approved, final list; no new delights until the next design pass. Each lives only in About, the welcome, status lines or existing feedback; never changes results, output or focus; happens once within the motion cap; never loops or plays a sound. Warm, never childish: at most one short sentence, no slang, exclamation marks, emoji or jokes about the person's work. The fun items sit behind Settings › General › **Small surprises** (on by default).

- **Feel:** a pressed shortcut briefly presses its footer cap; holding ⌘ shows every visible action's key; Enter with nothing to paste shakes the action once and says why; ⌘+/⌘− show the size briefly; the menu bar ibis dips its beak after a copy.
- **Saves time:** Services › Search with Ibis; a `.bib` dropped on the menu bar ibis becomes a library; every Undo names its action ("Undo Remove Okafor & Lima, 2018"); ⌥ on About shows the full build.
- **Fun:** About's line on the name ("Named for the bird ancient Egyptians linked with Thoth, god of writing and keeper of records."); clicking the About icon blinks the ibis; milestones once each ("A good start.", "A solid library.", "A remarkable library.").
- **Left out:** confetti, sounds, seasonal themes, joke results, a "what's new" screen, alternative app icons.

## Per platform

- **macOS is the standard now.** Every system uses the macOS design: one code path, the same sizes, radii, controls and layout.
- **Windows and Linux** share the macOS look with their own UI font, modifier keys and key labels (Ctrl+K), selection colours where the system demands them, and window corners (the system's own: square on Windows 10).
- OS-specific designs for Windows and GNOME come later as their own value layers in `tokens.css` on the same components. Start from these recorded choices: Windows Acrylic with a footer band, Fluent radii (window 8, control 4), a deeper dark selected row (#0067C0, white text) and Fluent settings-card rows; GNOME opaque `window_bg`, libadwaita radii (window 15, row 7), AdwActionRow settings rows and menu checks after the label.

| | Rule |
| --- | --- |
| Material [C13] | One glass sheet on macOS; an opaque surface elsewhere. |
| Accent [C14] | The Accent chosen in Look; with Follow the Mac, the system accent where the webview reads it, else `--accent`. |
| Selected row [C4] | Accent fill with its contrast-safe paired `--selected-text`. |
| Switch | Measured from `NSSwitch`, the same everywhere. |
| Window corner [C15] | `--r-win` on macOS, matching the native panel mask; the system's own elsewhere. |
| Follows the system | Modifier keys and key-cap labels, the UI font, accessibility settings and window handling. |

## Brand and website

[C16] Logos and icons come only from `brand/` ([brand/README.md](../brand/README.md)). The website's rules live in [website/README.md](../website/README.md#design); its demo uses the app's tokens and key-cap labels at every width.

## Coming features

How each user-visible item planned before 1.0 in ROADMAP.md fits the components above.

| Feature (release) | Fits as |
|---|---|
| Zotero while closed (0.2) | Footer status with words and a dot; detail in the tooltip and More |
| Output per app, self-finishing setup (0.3) | First run above |
| Paste into the app you came from (0.3) | Enter pastes; the footer says "Paste into Word"; a refusal copies and says why |
| Selected text as the search (0.3) | The picker opens with the selection in search, all selected; Enter pastes over the selection |
| Citation forms, 29 locator types, prefix and suffix, reorder, any style, presets (0.4) | The shelf, the format pop-up and Settings › Citations |
| `ibis://` links, Show in Zotero, Open PDF in Zotero (0.4) | A link opens the picker filled in; the Zotero actions sit in More and the row's menu |
| Settings window (0.4, #776) | **Settings** |
| Browse, "Selected in Zotero", passages (0.5) | Collections and tags as search filters; a reference's highlights unfold under its row and fill in its page |
| Bibliography, "References used this session", project `.bib` (0.4) | More and the format pop-up; rows say "In refs.bib" or "Missing from refs.bib" |
| `ibis check`, DOI lookup (0.6) | Command line first; DOI lookup as an opt-in no-results fallback |

## How the design changes

- Change `tokens.css` or the shared component in `src/ui/`, never one screen; fix a visual bug in the rule or component that allowed it, with a regression test.
- The five parameters at the top of `tokens.css` (spacing unit, control radius, body text size, Ibis red light and dark) are the knobs. The canvas's Parameters board turns them live; `node apps/desktop/canvas/apply-params.mjs --radius=10px …` writes chosen values back (then `python3 brand/build.py` for the reds).
- The canvas boards run the real app live (`pnpm -C apps/desktop build:canvas`, then upload `ibis-canvas.js` and `.css` and point `Screen.dc.html` at them); a missing state gets a scenario in `apps/desktop/canvas/main.tsx`, never a hand-drawn copy.
- Record maintainer decisions in [decisions.md](decisions.md) and update the rule here in place, deleting the text it replaces.
- Review this page against Apple's guidelines on each new macOS release.
