# Design

The one current rule book for how Ibis looks and behaves: picker, first run,
Settings, preview, brand and website. Accepted ADRs win over it. Values live in
[`apps/desktop/src/tokens.css`](../apps/desktop/src/tokens.css): this page names
tokens by role and gives numbers only where no token exists. The unbuilt
Settings window's detailed spec waits in #776; scope is
in [ROADMAP.md](../ROADMAP.md) and progress in the issues. Tags such as [C9]
(design canvas) and [D7] (token foundations) are cited by code and tests; keep
each beside its rule.

## Principles

1. **One anatomy everywhere.** Search, list, shelf and footer, in that order,
   at every size. Layouts fold rather than reorder.
2. **Native material, Ibis voice.** Each platform's material, radii, selection,
   menu metrics, font and shortcut labels (materials and metrics not yet built
   wait for 1.0; see the look freeze under Per platform). Ibis keeps its anatomy, icon
   drawings, wording and citation scarlet on every platform.
3. **Readable first.** Nothing smaller than 12 px; secondary text passes 4.5:1.
4. **Easy to hit.** Every target is at least 32 px; 44 px under `pointer: coarse`.
5. **Show less, reveal more.** An element appears when it becomes useful and
   steps back when it is not. Nothing is removed, only folded one key away. No
   modes and no "advanced" switch.
6. **Say what happened.** Every action ends in a visible result: copied,
   inserted, or why not. Colour is never the only signal. Nothing asks "are you
   sure?"; destructive actions apply at once with an 8 s Undo [D16].
7. **Calm motion.** Movement only explains where something came from, and it
   switches off when the system asks for reduced motion.
8. **No glow, no edge rings.** Selection is a fill, never an outline. No
   coloured borders, outer glows or bright rims. A focus ring appears only when
   the keyboard moved focus.
9. **Great defaults, then choice.** Every setting starts from one default that
   works for most people on every platform, so most never open Settings.
   Customising is there for those who want it, never required. No setting
   exists because a choice was hard to make.

## Tokens

Components read role tokens only, never raw colours, `GrayText`, literal px
radii or literal shadows [D1]. Three layers (ADR 0021): base values that
components never read; role tokens, whose defaults are CSS system colours so an
unknown webview still follows dark mode, the accent and high contrast; then
look overrides (`:root[data-platform=…]`), increased contrast, forced colours
and the gallery contrast preview, in that order. Light and dark come from
`color-scheme` and `light-dark()`; components never read `prefers-color-scheme`.

| Role | Tokens | Rule |
|---|---|---|
| Surfaces [D4] | `--surface-window` < `--surface` < `--surface-raised` < `--surface-overlay`; `--surface-field`, `--scrim` | Base, content sheet, band/cards/shelf, menus and dialogs. |
| Text [D5] | `--text`, `--muted`, `--text-tertiary`, `--text-disabled` | `--muted` always 4.5:1; tertiary only for glyphs and hints at rest (3:1), never sentences. |
| Accent [D6] | `--accent`, `--accent-text`, `--text-on-accent` | Always the system accent; Ibis offers no override. Selected rows and the menu highlight use `--selected-text`, the platform's own selected-text colour (white), as native lists do; never black. `--text-on-accent` (primary button, tiles on the accent) is picked per accent to reach 4.5:1. |
| Selection [D7] | `--row-active-*`, `--row-inactive-background`, `--selected-*` | See **Selection and focus**. |
| Focus [D8] | `--focus-ring-*` | See **Selection and focus**. |
| Fills | `--fill-control(-hover/-pressed)`, `--fill-subtle-*`, `--fill-primary-*`, `--fill-disabled` | Pressed is darker on macOS and GNOME, lighter on Windows. |
| Strokes | `--separator`, `--stroke-control`, `--stroke-field`, `--stroke-overlay`, `--line`, `--border-width`, `--contrast-border-width` | Never a coloured border on a control. On macOS a hairline or a shadow, never both with a fill. |
| Citation [D9] | `--citation`, `--citation-tile`, `--on-citation` | Scarlet; see **Colour roles**. |
| Status [D10] | `--status-done`, `--status-look`, `--status-stop`, `--status-info` | Ibis-owned hues, always with words or an icon. Stop is a rose kept apart from scarlet and from Fluent's and Adwaita's red. Info is `--accent-text`. |
| Progress | `--progress-track`, `--progress-fill`, `--progress-border`, `--progress-border-width` | Accent fill on a quiet track; contrast modes and the gallery use Canvas with CanvasText fill and edge. |
| Elevation [D11] | `--shadow-card`, `--shadow-raised`, `--shadow-overlay`, `--shadow-dialog`, `--shadow-toast` | One overlay shadow for every menu. None in contrast modes, none on rows, chips or inside an overlay; on Linux only cards, overlays and dialogs. In dark mode fills and hairlines separate layers first. |
| Space [D12] | `--space-1…10`, `--inset`, `--row-pad`, `--row-gap`, `--row-spacing` | 4 px grid; 2 px only as an optical nudge or between rows. |
| Shape [D2] | `--r-win`, `--r-min`, `--r-row`, `--r-tile`, `--r-menu`, `--menu-pad`, `--r-mi`, `--r-control`, `--r-key`, `--r-card`, `--r-dialog`, `--r-full` | See **Corners**. |
| Type [D13] | `--type-*`, `--font-family`, `--font-mono` | See **Type**. |
| Motion [D14] | `--motion-feedback/-panel/-layout/-exit`, `--ease-out`, `--ease-in` | See **Motion**. |
| Layers [D15] | `--z-content`, `--z-chrome`, `--z-overlay`, `--z-tooltip`, `--z-modal`, `--z-toast`, `--overlay-margin` | One z-order; overlays keep 8 px from the window edge and never leave the webview. |

**Colour roles.** The system accent means "you are here, act on this":
selection, focus, the primary button, switches and the macOS menu highlight.
Scarlet means "this is what gets pasted" and marks only the citation: the
shelf's beak mark, the added tile, the Copied notice's citation, the format
preview, and the logo [C14]. It never appears on a control, link, toggle,
progress, badge, counter, selection, focus ring, error, Stop status, Settings
control or the shortcut recorder. Brand scarlet itself (no app token) is for the
logo, app icon, website and store art. On an accent-filled row the added tile
becomes a `--text-on-accent` tile with a scarlet check, so a red or orange
accent never meets scarlet fill on fill. Ibis offers Appearance (System, Light,
Dark) and never an accent, brand colour, per-element colours, fonts, radius,
spacing, custom CSS or themes.

**Contrast.** Text, placeholder, footer and key-cap labels: 4.5:1 against
rendered pixels on every look and theme, the 22 px query included; over the
macOS material, over white, black and saturated wallpapers (#774). Focus rings,
meaningful icons, tertiary glyphs and boundaries that are the only cue: 3:1.
Increased contrast and forced colours collapse roles to system pairs (`Canvas`,
`CanvasText`, `Highlight`, `ButtonText`, `GrayText`), make materials opaque,
remove shadows, give controls a 1 px edge, the active row a 3 px edge and focus
a 2 px `CanvasText` ring; scarlet becomes `CanvasText`, so the check says
"added". Reduced transparency makes every material opaque.

**Type** [D13]. The platform UI font (SF Pro; Segoe UI Variable; Adwaita Sans
or the GNOME interface font), never another platform's. One scale of roles,
`--type-search-*` (large and frameless on macOS like Spotlight, a framed box
elsewhere), `--type-title-*` (semibold), `--type-body-*`, `--type-meta-*`
(menus, descriptions, footer, shelf) and `--type-label-*`, the smallest; dialog
headings (20/28) and Windows Settings page titles (28/36) have no token yet.
Weights 400, 500 and 600; no uppercase transforms; italics only for "why it
matched". Monospace (`--font-mono`) only for citation keys and key-format
output. `tabular-nums` on years, counts and pages.

**Text size.** Type and vertical sizes are rem; radii and hairlines stay px.
Larger text multiplies the root size by `--larger-text-scale` and grows the
window by the same factor; three steps are planned (#775). System text scaling
multiplies with it, capped at 2.

## Layout and geometry

**Spacing** [C1]. S = `--inset` = 8 px on every OS. Every fill (search field,
row, shelf, icon-button hover, footer action, menu) keeps S from the window edge
and from any divider it touches. The header is its tallest control + 2S, the
footer its 24 px actions + 2S, and the list's clipping viewport starts S below
the header divider and ends S above the shelf, even while scrolled.

**Header** [C2]. Control bands of 52 px macOS, 48 px Windows, 50 px GNOME: the
tallest control (36 / 32 / 34 px) plus S above and below; the always-visible
1 px divider is outside the band (53 / 49 / 51 px total). Order: search glyph,
query, clear ×, format pop-up (filled, no divider), pin. The trailing button
keeps S to the edge, top and bottom.

**Two columns** [C3]. Icons, the search glyph, the status dot and type tiles
centre on the 32 px tile column; text starts on the title column: row titles,
the search query, the status text and the shelf citation. The search glyph sits
6 px inside the tile column on each side. It is the magnifier unless "Ibis mark
in search" in More (`search_mark`, ADR 0020) swaps in the brand mark, drawn
from `brand/` at the same size and colour.

```
--lead     = row-pad + (tile − icon) / 2       field padding before the icon
--lead-gap = (tile − icon) / 2 + row-gap       icon to placeholder
tile centre = inset + row-pad + tile / 2       36 px on every OS
text x     = inset + row-pad + tile + row-gap  64 px on every OS
```

Distances run from the logical start edge, also in RTL. Windows field and card
strokes are inset shadows, so they take no column space. The trailing edge is
where row text ends (`W − inset − row-pad`): the last header control, the last
footer key cap and the year column end there; a 32 px target may overhang a
drawn glyph. Controls in one bar share a centre line, text a baseline.

| Token | Value | Notes |
|---|---|---|
| `--inset` | 8 px on every OS | [C1] |
| `--row-pad` / `--row-gap` | 12 px / 12 px | Row side padding; tile to text. |
| `--tile-size` / `--icon-glyph-size` | 32 px / 20 px | Compact draws a 24 px tile with no fill, centred in the column. |
| Footer | 40 px | 24 px actions + 2S; Windows and GNOME add a 1 px top divider outside it. |
| Smallest target | 32 px, 44 px on touch | A smaller drawn control keeps the target with `::before`. |

**Rows** [C4]. Rows are 2 px apart in results, Recently added and choice lists.
Densities (ADR 0020): Compact 40 px (one line), Comfortable 58 px (the default,
two lines) and Spacious 76 px (three lines). Density changes rows only, never
controls, targets, type or the header. The citation key is not shown in rows
(it stays in the accessible name and tooltip); the year (or "No date") is
trailing and vertically centred.

**Size follows content** [C10]. Every state is as tall as its content: search
and footer alone, one hint line, Recently added, or a list of choices. Results
grow the window in 220 ms with the top edge fixed, up to the remembered height;
it never shrinks while open and starts compact next time. Reduced motion: no
animation. Width: 720 px by default, 360 px minimum.

**Compact start** [C11]. With an empty query the picker shows its hint (the
search icon, "Type to find a reference", what to search by, and the Copy and
Add keys), and the window is just tall enough for it, with even space above
and below. Recently added (three compact rows, #1070) joins it once the
library records added dates.

**Corners** [C15][D3]. Fixed CSS px at every text size; nested shapes are
concentric, and an inner corner never goes below the per-OS floor:

```
--r-row  = max(--r-min, --r-win  − 8px)        rows, search field, shelf card
--r-tile = max(--r-min, --r-row  − 12px)       type tile
--r-mi   = max(--r-min, --r-menu − --menu-pad) menu items
```

| Token | macOS | Windows 11 | Linux (GNOME) |
|---|---|---|---|
| `--r-win` | 16 (every version; the native panel mask) | 8 | 15 (libadwaita 1.8) |
| `--r-min` | 6 | 4 | 6 |
| `--r-row` | 8 | 4 | 7 |
| `--r-tile` | 6 | 4 | 6 |
| `--r-control` | 8 | 4 | 6 |
| `--r-menu` · `--menu-pad` · `--r-mi` | 12 · 6 · 6 | 8 · 4 · 4 | 12 · 6 · 6 |
| Citation segment pill | 6 | 6 | 6 |

Key caps use `--r-key`, Settings cards `--r-card`, switches, toasts and capsules
`--r-full`.

These numbers are also in [`design-geometry.json`](design-geometry.json), which
`node scripts/gallery-geometry.mjs` measures the evidence gallery against;
change both together with this page (a test checks that they agree).

**Width classes** (container queries, in em). Below 28.75 em: header controls
wrap and footer hints show keys only. Below 60 em the preview replaces the
list; at 60 em (960 px) it sits beside it [C17]. Below 22 em high the shelf and
pane cap at 6 rem. Strokes at Windows 125 % and 150 % are inset shadows.

## Components and their states

Each `src/ui/` component wraps one React Aria component (ADR 0021), owns one CSS
file that reads role tokens only, and has a test per look and a gallery state.
Screens never restyle a shared component.

**States.** Hover (`--fill-*-hover`, only under `@media (hover: hover)`) never
moves or resizes anything; pressed has no scale on Windows and GNOME. Disabled
uses `--text-disabled`, `--fill-disabled` and `aria-disabled` (menu items stay
focusable). Busy shows a 16 px spinner after 400 ms with `aria-busy`. Invalid
fields show words and an icon, never a red ring.

**Progress.** Known downloads show their percentage through completion. Unknown
totals and checking/installing use a moving segment (`--progress-segment-width`,
`--progress-travel`, `--progress-duration`) without `aria-valuenow`. Reduced
motion replaces it with a static full-width dashed indicator, including in
forced colours; it never resembles a partially completed download.

**Selection and focus.**

| | Pointer over it | Chosen or current | Keyboard focus |
|---|---|---|---|
| Result row | Quiet tint | The selected-row fill | The same fill; never a ring |
| Menu item | The menu highlight | Check in a leading column on macOS and Windows, after the label on GNOME | The menu highlight; never a ring |
| Segmented choice | Quiet tint | Raised thumb on a recessed track | Ring around the group |
| Pop-up button, More | Quiet fill | Pressed fill while open | Ring |
| Buttons | One step darker | Primary: accent fill with `--text-on-accent` | Ring |
| Switch | No change | Accent track | Ring |
| Search box | None | None | None: the caret (Windows: its accent underline) |

- **Selection is a fill** [D7]. Every OS: the full accent with paired text (a
  tint reaches only about 1.2:1 beside other rows), no side pill. On the
  selected row an added tile is the paired text colour with a scarlet check
  [D9]. A kept-open picker without focus uses `--row-inactive-background`. Menu highlight: accent on macOS, a subtle fill on
  Windows and GNOME.
- **Focus ring only on keyboard focus** [D8] (`:focus-visible`), never after a
  click, and **drawn inside the item it marks**, so no scroll area clips it and
  it never covers a neighbour. macOS 2 px accent with a 1 px gap; Windows 2 px
  text colour around 1 px surface; GNOME 2 px accent mixed half toward the text;
  contrast modes 2 px `CanvasText`.

**List and row.** Type tile, title, meta line, trailing year. On hover or
selection the add button replaces the tile (plus, or check when added; a 32 px
target with a tooltip and no Tab stop), so the row never reflows; a row in the
citation always shows the scarlet tile with a white check. Comfortable's meta
line: authors shortened as in a citation ("García et al.") then the container,
each with an ellipsis; Spacious adds the type word; Compact puts the first
author after the title. "Why it matched" only when the match is hidden; "In 2
libraries" for duplicates; one 16 px attachment icon in a 24 px chip. Screen
readers hear title, first author, year, container, type and why it matched.

**Menus** [C9]. The operating system's own menus only where the system owns
them: menu bar, tray and native right-click (ADR 0021). In-window menus (format,
library scope, More) are Ibis's but match native metrics: `--r-menu`,
`--menu-pad`, items padded 10 px on both sides, `--menu-item-height` high (28 px macOS, 32 px Windows and GNOME),
separators inset 10 px with 4 px above and below, native open and close motion.
Checks sit where each system puts them: on macOS and Windows a menu with any
choice or toggle keeps a leading check column for every row, so labels share one
start and shortcuts line up at the end; on GNOME the check follows the label.
More's toggles are `menuitemcheckbox` items; a menu without them starts at its
labels. The format menu's filter field aligns with the item
labels; menus with more than seven items get one. Format opens 4 px below its
pop-up, aligned to its trailing edge; header menus open `bottom end`, footer
menus `top end`, flipping then shifting within 8 px of the window; a menu opened
above the footer ends at least 8 px above it. A menu that scrolls ends on a
whole row and fades at an edge with more rows beyond it; its side inset sits on the scrolling list, so a scroll bar never
takes it. Any pointer movement over an item highlights it, also after arrow
keys. Labels never wrap: the menu widens, then the label ends with an
ellipsis. A shortcut in
a menu is plain trailing text in native modifier order (⇧⌘C), never a key cap.
Check each menu side by side with a real one on every platform.

**Buttons.** Primary: `--accent` with `--text-on-accent`, one per view.
Secondary: `--fill-control` (plus the Windows stroke). Quiet: no fill, `--muted`
text, subtle hover. Icon button: a 20 px glyph in a 32 px square whose tooltip
names the action and key; a toggle shows a filled glyph on the pressed fill,
never the accent. Buttons size to content (at least 64 px wide) and never hold a
key cap: they use quiet trailing text ("Cite this page ⌘↩") [C8].

**Pop-ups.** The format pop-up shows its value and a secondary indicator that
never turns (⌃⌄, a Fluent chevron, `pan-down-symbolic`). Space, Enter or ↓
opens it (also Alt+↓ off macOS); a pick or Escape returns to search, Tab moves
on. Named by what it sets and its value ("Copy as Pandoc"); items are
`menuitemradio`. Hidden until a library is chosen.

**Key caps.** Only in the footer and in lists of keys, never in buttons or menus
[C8]. A 20 px fill (`--key-cap` on `--fill-control`), `--r-key`, label-type
UI-font text, never an outline and never scarlet. macOS joins glyphs in one cap (⌘K;
the Return cap is the ↵ glyph); Windows gives each key a cap 4 px apart (Ctrl
K); GNOME adds "+" between caps (Ctrl + K). Decorative caps are hidden from
screen readers; `aria-keyshortcuts` names the keys.

**Scroll areas.** One scroll area (`ui/ScrollArea`) for the results, menus,
Shortcuts and notices: the system's own scroll bars (overlay on macOS and
GNOME) and Fluent's thin bar on Windows, from `--scrollbar-width` and
`--scrollbar-color`. The side inset sits inside the bar, so a classic bar never
takes it. An edge with more content beyond it fades over `--scroll-fade-size`,
so a list that ends on a whole row still shows there is more; the results list
needs no fade, as a cut row already shows it.

**Tables** (the keyboard shortcuts list, Settings lists). Columns size to their
content and never break a word; a table that does not fit scrolls or drops a
column, it does not hyphenate.

**Labels and long strings.** Labels on actions, menu items and settings never
wrap, also at Larger text: the layout widens, or the label truncates with a
tooltip. Data truncates with a tooltip; file names shorten in the middle and
keep their extension. Budget +35 % for German and Greek. RTL uses logical
properties (tile, year and chevron mirror; key caps keep platform order);
mixed-direction strings are isolated and citation keys stay LTR.

**Icons.** Ibis's own drawings on every platform: 20 px grid, 1.5 px rounded
stroke (1.6 px macOS, 1 px Windows, 16 px symbolic `ibis-…-symbolic` on GNOME);
"on" is the filled form. Seven type icons (article, book, chapter, thesis, web
page, report, other) never reuse an attachment drawing. Platform icons only
where the platform owns the meaning.

**Footer and status** [C8]. Status dot and text at the leading edge, then
"Copy N as <format> ↵" (the primary hint, text colour), "Shortcuts ⌘/" and
"More ⌘K" (secondary). Every hint is label first, then keys: 8 px between them,
16 px between hints, label type on one baseline. Actions are quiet 24 px fills,
radius `--r-tile`, with 32 px targets; they invoke the same guarded actions as
the keys and sit in one group named "Keys". With Alt held: "Add to citation ⇧↵",
the Escape step ("Clear search Esc" or "Hide Esc"), then More. When the footer
does not fit, drop Shortcuts first (More lists it), then shorten the status,
then wrap. The status dot is `--status-dot-size`, at the footer's leading edge
(in line with the tiles' edge) and centred on the text's x-height; the status
text follows 12 px after it, as system status bars place an indicator.

| Case | Footer | Dot |
|---|---|---|
| Starting, first run | "Checking library…", "No library chosen" | Grey |
| Opening | "Zotero · opening…" | Grey |
| Loaded | "Zotero · 1,204 references" or "thesis.bib · 1,204 references" | Done |
| Changed while open | "… · updated 2 minutes ago", only when a reread brought different references | Done |
| Reloading | "… · reloading…", count kept | Unchanged |
| Reading notices, empty | "… · 2 reading notices", "Zotero · no references" | Look |
| Update failed, old copy kept | "Zotero · not updated · checked 2 hours ago" | Stop |
| Never opened | "thesis.bib · not opened" | Stop |

"Updated" means the references changed; "checked" means Ibis read them. The
footer always names the library searched; its tooltip and the top of More add
libraries, check times and problems. Names and ages are isolated for RTL; the
age ticks every 30 s outside the live region.

**Notices.** "Copied" or "Inserted" shows after the picker hides, in a window
that never takes focus: a pill near the top on macOS, bottom right on Windows,
a bottom toast on GNOME. In-window toasts ("Removed · Undo") last 8 s and pause
on hover and focus. Errors are one sentence beside their cause, never a modal.

**Empty states.** First run, no library and a library that could not be opened
use the choice screen [C12]. Otherwise one template, centred: a 48 px symbolic icon in
`--text-tertiary`, a title (title type), one line of at most 40 characters
(meta type, `--muted`, up to three rows), then one suggested action 16 px below;
quieter text buttons for alternatives. No skeletons or illustrations; a spinner
only after 400 ms, 48 px below the search band. The result heading appears only
above rows.

| Case | Title | Line | Below |
|---|---|---|---|
| No matches | No matching references | Try a surname, year or fewer words. | Clear search; quieter Search all libraries when some are left out |
| Empty Zotero library | No references in My Library | Add items in Zotero, then reload. | Reload |
| Loading or reloading | Opening thesis.bib… / Reloading thesis.bib… | None | Nothing for 400 ms, then a spinner; retained rows stay |

**Shelf** [C7]. What Enter pastes, one 40 px line above the footer's Copy
action: the beak mark hangs at the tile column's end and the citation starts on
the title column. Shown while there are results; hidden on empty, error and
choice screens. Results never move when a reference is added. With several
references the citation itself holds them as segments (no chips line): each
reserves 1 px on each side; under the pointer it becomes a soft pill (radius 6)
with a 16 px round remove button inside its end over a 6 px fade, so nothing
moves and neighbours are never covered (also for zero-width separators). From
the keyboard ← → select a segment as an accent-filled token and Delete removes
it. On copy the shelf shows a closing beak and "Copied" at the line end.

- **Text.** Text colours; key formats monospace with punctuation in `--muted`;
  formatted styles in the UI font. A read-only `<bdi dir="auto">` line with an
  ellipsis; the full text is in the tooltip and the wide pane.
- **Beak mark** (#888). The supplied two-head SVG in `currentColor` from
  `--citation`, 20 px tall (`--space-5`), scaled with text, `aria-hidden`,
  mirrored in RTL, 45 % while pending, `CanvasText` in contrast modes [C16].
- **States.** Pending: the old text dims for 300 ms, then "Formatting…"; Enter
  waits. Failed: a Stop icon, "Couldn't format in APA. Choose another style, or
  copy keys.", no Enter hint. Stale: a Look icon, "The library changed. Reading
  it again…". Warnings: a trailing pill ("1 warning"; "Reference entry" for a numeric
  style, which copies the entry because its number depends on the document).
- **Show what will be pasted.** On by default (`show_paste_preview`, ADR 0020),
  ⌘I / Ctrl+I, in More until Settings exists. Off hides only the line and
  closes the wide pane; the chosen references and copy readiness stay. If the
  output had focus, Search receives it first. Copied while the window is kept
  open says "Copied" without repeating text only when the visible output is
  exactly the copied string.
- **Per OS** [C13]. A tint on the macOS glass, a stroked card on Windows,
  full-bleed between dividers on GNOME. Without room it scrolls in its own area,
  keeping 24 px of results reachable.

**Preview** [C17]. At 960 px or wider the list keeps 400 px and the pane takes
the rest; below that the preview replaces the list. Pane toolbar and bottom row
are 48 px with S padding; the page area keeps S from the divider and the edge;
the content ends S above the shelf. The pane shows the focused reference, the
references in the citation and the full output, wrapped and selectable. The page
field shows the printed label with "PDF n of m"; "Cite this page ⌘↩"; Shift
extends to a range. Opening it widens the window toward the side with room.

**Overlays** [D15]. One of each kind at a time; a menu closes a tooltip, a modal
closes any menu. Tooltips appear after 600 ms (then at once for 300 ms), name
the action and its key, and never take focus. Dialogs trap focus, end-align
their buttons in platform order and become full-window sheets below 420 × 360.
Overlays close when the picker hides and do not reopen.

## Interaction

One table for every input. [`apps/desktop/src/keys.ts`](../apps/desktop/src/keys.ts)
(`KEY_TABLE`) is the code source of keyboard shortcuts: the keys overlay, the
footer hints and the More menu read their labels from it. A key is added there
only together with the code that handles it. The keys overlay groups them under
where they work (search box, anywhere in Ibis, a menu, any app), with the action
first and its keys at the end of the row, as system shortcut lists do.

| Input | Does |
|---|---|
| Click a row | Selects it; the shelf shows its citation [C5] |
| Double-click, Enter, or the footer Copy | Copies (or inserts) what the shelf shows; only copying closes the window |
| ⌘-click / Ctrl-click, the + on a hovered tile, Shift+Enter | Adds to the citation, in order (no Shift-click ranges) |
| Right-click a row | Opens More for that row |
| Esc | Steps back one layer: menu or dialog, preview, search and filters, then hide |
| ↑ ↓ (focus stays in search) | Move the selected row (`aria-activedescendant`) |
| ⌘K / Ctrl+K | More, the one door to every other action, each with its shortcut |
| ⌘/ / Ctrl+/, or `?` outside a text field | The keyboard shortcuts list |
| ⇧⌘P / Ctrl+Shift+P | Keep window open (the pin) [C6] |
| ⌘I / Ctrl+I | Show or hide what will be pasted |
| → in search | Opens the preview (planned, with attachments) |
| Typing anywhere | Types into search |

**Pin** [C6]. A neutral icon button at the header's end, always visible. On:
the filled glyph on the pressed fill, never the accent; More says "Keep window
open" / "Stop keeping open". Escape never unpins.

**Typed locators** (#354/#24). A recognized trailing locator is shown beside
search with **Search as typed** to undo interpretation. Adding a reference captures
its locator; later searches keep prepared references unchanged. **Apply locator**
updates only the highlighted, already added reference. A shared locator token
names its type and value and offers an accessible remove action that keeps the
reference. Both key and formatted previews/copies use these per-reference values.
The nine existing types are supported; timestamp interpretation remains planned.

**Focus order.** Tab: search box (Tab picks the first tag or author suggestion
while they are open), header pop-ups, the pin, the browsing tabs while visible
(one stop in a `tablist`; ← → choose, Home and End), the footer actions, More,
then back. The list is not a stop. Inside an open menu Tab closes it and moves
on. Nothing asks for focus except the search box on open.

**Filters and browsing.** `#tag`, `y:2019-2022` and `a:name` plus a space become
removable chips (Backspace in an empty box removes the last). Recently cited,
Recently added and With PDF show while the query is empty; suggestions for `#`
and `a:` take their place while typing.

**Learning and parity** (#788). First minute: type, Enter, paste; first week:
one footer tip at a time, retired once done (local "seen" flags only); power:
filters, the format menu, More, pin. No tour or "what's new". Every task works
by mouse, touch, voice or switch access: the tray and app icons open the picker,
every key has a visible control, hover is never the only way to see something,
and visible labels match accessible names.

## Per platform

| | macOS | Windows 11 | Linux (GNOME) |
|---|---|---|---|
| Material [C13] | One glass sheet for the whole panel (Liquid Glass; popover vibrancy fallback) under a veil of the surface colour; menus on glass | Desktop Acrylic (build 22621+), content directly on it; footer band with a top divider | Opaque `window_bg`, no blur; footer band with a divider |
| Accent fallback [C14] (light / dark) | #007AFF / #0A84FF | #005FB8 / #60CDFF | #3584E4 / #78AEED |
| Selected row [C4] | Accent fill, paired selected-content text | Accent fill (#005FB8 light, the deeper #0067C0 dark) with white text | Full accent with its paired text |
| Search box | Frameless, large | Text box whose underline turns accent on focus | Framed entry |
| Format pop-up | Borderless pop-up, ⌃⌄, 28 px in a 32 px target, capsule | ComboBox, chevron | Drop-down, `pan-down-symbolic`, bold label |
| Shortcut labels | ⌘ ⌥ ⇧ ⌃ joined (⌘K) | Ctrl Alt Shift, one cap each | Ctrl Super Shift, "+" between |
| Switch | 38 × 22 | 40 × 20, outlined when off | 46 × 26 |

**Look freeze until 1.0** (maintainer, 2026-10-25, #326). What has shipped
stays and is kept working, but is not extended. Rules on this page that are not
built yet for a platform are deferred: materials, native accent injection and
depth [C13, C14], versioned macOS radii and GNOME icon weight. Their issues are
in the Later milestone. New interface work uses the shared look and the
fallbacks above.

- **Materials** [C13]. Never glass on glass or CSS blur posing as the desktop.
  Over the macOS veil, citation and status colours go one step deeper to keep
  4.5:1; fills, hairlines and key caps are translucent; the window edge is a
  1 px hairline, never a white rim. Ibis sets no blur or opacity of its own.
  Windows: unknown versions, transparency off and battery saver are opaque;
  Mica only for long-lived windows such as Settings. Reduced transparency and
  contrast modes make every layer solid.
- **Accent** [C14]. The system accent, injected natively where the webview
  cannot read it (Windows; GNOME through the portal), else the fallbacks above.
  macOS uses WebKit's paired selected-content background and text.

## Motion

Feedback, panel, layout and exit durations come from `--motion-*`; layout is
the 280 ms hard cap. Entrances use the look's `--ease-out`, exits `--ease-in`. No
bounce. Windows menus slide 8 px and fade; macOS and GNOME menus appear at
once. Chevrons never rotate. Reduced motion: no translate, scale or slide,
fades of at most 100 ms. Never animated: list content while typing, row height,
the caret, layout on hover, the citation text, ticking counts, focus rings,
anything over 280 ms, anything that loops except a busy indicator.

## Accessibility

Every action has a key, discoverable on screen; focus, contrast and screen
reader names follow **Selection and focus**, **Tokens** and **List and row**.
"Copied" is announced politely. VoiceOver, NVDA and Orca verify it for 1.0.

## Window behaviour

- **Opening.** On the focused window's display, at the last position there,
  with a 120 ms fade and focus in search. A search from under a minute ago
  returns selected (memory only). The at-cursor form opens under the caret.
- **Moving.** Drag the empty header, never the list; snap to the centre line and
  upper third within 18 px (Alt places freely); remembered per display.
- **Clicking outside** hides Ibis and the click reaches its target, unless
  pinned or while a page number, picker dialog, file chooser, drag or sign-in
  is in progress.
- **Hover** only tints a row, and only under a moving pointer; a click selects
  [C5], so reaching across rows for Copy never changes what is copied.
- **List scrolling.** A clicked row stays under the pointer. Remember which row
  the click chose until it renders, even if a result reset or preview update
  renders first; only that row consumes the click. Arrow keys take back
  ownership and reveal the keyboard's row.
- **After a copy** Ibis hides, focus returns to the previous window and the
  notice appears; a failed insert leaves the citation on the clipboard and
  says why. Pinned, Ibis stays. Escape never asks or unpins.
- **Never** take keys people type with (so Windows and Linux use Ctrl+Shift+Y,
  not Ctrl+Shift+Space; Ctrl+Alt and Ctrl+Space belong to layouts and input
  methods), open by itself, appear in the taskbar, Dock or Alt+Tab (only
  Settings does), or animate past 280 ms. On X11 the first press over a
  full-screen app is ignored and a second within two seconds opens Ibis;
  Windows is #510.
- **Platform limits.** macOS: a non-activating floating panel, also over
  full-screen apps. Windows: comes forward only when asked; copies instead of
  typing into elevated apps. Wayland: centred, pinned means "stays open", copies
  and says "Press Ctrl+V" ([compatibility.md](compatibility.md#linux-shortcuts-and-tray)).
- **Persistence.** A new choice kept across restarts needs an ADR before it adds
  a `settings.json` field.

## First run and choice screens

[C12] First run, no library and a library that could not be opened share one pattern: a title and
one line on the tile column's edge, then result-style rows (tile, title, one
line of help, optional trailing tag, chevron).

Rows are 2 px apart (`--row-spacing`) and are a list box: arrow keys move,
Enter or a click chooses, and ↓ from search enters the list. The focused row
fills as the active result row; one suggested row at most gets an accent tile.
A choice screen hides the result heading, the shelf and the Copy hint.

- **No library:** "Where do your references live?": Zotero ("Found on this
  Mac/PC/computer" once looked for), BibTeX or BibLaTeX file, then "Look for
  libraries on this computer". Nothing is searched until that row is chosen;
  found .bib files join as rows above it (folder as the tag), and the row
  becomes "Look again" with the outcome as its help line. Zotero refusing other
  apps and folders too large to search are one quiet note below. A CSL JSON or
  YAML row joins only when those sources ship (after 1.0).
- **Could not open, nothing kept:** the title names the library and the fix;
  the backend sorts each failure by `problem`:

  | Problem | Title | Line | Rows (first suggested) |
  |---|---|---|---|
  | `missing` file | Can't find thesis.bib | It may have moved, or its drive is disconnected. | Find thesis.bib, Use another library, Try again |
  | `unreadable` | Can't read thesis.bib | The reason, as a sentence | Try again, Use another library |
  | `zotero_closed` | Zotero isn't open | Open Zotero, then try again. | Try again, Use another library |
  | `zotero_access_off` | Zotero isn't sharing your library | Where to turn it on | Try again, Use another library |
  | `other` | Can't open thesis.bib | The reason, as a sentence | Try again, Use another library |

  The title and line are announced once; "Use another library" shows the no
  library screen. A failure with references kept stays a notice above them.
- **First run:** two steps of the same pattern: where your references live,
  then, once the first library opens with an empty search, "What do you write
  in?" (Markdown (Pandoc), LaTeX, Typst; "Now" marks the current format). The
  choice sets the copy format and returns to search; results appear only after
  it. The copy format has one name everywhere ("Pandoc"); first run adds only
  "Markdown" in front. The writing step's note names the open shortcut; start
  at login lives in More (later Settings). No mark in first run. The footer
  shows "No library chosen", Shortcuts and More.

## Settings

[C18] Not built yet (0.4); the detailed spec waits in #776. Its own window
(Dock, taskbar and Alt+Tab while open) with a searchable sidebar of pages, each
a stack of titled groups of rows: a label and optional description, then one
control at the end, at least 48 px (56 with a description), on
`--surface-raised` [D17]. Changes apply at once; resets offer Undo.

Until then settings live in More and the tray menu. **Start Ibis when you log
in** is off until turned on (App Review 2.4.5(iii)), uses each system's words
("Open at Login", "Start when you sign in", "Start on Login"), and is read from
the system, never `settings.json` (`login_item.rs`, [PRIVACY.md](../PRIVACY.md)).

## Brand and mark

[C16] The final master and settings in `brand/` ([brand/README.md](../brand/README.md)):
one silhouette with a top-to-bottom tone, top rim light, bottom rim shade and a
short shadow from one SVG filter. App icon: the scarlet bird at 62 % of a light
tile with the kit's optical centring (on transparency on Windows); entries
below 48 px are flat. Logos and icons come only from `brand/`; regeneration is
in the [icon source notes](../apps/desktop/src-tauri/icons/source/README.md).

## Website

One calm, content-height page with a structural margin and `@key` links. It
imports the app's `tokens.css` through the public build, so there is no second
token source or motion scale; brand colours, the website roles included, come
from `brand/build.py`, and website CSS only composes the page. Build and
Cloudflare publication instructions live only in `website/README.md`; account
access, DNS records, credentials, backups and `.local/` never enter the public
build.

**Rhythm.** One vertical scale: 1.1 rem between paragraphs, 1.25 rem between
blocks (lists, link rows, the picker), about 5 rem between sections on desktop
and 4 rem on phones. The hero-to-first-section gap equals the section gap;
headings and setup steps follow the same scale, and platform groups get two
large steps when all three are shown. Content sets heights: ordinary scrolling,
no full-screen sections or scroll snapping.

**Chip.** One `@key` component for navigation, section margins and inline
links, with one font size and padding (links add touch space). At rest, scarlet
text on translucent scarlet; hover, a thin inset outline; current, filled
scarlet with contrasting text. Links press in slightly, and arriving at a
section briefly turns its heading scarlet. Keyboard focus is a separate ring
drawn outside, so it never crosses text (inside only in the sidebar, link rows
and the demo, where an edge would clip it). Dragging in the sidebar never
selects page text. No generic badges.

**Motion.** App tokens only: 120 ms feedback, 200 ms hand-overs, ease-out, no
bounce, for chip states, button press, the download-arrow nudge and the theme
thumb. Sections may reveal once from 12 px below; the logo draws once, can
redraw from Home and never loops. Quiet delights: a hovered page chip cites
itself in turn as `[@key]`, `\cite{key}` and `@key`; the Star button's star
fills and grows once on hover; the Why beak pecks once as it appears; the
writing cursor blinks; Copied and Pasted land with a tick and a pasted key uses
the chip's arrival cue; on touch a pressed row gives slightly; text selection is
a soft scarlet tint; the console greets the curious with one line pointing to
the source. Reduced motion turns off movement, transitions, arrival flashes and
smooth scrolling, and holds the chip's first form.

**Layout.**

- **Desktop, 62 rem and up** (tablet landscape too): a sticky sidebar near the
  top, each muted label right-aligned before its chip; the current label is in
  text colour with a filled chip, and a followed link stays current until the
  reader scrolls, even at the page end. The theme switch sits at the sidebar's
  foot, quieter until hovered or focused. The structural margin is a neutral
  hairline; scarlet marks only chips, links and citations.
- **Tablet portrait, 40–62 rem:** one centred reading column, chips centred
  beneath the logo, the picker at full column width, no empty second column.
- **Phone, below 40 rem:** "Early preview" on the logo's row, then a sticky,
  sideways-scrolling chip bar with an edge fade. When the current section
  changes, only the bar scrolls to keep its chip visible, never the page. Link
  rows stack title and note with the arrow on the right.
- Below desktop the theme switch is in the footer. The footer holds the
  open-source note and essential links; its divider stays within the reading
  column and its rule meets the margin rule.

**Type and colour.** System serif for prose, sample writing and headings that
introduce content; platform sans for small print, labels, action-row titles,
controls, functional headings and download names; monospace for citation keys
and output. One type role stays consistent across the page; no remote fonts.
Neutral greys have zero chroma: light background/surface/line `#FFF` /
`#F5F5F5` / `#E5E5E5`, text/muted `#171717` / `#525252`; dark `#111` /
`#1C1C1C` / `#2E2E2E`, text/muted `#F2F2F2` / `#B5B5B5`. The primary button
keeps its hue on hover and only softens toward the background. Page buttons,
and the demo's controls on touch screens, share one slim 2.25 rem height with
an invisible 44 px touch target. Text links rest with a soft underline that
turns solid on hover. Forced colours use system roles. Appearance follows
System until a local choice is made before paint: a small light/dark toggle
with a quiet System reset, no dropdown or hover-only menu.

**Words.** The README is the source for public words; the headline also matches
`brand/settings.json` (a test checks the built site). Feature and roadmap
claims come from [ROADMAP.md](../ROADMAP.md) and **Coming features**: a few
major highlights without version numbers, internal names such as "Insertion"
explained for a new visitor, and nothing planned shown as shipped. Short,
direct sentences, the maintainer's "I" for personal experience, plain
invitations to share a workflow or report a problem; no popularity claims,
promised dates, accent-bar callouts or disclosure controls. "Early preview",
naming the released version from `.release-please-manifest.json` and linked to
its notes, sits near the logo, and limitations come before downloads. Copying
and pasting keys is described first, with APA, Chicago, Harvard, IEEE and MLA
after the example; setup stays short. Each section after the first opens with
one larger lead sentence. Why opens with the beak quote mark and is signed with
the maker's `@key` chip, linking to their GitHub profile. The page closes with
one line and two buttons: Star on GitHub (filled) and Share Ibis (outlined,
system share glyph), which opens the browser share sheet or copies the link; no
third-party share buttons, star counts or badges. The logo aligns with the text
beside it by its ink, not its export clear space.

**Demo.** It mirrors the shipped app: a change to what it shows (the picker,
its keys, styles or paste) updates it in the same pull request, and it grows as
the app gains features worth showing. It uses invented references, says Ibis
does not build the document bibliography, follows **Interaction** and takes its
default shortcuts from `keys.ts`; the app's default shortcut (⌃⌘R, or
Ctrl+Shift+Y) opens and focuses it from anywhere on the page. Its selected row
is the system accent, never scarlet; without a known desktop platform (phones)
it uses the macOS accent pair, because WebKit's AccentColorText is black. The
result list keeps one height while filtering and the paste slot keeps the
sentence's line height, so the page never jumps. An underlined blank marks the
paste position; after copying it becomes a keyboard-accessible dashed scarlet
"Paste" chip with the platform paste shortcut, then the copied key lands there,
and copying again keeps the old citation until the next paste. Copy and paste
feedback sits beside the shelf output. The picker stays visible after copying
and pasting so visitors can try another reference; Escape still closes and
reopens it.

**Behaviour.** Offer the visitor's platform first and keep the others
reachable; phones and tablets see all platforms. Downloads keep the fixed
GitHub `stable` URLs. Explanation, setup, downloads and navigation work without
JavaScript, and every action works by keyboard and touch without hover.
On-page links stay in the page and GitHub links open in the same tab. A missing
page is the same quiet page with the logo, one line and a way home; neither it
nor preview links are indexed.

## Coming features

How each user-visible item planned before 1.0 in ROADMAP.md fits the
components above; "open" means not decided yet. Later work gets a row when it is
scheduled.

| Feature (release) | Fits as |
|---|---|
| Page numbers and locators (0.2, #354; other locator types 0.4, #24) | Typed in the search box; included in the shelf's citation; "Cite this page ⌘↩" in the preview |
| Any CSL style, prefix, suffix, author omission (0.4) | Styles in the format pop-up (filter, leading checks), listed per Settings › Citations; prefix, suffix and omission controls: open |
| Zotero while closed (0.3, #14), changed keys, cloud-synced data | Footer status with words and a Look or Stop dot; detail in the tooltip and More |
| "Is this in my library?" (0.5, #701) | A pasted identifier searches like a query; a missing one is an empty state handing off to Zotero; layout: open |
| Settings window (0.4, #776) | **Settings**; General, Shortcuts and Libraries first |
| Bibliography export (0.4, #667) | Export the shelf or the session's citations from More to `.bib`, Hayagriva or CSL-JSON; placement: open |
| `ibis toggle` for Wayland (0.2, #1091) | No picker UI; Settings › Shortcuts explains it |

## How the design changes

- A design change edits this page, `tokens.css` or a shared component, never one
  screen.
- A visual bug is fixed in the rule or component that allowed it, with a check
  (a test, the geometry check or a gallery state) so it cannot return.
- The maintainer's design canvas is the visual reference. Its decisions are
  written here, in place, with a [C] tag; the stale text they replace is
  deleted in the same change, never left as a "superseded" layer.
- Values marked as measured need native evidence on each platform (#774).
