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
10. **The bar for anything people see.** A beginner succeeds with no reading, in plain words, without opening Settings; a pro does everything by keyboard, with nothing waiting or confirming.
11. **One next action.** Every screen has one clear next action, and every failure says why and what to do.

## Where Ibis departs from macOS

Anything not listed follows Apple. A new departure is added here in the same change, or it is a bug.

| Where | Apple | Ibis |
| --- | --- | --- |
| Hit areas | Regular controls are 24 pt | Drawn at Apple's size, hit area at least 32 px (menu rows keep 24) |
| Smallest text | 10 and 11 pt captions | Nothing under 12 px |
| Accent | System accent | Follow the Mac by default, or **Ibis scarlet** |
| Citation colour | None | The scarlet beak marks the citation; its text stays in the text colours [C14] |
| Large buttons | Capsules on macOS 26 | Rounded rectangles at every size |
| Search icon | Magnifier | The Ibis mark, as Alfred shows its hat |
| In-window menus | `NSMenu` | Ibis's own, matching native metrics; menu bar, tray and right-click stay native |
| Window size | Content decides | One steady size while open [C10] |
| Key caps | Shortcuts only in menus | Caps in the footer and key lists; Settings writes keys as menus do |
| Focus ring | A translucent keyboard focus colour | Solid accent text, so it keeps 3:1 on every surface |

## The model

[M1] **One window that unfolds** (decision 0038). The picker is a launcher first:
type, see references, press Enter. Everything else unfolds toward the trailing
side of the same window, one region at a time, and folds back with Esc:

| Step | What the person does | What appears |
| --- | --- | --- |
| Search | Types | The list, the shelf with what Enter pastes, the footer's one action |
| Several references | ⇧↵, ⌘-click or a row's + | One shelf segment each, in order; the count names them |
| Details | Clicks a segment, or ⌘I | The **Inspector** beside the list: title, Show as, Page, what will be pasted, the source actions |
| Pages | **Show PDF** (⌘D) in the Inspector | The window takes a reading shape once, taller and wider; the list folds away and the **Reader**'s thumbnails and pages take its place beside the column |
| Choose a page | Clicks a thumbnail (⇧-click a range, ⌘-click another), or ⌘↵ on the page in view | The Page field and the shelf segment show it, "p. 417–419, 423" |
| Deliver | Enter, as the footer says | Pasted or copied, then the window hides |

- **Beginners** see every step as a visible, named control with its key beside
  it: the footer action, the segment, Show PDF, Cite This Page. **Experts** never
  leave the keyboard: ⇧↵ adds, ← → move between segments, ⌘I opens details, ⌘D
  the pages, ⌥↓ ⌥↑ move a page, ⌘↵ cites it, Enter delivers, Esc steps back.
- **Nothing moves under the person.** Unfolding never reorders search, list,
  shelf or footer. Reading is the one change of shape, because a portrait page
  needs height: the list folds away while the PDF is open, and typing in Search
  or Esc brings it back with the steady size.
- **Planned jobs have their place already.** Passages are results with their
  quotation under the title, and choosing one fills its reference and page;
  collections and tags are a search scope pop-up before the query (as Finder's
  search scope); a manuscript check is a sheet from More listing each citation
  with its state. None of these adds a window or a mode.

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

- **Labels** [D5]: `--text`, `--text-secondary`, `--text-tertiary` (resting glyphs, never sentences), `--text-disabled`; Apple's label ladder at AppKit alphas. `--text-placeholder` is AppKit's placeholder colour, which equals its secondary label.
- **Fills**: Apple's fill ladder `--fill-1` … `--fill-4`. Control rest, hover and pressed are tertiary, secondary and system fill (`--fill-control` and its states); quiet hover is quaternary. Grouped content (`--surface-grouped`, Settings cards) is tertiary (secondary in dark), a step above the sidebar, so groups separate by fill alone, as System Settings does (one edge per shape); Increase Contrast and forced colours add the edge.
- **Separators**: one `--separator` for hairlines and dividers; `--stroke-field` is the control edge that only Increase Contrast and forced colours draw. Never a coloured border on a control. **One edge per shape**: a border, an inset shadow or an overlay's shadow ring, never two; a divider belongs to one side.
- **Accent and selection** [D6]: there are two choices, **Follow the Mac** and **Ibis scarlet**, and each resolves to three roles with light and dark values: `--accent` (controls, switches, focus), `--selected` with `--selected-text` (selected rows, menu highlight, focused citation; text 4.5:1, never brighter in dark than light) and accent text on surfaces (4.5:1, also the progress ring). Follow the Mac reads the same roles from the system: on macOS `--selected` is WebKit's selection background, deeper than the accent and following it; accent text is the accent moved toward black or white only as far as 4.5:1 needs. Rings and marks with no text on them (a picture choice's ring, the file-drag ring) read `--accent-mark`, which equals `--accent` except on Windows and Linux in dark, where it is lifted to 3:1 while fills keep the system accent so white on them stays 4.5:1. The default button is `--accent` with `--text-on-accent`, or the deeper `--selected` where a Mac accent would fall below 4:1 with white; it keeps one text colour through hover and press. Hover and press darken toward black. Secondary text on a selection stays at full `--selected-text`; weight and size carry the hierarchy.
- **Focus** [D8]: `--ring`, solid accent text, only on `:focus-visible`; the system ink with Increase Contrast.
- **Status** [D10]: `--status-ok`, `--status-warning`, `--status-error`, the Mac's system hues. A dot or symbol, optionally on a wash; status words stay in a label colour. The dot agrees with the words beside it: while its menu or the footer reports a problem it is never green.
- **The one Ibis red** [D9]: `--scarlet` is the flat mark colour for the search-bar mark, the shelf beak, the added tile and the logo; it is never recoloured for a material. `--on-scarlet` is what sits on a scarlet fill. On a selected row the added tile becomes `--selected-text` with a scarlet check.
- **Ibis scarlet as accent:** it follows the rule of every macOS accent: the selection and filled controls are `--ibis` in both modes and carry white (5.4:1); the bright `--ibis-dark` stays for accent text, the ring and marks in dark (4.9:1 on the surface). Scarlet belongs to the bird, the shelf beak, the added tile and this one chosen accent, nowhere else.
- **Light and dark** come from one `color-scheme` on the root, in every window; `light-dark()` roles have no value without it.
- **Contrast:** text, placeholders and key-cap labels 4.5:1 against rendered pixels on every material and wallpaper (#774); focus rings, meaningful icons and only-cue boundaries 3:1. Apple's own label, placeholder and status values stay as measured where they fall short; Increase Contrast supplies the stronger set.

### Materials and layers

- **Surfaces** [D4]: `--surface` (window), `--surface-grouped` (grouped content), `--surface-overlay` (every overlay), `--surface-field`. Settings cards are grouped content. No other surface colours.
- **Window glass** [C13]: on macOS one glass sheet for the whole panel (Liquid Glass, popover vibrancy fallback) under a surface veil, so colour shows through but never readable shapes. Never glass on glass or CSS blur posing as the desktop. Elsewhere an opaque surface.
- **Readable overlays** [C13]: menus, the keys sheet and dialogs use the panel's glass, denser than the window, so content behind reads as soft colour only.
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
- **Rows** [C4]: two densities: Compact (one line; a long title ends in an ellipsis) and Comfortable (two, the default). Density changes rows only. Rows are 2 px apart in results.
- **One steady window** [C10]: the picker keeps one size the whole time it is open, in every state; nothing measures content to choose a height and nothing animates a resize. Medium 720 × 480 is the default, minimum 480 × 360; Look › **Window size** offers Small, Medium and Large. A dragged size shows as Custom and is kept per display.
- **Nothing clipped, one scroller**: no state is ever cut off; only the middle area or an open menu scrolls, never two at once. The inspector is the one exception: a sidebar scrolls apart from the list beside it, as in Preview. In a short frame with large text the header gaps and footer hint labels give way first.
- **Width classes** [C17]: below 28.75 em footer hints show keys only; at 60 em the preview sits beside the list, below it replaces it.

### Corners

- **Corners** [C15][D3]: rounded rectangles on one concentric chain. A shape near its container's edge keeps equal gaps and takes the container's radius minus that gap; every derived radius is computed in `tokens.css`, never typed. Window (`--r-win`, 16 on every macOS) → rows, fields, shelf (`--r-row`) → footer actions (`--r-control`) → key caps (`--r-key`); menus (`--r-menu`) → menu items (`--r-mi`).
- **Floors**: `--r-floor` for surfaces, `--r-floor-mark` for text-sized marks.
- **Neighbours share a radius**: shapes side by side at one depth use one radius and one gap.
- Overlays float on their own layer and keep `--r-menu`; Settings cards use `--r-card`.
- **Capsules** (`--r-full`) only for switches, count and tag pills and progress tracks. Nothing hangs off a shape's edge: a control that belongs to a shape sits inside it.
- The miniature pictures in Settings keep the picker's radii; see **Previews**.

### Icons

- macOS: SF Symbols and the system's own app and file icons where the system provides them, such as the installed Zotero icon (bundle id `org.zotero.zotero`, read through NSWorkspace).
- Fallback, and on other systems: Ibis's own set in `brand/icons/`, built by `python3 brand/build.py`; a 20 pt grid, 16 pt live area, one stroke weight matched to SF Symbols Regular, one stroked path each. The icon guard rejects anything else. A missing app or failed icon read shows the books glyph.
- Ibis never ships Zotero's logo or other apps' artwork.
- Seven type icons (article, book, chapter, thesis, web page, report, other) stay distinct from attachment drawings. Pages have rounded corners and a folded corner, as the book and web icons round theirs; the article page carries a title line and two text lines. Toggled-on keeps the same outline; the control's fill carries state.
- Settings page icons use Apple's icon shape: a white glyph from the set above on a `--page-*` tile, one family for every page. Mac icon layers are flat, with no added bevels or shadows.

### The bird

The bird appears only where Ibis itself is the subject, in the one treatment of that context, and a view holds at most one beyond the search bar's. It never labels a control, a page, a row or a setting, and never mirrors. In contrast modes every flat bird is `CanvasText`.

| Context | Treatment | Size |
| --- | --- | --- |
| Ibis as an app: About, the Dock, Finder, the paste guide's drag icon | The real app icon on its tile, never redrawn | `--mark-app` |
| Greeting: welcome, the first open | The depth mark, no tile | `--mark-greeting` |
| Waiting: opening a library | The flat mark, breathing | 2.5 rem |
| Working: the search bar | The flat mark in `--scarlet`, eye open | The search glyph's size |
| System chrome: the menu bar | The one-colour bird the system tints | The system's |

### Key caps

- One cap per shortcut, written as menus write it: ⌘K, ⇧⌘F, ↵ on macOS; Ctrl+K and Ctrl+Alt+Shift+K, modifiers in that order, on Windows and Linux (`ui/KeyCaps`). A pointer binding reads ⌘-click (Ctrl+click elsewhere).
- Caps appear only in the footer, the notice slot and key lists. Menus and Settings write shortcuts as plain text in native order (⇧⌘C), secondary like a menu's key equivalent [C8].
- A cap is `--fill-control` with `--r-key`, label type, never an outline or scarlet. Inside a quiet button it keeps its fill; on the primary it is the pressed fill. A trailing cap in a small button is concentric: `--key-gap` above, below and at the end. A one-symbol cap (↵) is square, and ↵ is lowered .1em so it sits optically centred.
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
- **Capitalisation:** on macOS, menu items, buttons, window, sheet titles and choice rows use title case ("Keep Window Open", "Try Again", "Find Them on This Computer"); everything else is sentence case. One string never appears in both styles; a sentence that names a command writes it as the command does ("Copy Only is set for this app").
- **Settings words:** a row label is sentence case, as System Settings writes it ("Window size", "Open at login", "Show in menu bar"); buttons and menu items are title case ("Back Up…", "Check for Updates"). A row's description is one or two sentences, each ending in a full stop. Shortcut rows describe an action in sentence case ("Move through results"), naming a sheet or command by its title ("Show Keyboard Shortcuts").
- **Punctuation:** typographic apostrophes and quotes (’ “ ”) and the ellipsis character (…), never three full stops, in everything a person reads; a test reads `messages.ts` and `en-US.ftl` for typewriter ones.
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
- **Focus ring** [D8] only on keyboard focus: outside on controls, inside on rows (results, choice rows, sidebar), so no scroll area clips it. It hugs what it names, never the space around a control.

| Component | Rule |
| --- | --- |
| Button (`ui/Button`) | Primary (accent, one per view), secondary (`--fill-control`) and quiet (no fill at rest, shared hover). Rounded rectangle, `--r-control`; `--action-h` large, regular in Settings, small (`--control-small-h`) in the shelf and footer; trailing key caps. |
| IconButton | Square, one size and glyph size per context, `--r-control`; rest transparent with secondary ink, toggled-on is the pressed fill with the same outline. Header ×, pin, sheet close, chip × and Settings search clear all use it. |
| TextField | A visible label above one single-line input; `--surface-field`, the shared control height and corner, and the same keyboard focus ring. Forms keep validation beside the field and own saving. |
| Pop-up button (`ui/PopupButton`, `ToolbarMenu`) | Shows its value and a chevron that never turns, nearer the edge than the label starts; Space, Enter or ↓ opens it; a pick or Esc returns to search. Named by what it sets ("Copy as Pandoc"). Quiet wherever its neighbours are quiet: in the search bar beside × and pin, and in a band that is itself a fill (the shelf's count); its chevrons say it opens, and its plate shows on hover and while open. |
| Menu (`ui/Menu`) | See **Menus**. |
| Segmented | `--r-seg` thumb on a quiet track. |
| Switch (`ui/Switch`) | The one on/off control: accent track when on, `--text-on-accent` thumb, size from `--switch-*`. |
| Search field | Frameless and large in the picker; a hairline field with no fill in Settings, glyph on the icon column. |
| Settings group and row (`ui/SettingsGroup`) | See **Settings**. |
| Shortcut table (`ui/ShortcutTable`) | Small secondary heading, one card of rows, action at the start and keys at the end. |
| Key caps (`ui/KeyCaps`) | See **Key caps**. |
| Shelf (`ShelfSegment`, `ChipGroup`, `LocatorToken`) | See **Shelf**. |
| Inspector (`ui/Inspector`, `CitationDetails`) | See **Inspector**. |
| Footer and status line (`LibraryStatus`, `SourceAction`) | See **Footer**. A status line is words, never a filled band. |
| Notice slot (`ui/NoticeSlot`, `UndoNotice`) and receipt (`CopyReceipt`) | See **Notices**. |
| Progress (`ui/Progress`) | A task such as an update is one line of the notice slot: a small ring where the status dot sits, then the title, phase and percent ("Updating Ibis", "Downloading · 3.1 of 7.4 MB", "42%"), so no row moves. An unknown phase turns the ring; Reduce Motion shows it dashed and still. Opening a library keeps a thin bar. |
| Choice cards (`ui/ChoiceList`) | See **First run and choice screens**. |
| Picture choice (`ui/PictureChoice`) | Equal columns, one per choice, captions on one baseline; never wrap three as two plus one. |
| Sidebar (`ui/SideNavigation`) | Single-selection list with `aria-current="page"`, the Mac's medium sidebar row, selected fill and paired text. `SideResults` draws search results as the same rows. |
| Scroll area (`ui/ScrollArea`) | The system's own scroll bars; the side inset sits inside the bar; `--scroll-fade` only at an edge with more beyond it. |
| File drag (`ui/FileDrag`) | A draggable file icon with button states. |

**Tables** size columns to content and never break or hyphenate a word. **No browser disclosure controls**: never `details`/`summary` or raw triangles; use always-open groups.

### Menus

- [C9] In-window menus (format, library, More, the reference count, Form) use `--r-menu`, `--menu-pad`, AppKit's item height, separators and the overlay material.
- A menu with any check keeps a leading check column for every row, so labels share one start and shortcuts line up at the end.
- **Menu heading**: small bold secondary type that never reads as an item.
- Sections keep one order: what it is about, what you can do, Ibis itself. Labels name the result; a quiet example of the output sits on a second line under the label, as macOS menu subtitles do, in full up to the menu's width, never cut to a column.
- An unavailable choice stays in the list, dimmed, with why in a few plain words on that second line ("Only when cited alone"); it never sets the menu's width and never needs two lines.
- **A pop-up anchors to itself** [C21]: a pop-up that fills a form row (Show as) opens its menu under its own leading edge, at least as wide as the button, as `NSPopUpButton` does (a menu wider than the room left moves back only as far as the window's inset needs); toolbar pop-ups keep `bottom end`.
- **A menu's reason is a heading**, one line, never a paragraph: the format menu says "For Word", "Default format", "For Thesis" or "This paste only".
- Menus over seven items get a filter field aligned with the labels. Header menus open `bottom end`, footer menus `top end`, flipping and shifting to stay within the window.
- Menus size to their labels up to the viewport and never wrap, as Mac menus; only at the window's edge does a label end in an ellipsis, its full name kept for screen readers. A scrolling menu ends on a whole row. Any pointer movement highlights an item.
- **More holds actions, not preferences**: Show in Zotero or Open thesis.bib (⌘O); Rows › (Compact, Comfortable); Keep Window Open; Keyboard Shortcuts (⌘/); Settings…; Quit Ibis. Each thing appears in one place, so More never repeats the library line the footer shows under it.

## Picker

### Search bar

- Order: search glyph, query, clear ×, format pop-up, pin. The header is its tallest control plus 2S, with a divider below [C2]. Its controls take the field's height, so the gap above and beside them is S and their corners nest in the window's (`--r-row`).
- `#tag`, `y:2019-2022` and `a:name` plus a space become removable chips; Backspace in an empty box removes the last.
- **Pin** [C6]: a neutral IconButton at the header's end; on, it shows the pressed fill, never the accent. Esc never unpins.
- The format pop-up names the exact output and, when the format belongs to something, whose it is (`APA · Word`, `APA · Thesis`); the default carries no suffix. One output has one name everywhere, the engine's (decision 0035). Hidden until a library is chosen.

### Results

- **List and row**: type tile, title, meta line, trailing year (or "No date") level with the title's first line in every density. Comfortable's meta line: authors as in a citation, then the container; Compact puts the authors after the title. One rule in both: one or two authors by surname ("García, Okafor"), three or more as "García et al.". The citation key is in the accessible name, not the row.
- **One record summary** (#1675): the row, `ibis search`, `ibis show` and agents read a record's title, authors, year and container from the engine's one summary, so a record shows the same year everywhere. The year is the date's year (also when a source writes it as text, "2016"), else the source's own date text cut to 20 characters ("circa spring 1890s"), else "No date". A year written as text is found by searching for it.
- **Chrome only where it acts**: a resting row shows its type as a glyph alone, with no plate; the plate appears where the tile does something (hover and selection: the add button, + or a check) or says something (a row in the citation: the scarlet tile with a check), without reflow.
- Titles wrap to two lines then end in an ellipsis; the byline stays one line. Opening details beside the list never changes a row's height; in a list narrower than 30 rem (details open, larger text) the byline keeps the authors and drops the journal, which Peek and the details show.
- **Peek** (⌘Y, as Quick Look by keyboard; Space types a space in the search): the highlighted row's full details in a card under it, with the menu's material: title, every author, where it was published, year, type and citation key. It never takes focus, so typing carries on; ⌘Y again, Esc, or a new highlight closes it.
- **Command search**: ">" first in the search lists the picker's commands as one-line rows with their keys, as Raycast does; words narrow them by title or description, ↑ ↓ move, Enter or a click runs one and clears the search, and the footer reads "Run <command> ↵". The library is not searched and nothing would be pasted. Each command's title, key and description come from the menus' words and `KEY_TABLE` (`features/picker/commands.ts`), so a command is written once.
- **Page with the search**: a trailing page typed after the words ("vale 417", "vale pp. 417-419") becomes the "p. 417" token beside the query and the highlighted reference's page; × on the token searches as typed.
- **Drag the citation**: the shelf's beak is the citation's proxy, as a Mac window's title icon is its document's: drag it into any app to drop exactly what Enter pastes (plain text, and rich text for a formatted style).
- **Hover** [C5] only tints a row under a moving pointer; a click selects, so reaching across rows for Copy never changes what is copied. Double-click copies.
- **List scrolling**: a clicked row stays under the pointer and only that row consumes the click, even if results re-render first. Arrow keys take back ownership.
- **Empty search** [C11]: a new library shows the hint ("Type to find a reference") centred in the middle area, with no keys. With history it shows **Cited here recently** (up to three, for the app Ibis came from) then **Recently added** (three), as ordinary rows navigated as one list.
- **Typing keeps the picker still**: rows, headings, shelf and footer stay until the next reply; nothing blanks. A click on a row from an earlier query does nothing.
- No matches: "No matching references", "Try a surname, year or fewer words.", then Clear Search. When something else is the obstacle the screen names it, with its one fix: a filter (**Remove Filter** keeps the plain words) or "Not in your libraries" for a pasted citation. Left-out Zotero libraries are where else to look, not the obstacle: the title stays "No matching references", the line says some Zotero libraries are left out, and the fix is **Search All Zotero Libraries**. A source failure and an empty library keep their own screens. When nothing matches every word, closest matches show under **Closest matches** and keep a "Closest match" pill on the shelf.

### Search rules

Built in `ibis-core` so every front end shares them (#1204).

- Any word order; case, accents and letter variants folded (`mueller` finds Müller); "et al." and "and" ignored; every word must match.
- Per word: whole word, then word start, then inside a word (three or more characters, or CJK), then one typo in words of five or more letters. Numbers, years, identifiers and tags are never guessed.
- Field hints: `a:` people, `t:` title, `j:` journal, `k:` key, `#` tag, `y:` year or range; quotes keep a phrase. An exact citation key always matches.
- Ranking: exact key, exact title, matches as typed, then typos, then abstracts; field weight; newest year; stable order.
- **Pasted lists** (#1326): keys in any supported citation syntax, or identifiers, in paste order under "Pasted list"; missing ones under "Not in your library: …". Up to 1,000 entries or 64 KiB; otherwise ordinary search.

### Shelf

- **Shelf** [C7]: one line above the footer showing what Enter pastes: the beak at the tile column's end, the citation on the title column. Shown while there are results or a citation is being built; hidden on choice screens. Results never move when a reference is added.
- Several references are segments of one citation, each with its page ("Vale 2021, p. 417"); a pointed segment shows its hover fill and grows at its trailing end to make room for a small ×, `--key-gap` from the edge on every side. Nothing ever covers text. A segment is the reference's own words: the brackets and separators the output adds are shared, quieter and never part of a segment, for one reference too, and a segment's fill hugs its words as a text selection does. ← → select a segment; Delete removes it with Undo; ↑ ↓ return to Search and move through the list, as they do there. The shelf itself is never a focus stop while it has segments, so no ring ever surrounds the whole shelf. Segment text starts exactly on the title column. A focused segment shows the ring every button has; the segment whose details are open keeps the pressed fill, as a toggled-on button does.
- **One line, details on demand**: a segment opens the **Inspector**. The count is a pop-up of every reference with its page; a pick opens that reference there.
- On copy a closing beak mirrors the opening one and a "Copied" capsule draws its check; Reduce Motion shows the end state. Each copy is confirmed once, in one place.
- Pending output dims, then "Formatting…"; Enter waits. A failed format says why and offers another style; refusals stay visible until corrected. Every preview ends in a result or a stated failure: an engine stop is asked again once (after the destination read, when one is running), and a second stop says the citation could not be prepared, so Copy is never left waiting on "Formatting…".
- The shelf keeps the last citation through a running search and when the next search finds nothing.

### Inspector

[C19] One reference's details are never an overlay. They open in a panel beside the list, as Preview's sidebar does, so one surface holds them now and a source's pages later.

- **Where**: in the middle row, on the trailing edge, `--inspector-width` wide, with the shelf running under both. Its divider runs the whole row it splits, as a Mac split view's does: it meets the search bar's divider at the top and the shelf at the bottom, and the content keeps S inside it (checked by `canvas/geometry.ts`). The window stays its one steady size. Search, list, shelf and footer stay where they are and stay live. Below 36 rem wide the panel takes the list's place instead.
- **Order**: the reference's full title (two lines, then an ellipsis) with ×, and its byline once, never repeated below; **Show as** and **Page** as label and control rows; what will be pasted, in full, as plain text with no box around it; a separator; **Show PDF ⌘D** (when the reference has one), the source action and **Remove from Citation ⌫**. The fields never move while typing; the column is the one scroller and ends with its last row whole.
- **Show as is a pop-up**, its value the current form, its menu the three forms with quiet examples. Only the engine says a form cannot be used, and only for the citation it was asked about; until it answers every form can be tried, so a refused citation can always be corrected here. Examples stay while the next arrive. Where no form is available, as in a reference-entry style, one quiet line replaces the pop-up.
- **One place for a failure**: while the panel is open, a citation refusal, a failed copy and every warning show in words in the panel, beside the fields that fix them, and not above the list. The shelf's warning and Details controls step aside, so nothing there comes and goes while typing.
- **Shelf and panel work together**: pressing a segment shows its details and focuses Page; pressing another moves them; pressing the same one closes them. A reference that leaves the shelf closes them.
- **Page** is empty when there is no page, with the placeholder "Add page", never "None"; it takes printed pages, ranges and lists ("417", "xii–xiv", "417–419, 423").
- **Keys**: Tab moves through it in order, never trapped. Enter in Page does what the footer says. Esc closes the Show as menu, then the panel, and focus returns to the segment if it was inside; Search keeps focus if it was there. Removing the reference closes the panel and returns to Search.
- **Warnings** use the same panel without a document.
- **A wider panel**: at 34 rem the column keeps its width on the trailing edge and the document sits beside it, scrolling alone.

### Reader (planned, #1252)

[C20] A read-only view of a reference's PDF that is already in Zotero or a `.bib` `file` field, for looking at a source and choosing its page without opening Zotero (decision 0036). Ibis never imports, downloads, edits or annotates a PDF; **Open in Zotero** stays beside it.

- **Where**: in the list's place, beside the Inspector column. **Show PDF** (⌘D, Ctrl+D elsewhere) is a row in the column under the fields, and reads **Hide PDF** while open. Opening gives the window a reading shape once, from its top edge and within the screen: as wide as thumbnails, one page at Fit Width and the column, and as tall as the screen allows up to 1.6 times the steady height. It is the one planned exception to [C10], with no animated resize. The list folds away and keeps its selection; search, shelf and footer stay where they were. Typing in Search, Esc or Hide PDF returns the list and the steady size; the details stay open.
- **Anatomy** [C20], as Preview: thumbnails in a sidebar at the reader's leading edge with its own full-height divider (⌥⌘2 hides and shows them); the pages on one `--surface-grouped` canvas edge to edge between the dividers, each a sheet with one `--separator` edge and `--inset` between them; and the page in view in a small floating bar centred over the bottom of the pages, in the overlay material, holding the page indicator and **Cite This Page ⌘↵** (and, for a page without a printed number, its one question). No toolbar row takes height from the page; zoom is ⌘+ ⌘− ⌘0 and find is ⌘F, as in Preview. With several PDFs a pop-up names each by its title, never a path, and the reader asks before showing one.
- **Status** (decision 0038): the shell is built against `features/reader/source.ts`'s `PageSource` (printed labels in PDF order, and a cancellable `draw`), with an invented book in the canvas's `reader` scenario and the tests; `pages.ts` is the one model of cited pages for the thumbnails, Page and the shelf. No source exists in the running app, so Show PDF does not appear there. Before it does, #1252 settles: the PDF.js adapter implementing `PageSource` behind the bytes command (decision 0036), with `/PageLabels` read through `citableLabel`; the worker CSP and no-live-worker tests in WKWebView and WebView2; the native resize to the reading shape and back; the reader's keys in `KEY_TABLE` with their handlers (⌘D, ⌘↵, ⇧⌘↵, ⌥⌘2, ⌥↑ ⌥↓) and an "In the reader" group in the keys sheet; keeping a confirmed offset by the file's content hash (today it lasts one opening); and the maintainer's answer on Liberation Sans.
- **Thumbnails** show each page small with its printed label under it ("417", "xii"); a page without a citable label shows "PDF 8" in secondary text, while printed numbers read in the text colour. The cited pages are the selected thumbnails, filled as selected rows are; the page in view has a quiet ring of `--separator`, never the accent.
- **Page indicator**: "p. 417 · PDF page 8 of 312", `tabular-nums`. An unknown label reads "PDF page 8 of 312 · no printed page"; a label printed on two pages adds "also PDF page 3". The tooltip and accessible description say where the printed page came from: the PDF's page labels, a Zotero annotation, or an offset you confirmed. It never shows the physical number as the citation page. A label that is not a citable page (decision 0036) shows in quotes with "no printed page"; one outside the record's pages shows both ("p. 12 · record pp. 413–440").
- **Choosing the page** (decision 0038): explicit, never by scrolling. A click on a thumbnail cites that page; ⇧-click extends to a range; ⌘-click adds or removes one page, as Preview selects pages. In the pages, **Cite This Page** (⌘↵) cites the page in view and **Cite Through This Page** (⇧⌘↵) extends the range to it. The Page field, the thumbnails and the shelf segment always show the same pages, and typing in Page selects the matching thumbnails. A page with no citable label asks once, in the column, "What number is printed on PDF page 8?", with the field and **Use for Nearby Pages**; the answer is an offset you confirmed and applies only to unlabelled pages next to it. A page outside the record's range is cited only after **Cite Anyway**. Choosing a page never copies or pastes; Enter still does what the footer says.
- **Several references**: the reader shows the PDF of the segment whose details are open; ← → on the shelf move to the next reference's PDF, each keeping its place for this opening.
- **Go to a page** (⌥⌘G, Ctrl+G elsewhere): a field in the toolbar. "417" goes to the page printed 417; with none it says "No page printed 417 in this PDF" and offers **Go to PDF Page 417** as a named choice; with several it lists them. It never jumps silently.
- **Zoom**: Fit Width by default, then Actual Size and 50 to 400 %. With focus in the pages ⌘+, ⌘− and ⌘0 (Ctrl elsewhere) zoom as in Preview; elsewhere they keep changing text size. Pages redraw sharp at the screen's scale; zoom lasts the session only and is never a setting.
- **Find in the PDF** (⌘F, Ctrl+F elsewhere, with focus in the reader): the toolbar becomes a search field with "3 of 12" and previous and next; ↵ and ⌘G (F3) go forward, ⇧↵ and ⇧⌘G (Shift+F3) back, Esc closes it. Matches are a quiet fill, the current one `--selected`; the count, not colour, says where you are. It searches the PDF's own text, page by page in the worker, stops when closed and never runs while hidden. A page without text says so ("This page is an image without text"), never "no matches".
- **Keys**: all by key, Tab in order (thumbnails, pages, toolbar, column). In the thumbnails ↑ ↓ move and Space or Enter cites, with ⇧ and ⌘ as for clicks. In the pages: ↑ ↓ scroll, Space and ⇧Space or Page Down and Page Up by a screen, ⌥↑ ⌥↓ by a page, Home and End to the first and last page; typing there never goes to search. On Windows and Linux no reader key uses Ctrl+Alt, which is AltGr. Esc steps back: find, Go to page, the reader (focus returns to Show PDF), then the panel. The keys sheet lists the reader's keys under their own heading.
- **Screen readers**: the pages are one region named by the reference's title, never the PDF's own /Title; each page is a group named by its indicator words, holding its text in reading order; the drawn page is hidden from them. Moving page by key announces the indicator once, politely; scrolling announces only where it settles.
- **Light, dark and contrast**: the paper is drawn as the PDF draws it, white in dark mode as in Preview; only the surround follows the theme. Increase Contrast adds `--stroke-field` to each sheet. Forced colours draw the page in `Canvas` and `CanvasText`, and text selection uses `Highlight`.
- **Motion**: page jumps and zoom are instant; nothing scrolls or zooms by animation, so Reduce Motion changes nothing here.
- **Right to left**: the panel and toolbar mirror; the pages, their text and the numbers in the indicator never do (each number is isolated).
- **Hidden and reopened**: hiding the picker closes the PDF and frees it. Reopening within a minute returns to the same page from memory; the place is never saved to disk.
- **Where it is unavailable**: on Linux until the web view sandbox is on, and where the web view's engine is too old (decision 0036), Show PDF stays, dimmed, with the reason in one quiet line; **Open in Zotero** still works.
- **When it cannot open**: the document area shows one sentence and one fix, centred, never a dialog; the column stays usable, so a page can still be typed. Nothing passes through the notice slot.

| State | Words | Fix |
| --- | --- | --- |
| No PDF | Show PDF is dimmed: "No PDF for this reference" | — |
| Missing file | Can't find this PDF | Show in Zotero, or Open thesis.bib |
| Not downloaded or in the cloud | This PDF isn't on this computer | Open in Zotero |
| Password-protected | This PDF is password-protected | Open in Zotero |
| Damaged | This PDF is damaged | Open in Zotero |
| Over 256 MiB | This PDF is too large to open in Ibis (412 MB) | Open in Zotero |
| Over 15 s | This PDF took too long to open | Try Again, or Open in Zotero |
| One page fails | That page shows "Couldn't draw this page"; the rest stay | — |
| An image over the limits | That page shows "Part of this page is too large to draw", never a blank page as its content | Open in Zotero |

### Footer

- **Footer** [C8]: status dot and text at the leading edge (dot on the tiles' column), the source action, the one primary action ("Paste into Word · APA ↵" or "Copy N as <format> ↵"), and last, on the rows' trailing edge, "More ⌘K".
- **More never moves**: it has the last column of its own, so nothing about the action moves it: not a longer label, not Alt, not the action being absent. A click aimed at More can never land on Copy. The action sits against it and grows toward the status. The leading slot always keeps room for its fix ("Find thesis.bib…"), so the fix never wraps onto the action; a failure line under them spans the footer.
- Every hint is label first, then keys, on one baseline. When the footer does not fit, shorten the status, then truncate the action's label with "…", its keys and full name kept, and only by the window's width drop hint labels to keys. The action's own width never decides that fold.
- **Quiet bands**: nothing in the shelf or footer is a filled button. The main action (Copy or Paste) reads as Raycast's: its words in the text colour at medium weight and the footer's one accent on its ↵ cap, so the eye rests on the results, not on the bottom edge. The library status, the source action, More, the reference count, notes and Details are quiet.
- Never show an action twice on one screen. With ⌥ held: "Add to citation ⇧↵", the Esc step, then More.
- **One place for each status**: library state lives only in the footer status; citation refusals and settings notices use the middle area; anything passing uses the notice slot (see **Notices**).
- **One reading of a library's health**: the dot, the hover text and the library menu all draw from `libraryCondition`. A problem in words is never a green dot: anything the menu reports makes the dot at least Look.

| Case | Footer |
| --- | --- |
| Starting, first run | "Checking library…", "No library chosen" |
| Loaded | "Zotero · 1,204 references" or "thesis.bib · 1,204 references" |
| Changed while open | "… · updated 2 minutes ago" (only when references changed) |
| Reloading | "… · reloading…", count kept; rows and the citation stay until it ends |
| Problem, old copy kept | Warning dot, never the stop red, since the old copy is still searched; "Zotero not updated · 2 h", then one fix button with its key ("Open Zotero ⌘O", "Find thesis.bib…") |

- Counts include bibliographic records only. Background work shows a small progress ring before the words.
- **Failed refresh with a last-good copy** (planned, maintainer, 2026-11-04): keep a coherent snapshot of the same established source searchable and citable. The persistent source status clearly says that refreshing failed and gives the last successful read time; it clears after a successful refresh. Enter keeps the action the footer promises, without a repeated confirmation or a forced switch from paste to copy. This does not permit a citation whose source identity is uncertain or whose selected work or key binding is known to have changed (decision 0028).
- The status opens the **library menu** (`top end`, like every footer menu): a heading with the library, its count and age, then a separator; with several Zotero profiles and Zotero not running, a **Zotero Profile** heading with one checked row each; with several Zotero libraries, a **Search in** heading, All Libraries, then one checked row per library; a library that could not load stays dimmed with "Couldn’t load"; then, after a separator, Choose Library… ⇧⌘O, Use Zotero for a file, and Reload Library.
- **Open the source** (⌘O): the words always name the target. While opening Zotero is the fix, ⌘O opens Zotero, so the fix button and More both read "Open Zotero ⌘O". **Show in Zotero** selects the reference in Zotero (starting it if closed); **Open thesis.bib** opens the file; **Open Zotero** only when Zotero is the source and not running. ⌥ turns "Open thesis.bib" into "Show in Finder". If the source cannot open, offer Choose Library… or Find….

### Shared project edits (planned)

Approved 2026-11-04, decision 0028: a saved project is shared, while each ongoing
writing task keeps its established citation choices.

- An edit made for the current task applies after it saves, regenerates and revalidates the preview, and cancels queued insertion. Saving never pastes automatically.
- A citation-affecting edit elsewhere shows one persistent, nonmodal notice: "Thesis settings changed elsewhere. Using APA · thesis.bib · Apply Updates". Enter continues to use the shown task choices and preview. No repeated prompt, automatic switch between citations, or visible version-management interface. Completing one citation or hiding the picker does not itself end the writing task; a deliberately started new task uses the latest saved project.
- **Apply Updates** keeps valid picks, pages, prefixes and suffixes, shows the changed format or bibliography, and prepares a fresh preview. It requires a fresh insertion action and never replays a queued Enter. A running agent adopts updates for its whole affected output, not halfway through a citation list.
- A cosmetic rename updates the displayed project name without changing its identity or citation choices. A project removed elsewhere leaves the ongoing task's settings available and clearly labelled as retained settings; it never silently selects a different project or app defaults.
- Changing the project's selected bibliography or source scope is a pending context edit. New contents in the already bound bibliography or sources are factual updates: refresh and validate them normally, apply the unreadability policy below, and refuse newly conflicting keys. Explicit source disconnection or access revocation stops affected work.
- Failed or conflicting saves keep current task choices intact. Show the other writer's change before overwriting the shared definition; preserve valid user input.

### Project search defaults (planned)

Approved 2026-11-04, decision 0028, under #1260: a writing project may optionally
save which libraries or collections a new task searches. Projects without a
saved scope start with **All Connected Libraries**.

- Saving a project search default is explicit. A temporary task filter stays local and never silently changes that default.
- New tasks start with the project's saved scope; ongoing tasks keep their applied choices and adopt external material edits through **Apply Updates**.
- Collection membership filters discovery. The selected bibliography remains the separate authority for checking emitted keys; a collection does not establish what the document compiles against.

### Historical citations (planned)

Approved 2026-11-04, decision 0028: distinguish recovering saved output,
recreating a historical rendering, and citing with current information.

- Plain citation text already pasted stays as inserted. A corrected record supplies new citations; a change affecting a prepared citation requires review before insertion, never a silent replacement under Enter.
- A saved result is dated and shows the text actually retained. Where a current record differs, the historical view can show "Saved on November 4: (Vale, 2023)" and "Current record: publication year 2024", with **Copy Saved Text** and **Create Current Citation** as distinct actions. Do not interrupt ordinary searching with notices for every metadata change.
- **Recreate the Earlier Citation** requires the necessary old records, request and formatting inputs. If unavailable, say, for example, "The original formatting rules weren’t saved. Ibis can create a citation using current rules." Never label current rendering an exact reproduction. Exact saved text and markup do not guarantee the same destination layout.
- Updated formatting rules can improve new citations without rewriting old text. A generation job uses a consistent set of rules; inability to resume with those rules is explained before continuing.
- A receipt records what was used and checked, including relevant formatting versions; it does not restore material never retained. Historical key text alone does not preserve what a document resolves it to. Reusing a key in a current project still enforces current key-to-work validation and known-conflict refusal.
- Durable preservation is an explicit project export of the relevant references, citation requests, rendered outputs and formatting inputs. Do not silently archive old libraries, documents or attachments, or imply that recents and Change Last Citation are permanent history. Saved historical output remains distinct from current verification.

### Keys

`KEY_TABLE` in [`apps/desktop/src/keys.ts`](../apps/desktop/src/keys.ts) is the one source of shortcuts; the keys sheet, footer and More read it. A key is added only with the code that handles it.

| Input | Does |
| --- | --- |
| ⇧⌘Y (Ctrl+Shift+Y elsewhere) | Opens Ibis from any app (the default; follows the active layout's letter) |
| Click / double-click a row | Selects [C5] / pastes or copies what the shelf shows |
| Enter, or the footer action | Pastes or copies what the shelf shows; success hides an unpinned window |
| ⌘-click, the tile’s +, ⇧Enter | Adds to the citation, in order |
| ← → / ⌫ or ⌦ on a shelf segment | Moves between the citation’s references / removes the focused one, with Undo |
| Esc | Steps back one layer, from any control: sheet, menu, inspector, library chooser, then clears search (the shelf stays), then hides. A held Esc takes one step |
| ⌘O / ⇧⌘O | Opens the source / Choose Library… |
| ⌘K | More |
| ⌘/, or `?` outside a field | Keyboard shortcuts |
| ⇧⌘P | Keep window open |
| ⇧⌘F | The format chooser |
| ⌘Y | Peek at the highlighted reference |
| `>` first in the search | Command search |
| ⌘E | Change the last citation for this app |
| ⌘+ / ⌘− | Text size |
| ⌘Z | Undo what the notice slot offers |
| Typing outside a field | Types into search, except in the reader's pages |

### Copying and pasting

- Cancelled work is not a failure and never reaches the person: a preview stopped by a newer choice or a reload says nothing. A preview that fails belongs to the opening that saw it: reopening or choosing the citation again prepares it afresh, and the failure is gone from the first frame.
- Enter does what the footer says. Pressed while results or the preview load, it waits and runs once. Repeated activations on the same citation never copy twice; a changed query, reference, page or format, arrows, Esc or hiding cancel a waiting Enter. A stale reply never replaces newer shelf or feedback.
- **One format rule** (decision 0031): a selected writing project's format takes precedence; otherwise Ibis remembers a format per app. You change either in the format menu, which names where the pick goes. An app with none starts from the table's suggestion (Word uses your default style, Obsidian Pandoc) or else your **Default format** in Settings › Citations. Without a project, when Ibis can't tell the app the default applies and a pick changes it, so no choice is lost. There is no one-time override and no reset; a later pick replaces the old one.
- **The format control always says why.** Closed, it reads "APA · Word" for a format belonging to the app, plain "APA" for the default, or "APA · This paste" for a format that could not be saved or that a permission return kept. Opened, its one-line heading says whose the pick is ("For Word", "Default format", "This paste only"); the full sentence is the menu's accessible description.
- **Writing projects** (maintainer, 2026-11-04, #1697): optional named choices keep one format and an optional bibliography together. The selected project is remembered **per app**: Thesis in a browser does not become the project for Word. The closed format control names it, "BibLaTeX brackets · Thesis"; its menu says "Saved for Thesis. Your pick changes this project's format." **Writing Projects…** opens a sheet with the projects, **No Writing Project** and **New Writing Project…**. The list is shared across apps; selecting a project in another app is explicit. The sheet says edits are shared; an ongoing task in another app offers Apply Updates while keeping its established choices. An unidentified app uses a session-only selection and says so. Ibis never infers a project from a window title, URL or document contents.
- **The writing-project sheet** uses the shared sheet geometry with one scrolling body. Creating starts with the current format, asks for a name, selects the project after it is saved and leaves the sheet open to choose a bibliography. The selected project offers **Choose Bibliography…**, **Remove Bibliography**, **Rename Project…** and **Remove Project…**; removal asks once in place and removes only the Ibis choice, never its file. Selecting an existing project or **No Writing Project** saves before returning to search. A failed save leaves the earlier project, format and bibliography together. A project without a bibliography inherits none; **No Writing Project** restores the app/default output and the standalone bibliography check. Switching keeps the search, references and pages, retires any earlier Enter or Change Last Citation when the change starts, and prepares the citation afresh. A fresh Enter after that boundary waits for the new context and preview; finishing an opening or a bibliography read never cancels it. Explicit editor requests keep their requested output; project changes wait until that request finishes or is cancelled. Selecting a project grants no paste permission.
- **First paste in an app**: without a selected writing project, a browser, a code editor or an unrecognised app opens the format chooser once; the pick is remembered for that app. In a browser or code editor, answering Paste or Copy only in that question accepts the format it shows. An unrecognised app also asks **Paste into [app name] from Now On** or **Copy Only** even with a project selected; answering never replays the held Enter.
- Chooser items read as plain names with a quiet example ("BibLaTeX brackets   \parencite{key}").
- Refused apps (terminals, remote viewers, virtual machines) and targets Ibis cannot prove only copy, with the reason. A permission detour keeps the prepared citation and waits for a fresh Enter; newly selected text still opens as the search, with the shelf kept, because Enter would replace it.
- **Selected text as the search** (macOS): short text on one line, selected in the target app, opens as the search, all selected, so typing replaces it; Enter pastes over it. It lasts one opening: it never returns as a recent search unless edited. Long or multi-line selections, secure typing, password fields, copy-only, refused or unapproved apps and failed reads open an empty search and change nothing. **Ibis replaces only text the person saw**: with a long selection the footer offers Copy, and any selection that changed, or was never read, stays as it is, with the reason. Enter still pastes at the caret when nothing is selected.
- **Change Last Citation** (More, ⌘E): reopens the last citation for this app on the shelf, with its pages and options; the footer says "Change Citation". Enter replaces text only when the old citation is selected in the document, proved just before the paste; otherwise it copies the changed citation and says so. Ibis never deletes text it cannot see. The last citation lives in memory only, for this run of Ibis and at most eight apps. Forget Recent Citations clears it, and the shortcut that opens Ibis never reopens it: hiding Ibis abandons the change.
- A successful copy hides Ibis, returns focus to the previous window and shows the notice. Pinned, Ibis stays.

### Notices

- "Pasted" or "Copied" shows after the picker hides, in a window that never takes focus: one card near the top centre on every system, ignoring the pointer. What was copied is the hero: the citation in the text colour at body size, monospaced. Above it a quiet secondary line says what happened ("Copied", "Pasted into Word" or the reason). A green check in a soft green circle, centred on both lines, confirms success. A long reason wraps and is never clipped; the citation stays one line, ending in "…" with "+4 more" in view. The notice follows the chosen appearance.
- **Plugin note**: the first paste into a word processor (Word, LibreOffice, Pages) adds a line under the citation and stays nine seconds: if this document uses a reference manager's plugin, insert with the plugin. Once per app, remembered in the per-app file, reset by Clear. It depends only on the app ID; Ibis cannot see the document, and browsers are skipped because Google Docs looks like any page.
- No notice carries a side bar or a one-sided border. Errors are one sentence beside their cause, never a modal.
- **One notice slot per window** (`ui/NoticeSlot`): notices never float. Everything that passes takes the window's status area: in the picker the footer's left side, standing in for the library status, which returns after 8 s; in Settings one line at the foot of the content area. The slot is in the layout, so it never covers content, never moves anything and opens no layer: it can sit behind any open menu or panel. One notice shows: the newest Undo offer, which is also what ⌘Z acts on, then a remark, then a running update. A notice's time runs only while it shows, so one that waits returns with the rest of its time and none passes unseen, including a failed update.
- A notice's words start exactly where the status words start, from one shared measure (`--status-text-start`), whatever the dot or the button around it.
- A notice is one line ending in an ellipsis, with short names ("Removed Vale 2021", first author and year as on the tile). An Undo offer has a quiet **Undo ⌘Z** right beside its words; it lasts 8 s, pauses on hover and focus, and ⌘Z does the same while it shows. A remark needing nothing (tip, milestone, the size after ⌘+, why Enter did nothing) is words alone, and leaves when you type. Screen readers hear both through the status role.

Every "copied, not pasted" sentence ends with its fix, with full stops, and the fix is usually "Press ⌘V to paste." (Ctrl+V on Windows and Linux). These are the exact words; a test compares them with `copiedNoticeText`.

| Reason | Code | Words |
| --- | --- | --- |
| No reason given | — | Copied. Press ⌘V to paste. |
| Pasting off | `paste-off` | Copied. Press ⌘V to paste. |
| Paste check unavailable | `paste-unavailable` | Copied. Press ⌘V to paste. |
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
| Windows keys stuck | `key-release-failed` | Copied. Press and release Ctrl and V to paste. |
| Selected text never shown | `selection-not-shown` | Copied. Ibis replaces only text it showed you. Press ⌘V to replace it. |
| Old citation not selected | `citation-not-selected` | Copied. Ibis replaces a citation only when it is selected. Select it, then press ⌘V. |

Never call the destination a password field. The right paste key is shown per system.

### Window behaviour

- **Opening**: on the focused window's display, at its last place there (first time: centred, top in the upper third), with a short fade and focus in search. A search from under a minute ago returns selected, unless Esc or Clear cleared it: an explicit clear sticks. Text typed while opening is kept. The window is current in its first frame.
- **Dropping a file**: one `.bib`, `.biblatex` or CSL-JSON file dropped on the picker becomes the library, and the footer shows its name and count. Anything else leaves the library alone and says what Ibis accepts until the next good drop or opening.
- **Moving and resizing**: drag the empty header; snap to centre, upper third and corners. Settings › Advanced › **Where Ibis opens** offers Centre and the four corners; **Reset Window Position** restores the default.
- **Other windows**: a command that hands the screen to another window has one effect however it was reached, and Esc then belongs to that window. Settings always closes the picker, even a pinned one, which would otherwise float above it; the pin stays for the next opening. An opened source hides an unpinned picker; pinned, it stays beside. A file chooser belongs to the picker, which stays behind it.
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
- **Welcome**: the whole window, with no search header. The mark, a title, the line "Ibis finds a reference and pastes its citation where you're typing. It only reads your library, never changes it.", then four starts as a 2 × 2 grid of tiles (Zotero, a .bib file, Find Them on This Computer, Try Sample References). Every tile is the same height and puts its glyph in the same place, level with its title; a fact such as "Found on this Mac" replaces the help line rather than adding one. The note names the registered shortcut ("After this, open Ibis from any app with ⇧⌘Y."), or says **No shortcut is set** with how to open Ibis instead. Esc dismisses a menu, then hides.
- **Opening a library**: nothing for 400 ms, then the mark, "Opening example.bib…", "Reading your library. Zotero can stay closed." and a thin progress bar. Rows kept from before stay instead.
- **No library**: "Where do your references live?" with the welcome grid; nothing is searched until Find is chosen.
- **Zotero found**: Ibis still asks. When Zotero is running or its library is on this computer, the welcome screen keeps every start and highlights Zotero, so Enter takes it. Only that look happens first; no library is read until it is chosen.
- **Empty library** (a successful load with zero references): "No references in thesis.bib", "Choose a library with references.", then **Use Zotero**, **Choose a Library File…**, and for a file **Open <file> ⌘O**; below, "Or drag a .bib or CSL-JSON file into this window."
- **Could not open, nothing kept**: the title names the library and the fix, and the line under it says why in the backend’s words, never the title again; **Use Another Library** shows the no-library screen. Every row has a help line, the source row too ("Ibis carries on by itself once it’s open", "See the file in its own app"). A failure with references kept stays a notice above them.

| Problem | Title | Rows |
| --- | --- | --- |
| `missing` | Can’t find thesis.bib | Find thesis.bib, Use Another Library |
| `unreadable`, `other` | Can’t read thesis.bib / Can’t open thesis.bib | Try Again, Use Another Library |
| `zotero_closed` | Open Zotero to read your library ("Ibis usually reads it with Zotero closed, but not this time.") | Open Zotero (accented), Use Another Library |
| `zotero_missing` | Ibis can’t find Zotero’s library | Use Another Library, Try Again |
| `zotero_access_off` | Zotero isn’t sharing your library | Open Zotero, Use Another Library |
| `zotero_profiles` | Which Zotero profile? | One row per profile, Open Zotero, Use Another Library |

- Setup finishes itself: Ibis never asks "What do you write in?"; with Zotero closed Ibis reads the library anyway. A screen waiting on Zotero says where the fix is and asks Zotero every two seconds while it is shown, never while the picker is hidden, so it carries on by itself as soon as Zotero answers. With several profiles the closed path asks once, by the profile's name, and only a closed read that used it remembers the answer; a saved profile that is gone or unreadable asks again and never reads another. With references on screen the choice stays out of the way: the footer offers **Choose Zotero Profile…**, and the library menu lists the profiles whenever there are several and Zotero is not running, so a mistaken choice can be changed. A running Zotero answers for the profile it opened, so it offers no choice.
- **Try Sample References** loads three invented references (lantern gardens, clockwork clouds, the invented sea) for this run only, marked by the footer status "Sample references"; every desktop sample citation ends with ` [Sample]`. The website demo and help screenshots use the same set without the suffix. The practice task is identical on every surface: Search for “lantern”, choose the lantern gardens reference, then press Enter.

## Settings

[C18] Its own window (in the Dock and Alt+Tab while open), opened with **Settings…** (⌘,) from More, the menu bar or tray. A sidebar of pages runs under the window buttons, as in System Settings. Every change applies at once and reaches an open picker.

- **Window**: 920 × 560 by default, minimum 800 × 400. A two-column shell with a reserved `--titlebar-height`; the sidebar background runs full height. Nothing slides behind the title bar; only the page and the sidebar list scroll.
- **Pages** (in this order): **General** (opening shortcut, Open at Login, pasting), **Look** (Appearance, Accent, Rows, Window size, text size), **Libraries**, **Citations** (default format, Recent citations), **Shortcuts**, then after a gap **Advanced** and **About Ibis**. A page with no built controls stays out of the sidebar.
- **Only built settings appear**: no disabled promises or "Coming soon" rows, and nothing unbuilt in the search index.
- Each setting is one entry in one registry (key, page, group, label, description, control, default, search words); pages and search are drawn from it.
- **Groups and rows** [D17]: `ui/SettingsGroup` cards (`--r-card`, `--surface-grouped`) of inset rows: a label, an optional description, one control at the end. Cards keep their natural height; rows grow with text; controls move below a label that does not fit. No page opens with more than eight rows; rarer rows go behind **Details…**.
- A search field heads the sidebar, matching labels, descriptions and common words ("dark", "startup"). **Results are sidebar rows** (`SideResults`), as in System Settings: the page's tile, then the setting's label, starting at one edge and wrapping there; never buttons or pills. A click, or Enter for the first, opens the page and tints the row. "ibis" or "thoth" opens About.
- A page opens at its top with a `--type-display-*` heading that stays put while the page scrolls under it, as System Settings keeps its title in the toolbar; a hairline appears under it once anything has scrolled beneath. Clicking the current page in the sidebar returns it to the top.
- Visual choices are rows of real pictures of the picker (sample references, current appearance), picked by clicking: Appearance (System, Light, Dark; System split diagonally), Rows, Window size.
- **Previews** (`settings/PickerPicture`, `ui/PictureChoice`): a preview is the real part, and its frame comes from what it shows, so nothing is cut and no band is left empty. A window has a set shape and is that shape scaled (`--preview-window`). A Rows strip shows two whole rows at `--preview-rows`, in a frame that hugs them at every density; a picture is never framed twice, so a picture choice inside a card draws the picture and its ring, never a second box.
- **Accent**: a segmented choice of **Follow the Mac** (the default) and **Ibis scarlet**. No other hues, free colour picker, custom fonts, themes or CSS.
- A failed save shows an alert and restores the saved choice; queued choices finish saving (at most two seconds) before the window closes. Preference resets offer Undo.

**Shortcuts.** Settings › Shortcuts has a search field, then grouped cards in one order (`SHORTCUT_GROUPS`: Open Ibis, While searching, In the citation, Your library, Anywhere in Ibis, In a menu); the keys sheet shows the same groups, named the same, in the same order, less In a menu, which every menu shares. Open Ibis holds one row, "From any app", with the opening shortcut. Only the opening-shortcut recorder edits a key; fixed keys are plain secondary text at body size, as menus write key equivalents, in full ink under Increase Contrast. Group headings scroll with their cards, never sticking over a card's edge. Keys and the "or" between them share one baseline with equal gaps. General and Shortcuts share `ui/ShortcutSetting`. The Keyboard Shortcuts sheet (⌘/) is a readable overlay of two equal columns that hugs its content and closes with the square IconButton [C13].

- The recorder shows the shortcut in system notation; clicking says **Type a shortcut…**; Esc, Tab or leaving cancels.
- A shortcut needs Control, Option or Command; **⌘ + one key is refused**, as are picker commands and, on Windows, Ctrl+Alt with a printable key (AltGr). Refusals name the owner and keep the working shortcut.
- **Use Default** appears only for a different shortcut. On Wayland the desktop owns the shortcut: show its trigger text verbatim or **Not assigned**, with **Change…** where the portal supports it.

**Permissions.** A guide in Settings › General › Pasting says why in one sentence, names the system's list in its current words, shows a picture of the switch, opens the pane with one button, notices the change by itself and confirms in words. Mac auto-paste needs PostEvent and Accessibility; Settings says allowed only after both read back. The Ibis icon beside **Open System Settings** can be dragged into the list ("If Ibis is not in the list, drag it there."). Granting never replays a paste: return to the document, open Ibis again and press Enter.

**Citations › Editor endpoint.** A switch, off by default. On, it shows the address to paste into the editor's Cite as You Write URL setting, with **Copy Address**, and says who can use it: editors on this computer, never web pages. While an editor waits, the picker's notices say "Citing for your editor in Pandoc" and the preview is in that format; Enter sends it, Esc answers with nothing. The format menu, paste and the app question step aside for that one citation.

**General.** **Open at Login** is off until turned on and read from the system, never `settings.json`. **Small surprises** closes the same card. **Show in menu bar** is on by default; while it is off, opening Ibis from Applications opens Settings. The menu bar icon is a one-colour bird from `brand/` opening a menu: Open Ibis, the library line, Reset Window Position, Settings…, Check for Updates…, Quit Ibis.

**Another computer.** Settings › General ends with **Back up settings** (Back Up…, a save dialog that starts in iCloud Drive on macOS, OneDrive on Windows, otherwise Documents) and **Restore settings** (Restore…). A chosen backup is checked whole, then the row lists every change as "Setting: before → after" with notes (a bibliography not on this computer, a shortcut kept for this system), **Cancel** and the primary **Restore and Restart**. Nothing changes before that button; a bibliography shows by its file name only. Refusals say why in one line and change nothing ([moving to a new computer](moving-to-a-new-computer.md)).

**About.** A centred hero with the actual app icon, the name, a **Version x** button that copies it (⌥ shows and copies the full build), the mission line and the line on the name, which keeps the room of two lines so a new fact never moves the page; then Updates (one row: the automatic-checks switch), links and the free-software note. **Check for Updates** is a button under the version, as Mac apps place it, and its result replaces the line under it. The mission line is the hero's lead, in the text colour; the line on the name is secondary. The icon is the page's only bird. It fits 920 × 560 without scrolling. **Check for Updates…** in the tray opens About and runs Check for Updates, so its result shows there, never in an alert. Automatic update offers never show or focus the picker; they wait for empty, idle input, and **Later** is the default for every key (decision 0019).

## Source checks and passage reuse (planned)

Approved directions from ROADMAP.md; planned behaviour. No new notebook, source writes, cloud access or agent permission; MCP gains no notes, annotations or private links.

**Check the bibliography.** Does the exact key occur in one chosen read-only `.bib`? Name that file, never "the whole document". Kept separate from loaded search libraries; an unknown result is never "missing", and Ibis never silently swaps a key.

With a writing project selected, choosing or removing this bibliography changes that project. With none selected it changes the standalone check. A project with no bibliography never borrows the standalone check or a previous project's file.

| Checked-file state | What the person sees |
| --- | --- |
| One exact occurrence | In thesis.bib; inspect the project entry |
| No occurrence after a complete scan | Missing from thesis.bib; copy an available entry or re-export |
| Unreadable, incomplete or uncertain | Couldn't check thesis.bib; retry or find the file |
| Several occurrences | Key appears more than once; inspect its locations |
| Conflicting fields | Key may name another reference; compare the entries |
| Key is known to identify a different work | Refuse this key citation until the conflict is resolved |

Built for key outputs: **More › Check Against a Project Bibliography…** chooses the file, and each result row then says its state in words, naming the file ("In thesis.bib", or with "(last good copy)" when the newest read failed or was damaged), with the lines to look at for a repeated or doubtful key. The file is read when chosen, at launch and when the picker opens; nothing polls. More names the file, what is wrong with it if anything, and offers **Stop Checking thesis.bib**. A key known to identify a different work is refused: Enter neither copies nor pastes that key, keeps the selection and explains the conflict. This rule replaces warn-and-copy for that case (maintainer, 2026-11-04, decision 0028). Other states outside "In" join the citation's warnings, in the preview and in the copy's own result, so Enter copies, says why and stays open, except for the unreadable-bibliography policy below: retained verified evidence or an explicit one-off unverified action keeps the intended delivery available, with the warning also shown in its receipt. Copy Entry says the entry comes as the file has it, `file` and `note` fields included. Formatted citations show no marks: a style has no key to check.

**Unreadable project bibliography** (maintainer, 2026-11-04; decision 0028):

- If a retained complete, coherent copy of the same selected bibliography supports the key-to-work match, normal Enter, copy and paste remain available. A persistent status says "Couldn’t read thesis.bib. Using its last checked copy from 10:42", with Retry. No repeated confirmation or forced switch to copy; a newly selected reference may also be checked against that copy.
- If no usable copy exists or it cannot establish the match, pause normal insertion with "Couldn’t check this citation against thesis.bib" and Retry or Locate Bibliography…. Keep the picks, pages, output and preview; never silently detach the bibliography, change projects or substitute formatted text. An explicit one-off **Paste Unverified Key** or **Copy Unverified Key** is available for the intended action, reported as unverified and never saved as a hidden permanent preference. It cannot bypass a known wrong-work conflict or source-identity refusal; agents do not silently choose it to escape a refusal.
- Say "Not found in the last checked copy" for an old absence, never "Missing" from the current file. New coherent evidence supersedes old evidence; never fall back behind a detected conflict. Project or bibliography changes cannot reuse another selection's check.
- Strict freshness requires a successful current check in every client. Successful Retry updates the evidence and preview but never replays a stopped insertion; a fresh action is required. A current local check still makes no promise about which bibliography the actual document or remote project uses.

- **Copy bibliography entry** (More, on a missing row) keeps the key, never edits a source, and supplies a self-contained entry or explains how to export one.
- **Find a passage**: search Zotero highlights, notes and comments, labelled Highlight, Comment or Note; a note is never treated as a quotation from a guessed reference; image-only annotations show "No text quotation".
- **Open passage** opens a verified Zotero annotation, or says it is page-only. Physical and printed pages show apart ("PDF page 8 · cite p. 417"); an unknown printed page shows "Citation page unknown".
- **Copy quote with reference**: the stored text, printed page, reference and link as plain text or escaped Markdown; comments stay separate; nothing summarised or truncated.
- **Export evidence brief**: chosen passages in order, an optional question, one new Markdown file; local Zotero links labelled apart; no private paths.
- **Changed copied key**: "You previously copied oldKey; this item's current key is newKey", only for sources whose identity survives a rename (Zotero, CSL-JSON with a stable `id`).

## References used this session

**More › References Used This Session…** opens a sheet, named for what Ibis knows: what it copied since it opened or since a reset, never the document's bibliography. Its lead explicitly says **across all projects this session**; choosing a project neither partitions nor clears the list. It lists each reference once, in the order first cited, with Remove; **Undo** names its action ("Undo Remove Okafor & Lima, 2018"), and undoing keeps anything copied since. **Reset List** is undoable too. Under the list is the bibliography in the output's style (APA when the output is keys), exactly what **Copy Bibliography** writes as rich and plain text together. **Save…** writes it as rich text, a web page or plain text. A reference from a library that isn't open is left out and counted, never guessed. The list holds record identities, in memory only, and leaves it only through Save.

## Small delights

The approved, final list; no new delights until the next design pass. Each lives only in About, the welcome, the notice slot or existing feedback; never changes results, output or focus; happens once within the motion cap; never loops or plays a sound. Warm, never childish: at most one short sentence, no slang, exclamation marks, emoji or jokes about the person's work. A delight is a person's action answered, or a real moment marked; nothing appears at random. The fun items sit behind Settings › General › **Small surprises** (on by default).

- **Feel:** a pressed shortcut briefly presses its footer cap; holding ⌘ shows every visible action's key; Enter with nothing to paste shakes the action once and says why; ⌘+/⌘− show the size briefly; the menu bar ibis dips its beak after a copy.
- **Saves time:** Services › Search with Ibis; a `.bib` dropped on the menu bar ibis becomes a library; every Undo names its action ("Undo Remove Vale 2021"); ⌥ on About shows the full build; once, after the first paste, a tip that ⇧⌘P keeps Ibis open between citations.
- **Fun:** About's line on the name ("Named for the bird ancient Egyptians linked with Thoth, god of writing and keeper of records."); clicking the About icon dips the bird's beak, the menu bar's gesture, and the line says the next bird fact, one click, one fact, carrying on next time, until you leave the page; the first open greets with the mark; the first, 100th and 1,000th paste say so on the receipt; a library's size is marked once, with the highest milestone reached only ("A good start.", "A solid library.", "A remarkable library."), so a large import is one moment; the day a year after first use, the line reads "One year with Ibis today."
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
| Output per app, self-finishing setup (0.3) | First run above |
| Paste into the app you came from (0.3) | Enter pastes; the footer says "Paste into Word"; a refusal copies and says why |
| Citation forms, 29 locator types, prefix and suffix, reorder, any style, presets (0.4) | The shelf, more label and control rows in the **Inspector**, the format pop-up and Settings › Citations |
| `ibis://` links, Show in Zotero, Open PDF in Zotero (0.4) | A link opens the picker filled in; the Zotero actions sit in More and the row's menu |
| Settings window (0.4, #776) | **Settings** |
| Browse, "Selected in Zotero", passages (0.5) | Collections and tags as search filters; a reference's highlights unfold under its row and fill in its page |
| Bibliography, "References used this session", project `.bib` (0.4) | A sheet from More; rows say "In refs.bib" or "Missing from refs.bib" |
| `ibis check`, DOI lookup (0.6) | Command line first; DOI lookup as an opt-in no-results fallback |
| A source's PDF, choosing the page there (#1252) | **Reader**: the window widens once, the one planned exception to [C10]; thumbnails and pages sit between the list and the column, and a page is cited by choosing it ([M1]) |

## How the design changes

- Change `tokens.css` or the shared component in `src/ui/`, never one screen; fix a visual bug in the rule or component that allowed it, with a regression test.
- The five parameters at the top of `tokens.css` (spacing unit, control radius, body text size, Ibis red light and dark) are the knobs. The canvas's Parameters board turns them live; `node apps/desktop/canvas/apply-params.mjs --radius=10px …` writes chosen values back (then `python3 brand/build.py` for the reds).
- The canvas boards run the real app live (`pnpm -C apps/desktop build:canvas`, then upload `ibis-canvas.js` and `.css` and point the two constants in `Screen.dc.html` at them; boards link neither, so a refresh changes one file); a missing state gets a scenario in `apps/desktop/canvas/main.tsx`, never a hand-drawn copy.
- **Rules that can be measured are measured** [C22]: `apps/desktop/canvas/geometry.ts` holds the layout rules as data (the two columns [C3], pop-ups under their button [C21], form rows sharing edges, equal choice cards [C12]) and checks every canvas screen in the Mac's WebKit (`gallery/webkit.swift`, run by `scripts/check.mjs`) and in Chromium (`gallery/canvas.mjs`). A new part joins a rule by adding its selector to the table; a new rule is a few lines there, never a test per screen. A visual bug a rule could have caught gets a rule.
- **Judged from pictures** (maintainer, 2026-11-04): every visible change renders its canvas scenarios (light and dark, plus larger text, right-to-left, Windows key labels and the forced-colours and Increase Contrast captures where it matters) and is judged by a reviewer who did not make it. A small change needs one judge; a batch gets the panel of independent views (macOS, visual design, first-time user, keyboard only, accessibility, Windows and Linux). A pass merges when no view scores any screen lower than before; work continues until every view scores every screen 9 or better. A defect is fixed in the rule that allowed it, so it cannot come back.
- Record maintainer decisions in [decisions.md](decisions.md) and update the rule here in place, deleting the text it replaces.
- Review this page against Apple's guidelines on each new macOS release.
