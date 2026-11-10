# Components and their states

The rules for each shared part, beside its code. The principles are in [design](../../../../docs/design.md) and the values every part reads in [foundations](../../../../docs/design/foundations.md).

Each component in `apps/desktop/src/ui/` wraps one React Aria component (decision 0021), owns one stylesheet that reads tokens only, and is the only way a screen draws that part. A screen needing a new control adds it to `ui/` with its line here first. Shared rules every window uses live in `ui/Foundation.css`.

**States.** Hover (`@media (hover: hover)` only) never moves or resizes anything; a press gives a little (`--press-scale`) and springs back, as in [Motion](../../../../docs/design/foundations.md#motion). Disabled uses `--text-disabled` and stays focusable in menus. Busy shows a spinner after 400 ms with `aria-busy`. Invalid fields show words and an icon, never a red ring.

| | Pointer over it | Chosen or current | Keyboard focus |
| --- | --- | --- | --- |
| Result row | Quiet tint | `--selected` fill | The same fill; never a ring |
| Menu item | Menu highlight | Check in the leading column | Menu highlight |
| Segmented | Quiet tint | Darker thumb on a quiet track | Ring around the group |
| Buttons, pop-ups | One step darker | Primary: accent fill | Ring |
| Switch | No change | Accent track | Ring |
| Search field | None | None | The caret only |

- **Selection is a fill** [D7]: result rows use the full `--selected` fill with paired text, no side pill; a kept-open picker without focus dims it. Picture choices select as System Settings does: an accent ring outside the picture, following its corner, and a semibold caption.
- **Focus ring** [D8] only on keyboard focus: outside on controls, inside on rows (results, choice rows, sidebar), so no scroll area clips it. It hugs what it names, never the space around a control. Something only the app focuses (a sheet as it opens, a menu, anything with `tabindex="-1"`) never shows one (`canvas/geometry.ts`).

| Component | Rule |
| --- | --- |
| Button (`ui/Button`) | Primary (accent, one per view), secondary (`--fill-control`) and quiet (no fill at rest, shared hover). Rounded rectangle, `--r-control`; `--action-h` large, regular in Settings, small (`--control-small-h`) in the shelf and footer; trailing key caps. |
| IconButton | Square, one size and glyph size per context, `--r-control`; rest transparent with secondary ink, toggled-on is the pressed fill with the same outline. Header ×, pin, sheet close, chip × and Settings search clear all use it. |
| Search field (`ui/SearchBar`) | The picker's top band is the window's drag region; `SearchField` is its large frameless input with a hidden label, a clear button while it holds text, and a trailing slot (the page token). Controls in the band fill the field's height and nest their corners in the window's. |
| TextField | A visible label above one single-line input; `--surface-field`, the shared control height and corner, and the same keyboard focus ring. Forms keep validation beside the field and own saving. |
| List row (`ui/Option`) | Every list of options (results, commands, recent and sample rows) draws the same row: the leading tile, a title that wraps to two lines (one in Compact), the year on the first line and a quiet meta line. A screen fills its row's text, never its spacing or colours. |
| Sheet (`ui/Sheet`) | A view frame (maintainer, 2026-11-10, option B): its name in the header's chip, one body that alone scrolls with its text from the title column on a readable measure, and the footer as its action bar in the picker footer's words: leading actions, then the trailing ones with the primary last, then Back with Esc, each plain text as the footer's actions are. A sheet passes its actions as data (`SheetAction`), so none draws its own buttons there; the primary shows ↵ and Enter runs it unless a control in the body has the keyboard. Tab stays inside; Esc, a hide or Back close it; a control that disappears hands the keyboard back to the sheet. `SheetList`/`SheetRow` for a list of things with their buttons, `SheetChoice` for a list to choose one from (the whole row is the button, a check marks the chosen one), `SheetSection` for a titled part drawn as a Settings group, `SheetButtons` for buttons that act on what is just above them. The keys sheet, Cited Since Opening and Writing Projects use it. | Its lists and its output text are Settings cards and rows (`ui/SettingsGroup`), so a sheet looks like the rest of Ibis.
| Held keys (`ui/HeldKeys`) | Holding ⌘ (Ctrl elsewhere) for a second shows each visible control's key in a small badge just above it, or below it where the window has no room above; a badge never covers its control. |
| Pop-up button (`ui/PopupButton`, `ToolbarMenu`) | Shows its value and a chevron that never turns, nearer the edge than the label starts; Space, Enter or ↓ opens it; a pick or Esc returns to search. Named by what it sets ("Copy as Pandoc"). Quiet wherever its neighbours are quiet: in the search bar beside × and pin, and in a band that is itself a fill (the shelf's count); its chevrons say it opens, and its plate shows on hover and while open. |
| Menu (`ui/Menu`) | See **Menus** below. |
| View frame (`ui/ViewFrame`) | A view of the window (maintainer, 2026-11-10): the picker's content swaps out and the view fills the window edge to edge, with no card, border, corner or shadow of its own. Its header is the search bar's geometry: the back chevron (also Esc) where the bird sits, the view's name as a quiet chip, then an optional search field on the title column. Its footer is the picker footer's geometry: status words on the title column saying where the choice applies, the view's actions with the primary last, then Back with Esc. The palette and every sheet are this frame. |
| Palette (`ui/Palette`) | A view frame for every choice that isn't search itself: the format chooser and More. Its chip names it ("Format", "Actions"), its field has one placeholder ("Search formats") and takes keys from its first frame, and its footer status says whose the choice is. Rows sit on the picker list's grid (the same inset, row padding and heading style; every row keeps the tile column for its icon or check): a name, a quiet detail only when it tells rows apart, and the key at the end. `only: "browsing"` groups (your few) give way to `"searching"` ones (the catalogue) when you type, so each row appears once. ↑ ↓ move, Enter runs, Esc or Back returns focus to whatever opened it, Tab leaves for Search. An optional preview column shows what the highlighted row would paste with the note on where it applies at its foot; below 34rem it becomes a strip under the list. The footer's primary is the highlighted row's action with ↵. |
| Segmented | `--r-small` thumb on a quiet `--r-control` track. |
| Switch (`ui/Switch`) | The one on/off control: accent track when on, `--text-on-accent` thumb, size from `--switch-*`. |
| Search field | Frameless and large in the picker; a hairline field with no fill in Settings, glyph on the icon column. |
| Settings group and row (`ui/SettingsGroup`) | See [Settings](../../../../docs/design/settings.md). |
| Shortcut table (`ui/ShortcutTable`) | Small secondary heading, one card of rows, action at the start and keys at the end. |
| Key caps (`ui/KeyCaps`) | See [Key caps](../../../../docs/design/foundations.md#key-caps). |
| Shelf (`ShelfSegment`, `ChipGroup`, `LocatorToken`) | See [Shelf](../../../../docs/design/picker.md#shelf). |
| Inspector (`ui/Inspector`, `CitationDetails`) | See [Inspector](../../../../docs/design/inspector-and-reader.md#inspector). |
| Footer and status line (`LibraryStatus`, `SourceAction`) | See [Footer](../../../../docs/design/picker.md#footer). A status line is words, never a filled band. |
| Notice slot (`ui/NoticeSlot`, `UndoNotice`) and receipt (`CopyReceipt`) | See [Notices](../../../../docs/design/delivery.md#notices). |
| Progress (`ui/Progress`) | A task such as an update is one line of the notice slot: a small ring where the status dot sits, then the title, phase and percent ("Updating Ibis", "Downloading · 3.1 of 7.4 MB", "42%"), so no row moves. An unknown phase turns the ring; Reduce Motion shows it dashed and still. Opening a library keeps a thin bar. |
| Choice cards (`ui/ChoiceList`) | See [First run and choice screens](../../../../docs/design/choice-screens.md). |
| Picture choice (`ui/PictureChoice`) | Equal columns, one per choice, captions on one baseline; never wrap three as two plus one. |
| Sidebar (`ui/SideNavigation`) | Single-selection list with `aria-current="page"`, the Mac's medium sidebar row, selected fill and paired text. `SideResults` draws search results as the same rows. |
| Scroll area (`ui/ScrollArea`) | One drawn scroll bar everywhere (`ui/Foundation.css`): a `--scrollbar-thumb` capsule on a clear track, the same at every size and on every system; the side inset sits inside the bar; `--scroll-fade` only at an edge with more beyond it. |
| File drag (`ui/FileDrag`) | A draggable file icon with button states. |

**Tables** size columns to content and never break or hyphenate a word. **No browser disclosure controls**: never `details`/`summary` or raw triangles; use always-open groups.

## Menus

- [C9] In-window menus (library, the reference count, Form; the format chooser and More are palettes) are `--r-large` frames padded `--space-1`, with `--r-control` rows at AppKit's item height (`--control-small-h`), separators and the shared overlay layer (`ui/Overlay`). A pop-up menu of choices is one narrow column whose rows name the choice and nothing more (15rem, the format menu); a quiet example shows only where names alone can't tell the choices apart, as in Show as (at most 20rem). Longer words (a style's full title, what a key format writes) are found by the filter, never drawn.
- A menu with any check keeps a leading check column for every row, so labels share one start and shortcuts line up at the end.
- **Menu heading**: small bold secondary type that never reads as an item.
- Sections keep one order: what it is about, what you can do, Ibis itself. Labels name the result; a quiet example of the output sits on a second line under the label, as macOS menu subtitles do, in full up to the menu's width, never cut to a column.
- An unavailable choice stays in the list, dimmed, with why in a few plain words on that second line ("Only when cited alone"); it never sets the menu's width and never needs two lines.
- **A pop-up anchors to itself** [C21]: a pop-up that fills a form row (Show as) opens its menu under its own leading edge, at least as wide as the button, as `NSPopUpButton` does (a menu wider than the room left moves back only as far as the window's inset needs); toolbar pop-ups keep `bottom end`.
- **A menu's reason is a heading**, one line, never a paragraph: the format menu says "For Word", "Default format", "For Thesis" or "This paste only".
- Menus over seven items get a filter field aligned with the labels. Header menus open `bottom end`, footer menus `top end`, flipping and shifting to stay within the window.
- Menus size to their labels up to the viewport and never wrap, as Mac menus; only at the window's edge does a label end in an ellipsis, its full name kept for screen readers. A scrolling menu ends on a whole row. Any pointer movement highlights an item.
- **More holds actions, not preferences**: the library (Show in Zotero or Open thesis.bib ⌘O, Show in Folder, Choose Library…), this citation (Change Format…, Peek, Change Last Citation, Keep My Order, Cited Since Opening…, Forget Recent Citations) and Ibis (Keep Window Open, Keyboard Shortcuts, Settings…, Quit Ibis). Rows lives in Settings and the app's paste choices in the format chooser; each thing appears in one place.
