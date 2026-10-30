# Design

The one current rule book for how Ibis looks and behaves: picker, first run,
Settings, preview, brand and website. [Decisions](decisions.md) win over it. Values live in
[`apps/desktop/src/tokens.css`](../apps/desktop/src/tokens.css): this page names
tokens by role and gives numbers only where no token exists. Settings has
General, Shortcuts and About. The Settings
window's remaining pages wait in #776; scope is
in [ROADMAP.md](../ROADMAP.md) and progress in the issues. Tags such as [C9]
(design canvas) and [D7] (token foundations) are cited by code and tests; keep
each beside its rule. This page, `tokens.css` and the components in
`apps/desktop/src/ui/` are the single source of truth for design; the design
canvas mirrors them (see **How the design changes**).

## Principles

1. **One anatomy everywhere.** Search, list, shelf and footer, in that order,
   at every size. Layouts fold rather than reorder.
2. **One design, macOS first.** Until per-platform designs return (after the
   rebuild), Ibis uses its macOS design on every system: the same sizes,
   radii, controls, selection and layout. Only what is correctness, not look,
   follows the system: modifier keys and their labels (Ctrl on Windows and
   Linux), the system font and accessibility settings. Ibis keeps its anatomy,
   icon drawings, wording and citation scarlet everywhere.
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
radii or literal shadows [D1]. Three layers (decision 0021): base values that
components never read; role tokens, whose defaults are CSS system colours so an
unknown webview still follows dark mode, the accent and high contrast; then
the shared macOS look (`:root[data-platform="macos"]`), increased contrast, forced colours
in that order. Light and dark come from
`color-scheme` and `light-dark()`; components never read `prefers-color-scheme`.
`data-system` records the actual OS for fonts, modifier labels and native
material correctness; it does not choose a different layout. The copied
notice uses the same 560×100-point frame near the top centre on every OS,
including its 16-point shadow margin; the frame ignores the pointer. The
notice never clips its words: a citation that would be cut short moves to a
second line. A green check means pasted; copied without a paste shows the
same mark in `--muted` with the reason in words.
If Windows cannot release the keys Ibis injected, the notice says "Copied.
Press and release Ctrl and V". Ibis retains the unsent releases and retries
them before any later delivery; it sends no new paste while they remain blocked.

| Role | Tokens | Rule |
|---|---|---|
| Surfaces [D4] | `--surface-window` < `--surface` < `--surface-raised` < `--surface-overlay`; `--surface-field`, `--scrim` | Base, content sheet, band/cards/shelf, menus and dialogs. On the macOS glass panel the levels share one material (`--surface`) and layers separate by fills and hairlines. |
| Text [D5] | `--text`, `--muted`, `--text-tertiary`, `--text-disabled` | `--muted` always 4.5:1; tertiary only for glyphs and hints at rest (3:1), never sentences. |
| Accent [D6] | `--accent`, `--accent-text`, `--text-on-accent` | Always the system accent; Ibis offers no override. Selected rows and the menu highlight use `--selected-text`, the platform's own selected-text colour (white), as native lists do; never black. `--text-on-accent` (primary button, tiles on the accent) is picked per accent to reach 4.5:1. |
| Selection [D7] | `--row-active-*`, `--row-inactive-background`, `--selected-*` | See **Selection and focus**. |
| Focus [D8] | `--focus-ring-*` | See **Selection and focus**. |
| Fills | `--fill-control(-hover/-pressed)`, `--fill-subtle-*`, `--fill-primary-*`, `--fill-disabled` | Pressed is darker on macOS and GNOME, lighter on Windows. |
| Strokes | `--separator`, `--stroke-control`, `--stroke-field`, `--stroke-overlay`, `--line`, `--border-width`, `--contrast-border-width` | Never a coloured border on a control. On macOS a hairline or a shadow, never both with a fill. |
| Citation [D9] | `--citation`, `--citation-tile`, `--on-citation` | Scarlet; see **Colour roles**. |
| Status [D10] | `--status-done`, `--status-look`, `--status-stop`, `--status-info` | Ibis-owned hues, always with words or an icon. Stop is a rose kept apart from scarlet and from Fluent's and Adwaita's red. Info is `--accent-text`. |
| Progress | `--progress-track`, `--progress-fill`, `--progress-border`, `--progress-border-width` | Accent fill on a quiet track; contrast modes use Canvas with CanvasText fill and edge. |
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
(menus, descriptions, footer, shelf) and `--type-label-*`, the smallest;
`--type-page-title-*` (28/36) for Settings page headings. Dialog headings
(20/28) have no token yet.
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
6 px inside the tile column on each side. It is the Ibis mark by default, as
Alfred shows its hat (maintainer, 2026-10-30); Settings › Look ›
**Search icon** offers Ibis mark, Magnifier or None, drawn as small pictures.
With None the slot stays empty, so the query never moves off the title
column. It is a person's choice and never a state [C14].

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
Densities (decision 0020): Compact 40 px (one line), Comfortable 58 px (the default,
two lines) and Spacious 76 px (three lines). Density changes rows only, never
controls, targets, type or the header. The citation key is not shown in rows
(it stays in the accessible name and tooltip); the year (or "No date") is
trailing and vertically centred.

**One steady window** [C10] (maintainer, 2026-10-30). The picker keeps one
size the whole time it is open, in every state: first run, empty search,
loading, results, no matches, recovery, menus and the keys sheet. Typing,
clicking a header control, opening a menu, adding a reference or a notice
never resizes or moves the window; nothing measures content to choose a
height. The size is set before the window is shown, so nothing can be cut off
while it catches up, and there is no resize animation. Default 720 × 480,
minimum 480 × 360. Dragging an edge sets the size for that display, kept
across launches; Ibis never changes it. Larger text multiplies the height by
`--larger-text-scale`, capped at the display's work area. Inside the frame the
header, shelf and footer keep their sizes; the middle area holds the state,
starts S below the header divider and is the only thing that scrolls.
Steady, not rigid: Settings › Look › **Window size** offers Small
(640 × 400), Medium (720 × 480, the default) and Large (840 × 600) as
pictures, and any dragged size shows as Custom. **Compact** (maintainer,
2026-10-30, option C) is the fourth picture: the picker opens as a short bar
(search, up to three recent citations, footer) and grows once to Medium on the
first key, menu or choice screen, then never shrinks while open. Medium stays
the default so a new person never sees the window move.

**Nothing clipped, one scroller** (maintainer, 2026-10-30). No state is ever
cut off: the empty-search hint, a notice line, the shelf and the footer always
show whole, with S below the footer actions. There is no window-level scroll
bar and never two scroll bars at once; only the middle area or an open menu
scrolls, and only while its content overflows. Content taller than the middle
area (a small window, 200% text) scrolls there, never under the header or
footer. A clipped hint or a second scroll bar is a bug, fixed with a
regression test at normal and Larger text and at the minimum size.

**Empty search: first time and every day** [C11]. Same frame, different
content. While no library is chosen, the welcome screen (**First run**). For
the first days with a library, the hint: the magnifier, "Type to find a
reference", what to search by, and the Copy and Add keys, centred in the
middle area. Once there is history, the middle area shows what people most
likely want: Cited here recently (up to three, for the app Ibis was opened
from, #45) then Recently added (three, #1070), as compact rows under their
headings, and the hint shrinks to the search placeholder and the footer keys.
Typing replaces these rows in place.

**Corners** [C15][D3]. Fixed CSS px at every text size; nested shapes are
concentric, and an inner corner never goes below the per-OS floor.
**Equal gaps, derived radii** (maintainer, 2026-10-30): where a shape sits
near its container's edge, the gap is the same on every side it shares with
that edge, and its radius is the container's radius minus that gap. Window 16
→ 8 px inset → footer action and shelf 8 → 4 px gap → key cap 4. The footer is
44 px so its actions sit 8 px from the window's bottom and side, like the
shelf. A shape farther from the edge than its container's radius keeps its own
radius. Check every new nested pair (key caps in hints, chips in menus, fields
in rows) against this before adding it:

```
--r-row  = max(--r-min, --r-win  − 8px)        rows, search field, shelf card
--r-tile = max(--r-min, --r-row  − 12px)       type tile
--r-mi   = max(--r-min, --r-menu − --menu-pad) menu items
```

| Token | Value |
|---|---|
| `--r-win` | 16 (every macOS version; the native panel mask) |
| `--r-min` | 6 |
| `--r-row` | 8 |
| `--r-tile` | 6 |
| `--r-control` | 8 |
| `--r-menu` · `--menu-pad` · `--r-mi` | 12 · 6 · 6 |
| Citation segment pill | 6 |

Key caps use `--r-key`, Settings cards `--r-card`, switches, toasts and capsules
`--r-full`.

**Width classes** (container queries, in em). Below 28.75 em: header controls
wrap and footer hints show keys only. Below 60 em the preview replaces the
list; at 60 em (960 px) it sits beside it [C17]. Below 22 em high the shelf and
pane cap at 6 rem. Strokes at Windows 125 % and 150 % are inset shadows.

## Components and their states

Each `src/ui/` component wraps one React Aria component (decision 0021), owns one CSS
file that reads role tokens only, and has a test per look.
Screens never restyle a shared component.

**Shared components** (`apps/desktop/src/ui/`): Button, Switch, Segmented,
Option (a list row with virtual focus), Menu and ToolbarMenu (the format
pop-up), ChoiceList (choice screens [C12]), ChipGroup and LocatorToken (shelf
references and their details), CitationDetails (the per-reference editor),
KeyCaps, ScrollArea, Progress, ShortcutRecorder, SettingsGroup, SideNavigation
and FileDrag. A screen needing a control not listed here adds it to `src/ui/`
with its rule on this page first.

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
The container is the journal, book or proceedings title, falling back to the
publisher. Strip markup and trim both values before choosing; blank or
markup-only values do not hide the fallback or leave an empty meta label.

**Menus** [C9]. The operating system's own menus only where the system owns
them: menu bar, tray and native right-click (decision 0021). In-window menus (format,
library scope, More) are Ibis's but match native metrics: `--r-menu`,
`--menu-pad`, items padded 10 px on both sides, `--menu-item-height` high (28 px macOS, 32 px Windows and GNOME),
separators inset 10 px with 4 px above and below, native open and close motion.
Checks sit where each system puts them: on macOS and Windows a menu with any
choice or toggle keeps a leading check column for every row, so labels share one
start and shortcuts line up at the end; on GNOME the check follows the label.
More's toggles are `menuitemcheckbox` items; a menu without them starts at its
labels. **More holds actions, not preferences** (2026-10-30): Keep Window Open
and Show What Will Be Pasted (both have keys), then Keyboard Shortcuts,
Settings… and Quit Ibis. Library choices live in the library menu behind the
footer status. Rows, text
size, starting at login and update checks live only in Settings. Sections keep
one order in every Ibis menu: what it is about, what you can do, Ibis itself.
Item labels name the result in plain words; a quiet example of the output may
follow at the end, as in the format menu. The format menu's filter field aligns with the item
labels; menus with more than seven items get one. Format opens 4 px below its
pop-up, aligned to its trailing edge; header menus open `bottom end`, footer
menus `top end`, flipping then shifting within 8 px of the window; a menu opened
above the footer ends at least 8 px above it in normal windows; at heights
up to 400px it can use the full viewport so the filter cannot starve its list. A menu that scrolls ends on a
whole row and fades at an edge with more rows beyond it; its side inset sits on the scrolling list, so a scroll bar never
takes it. Any pointer movement over an item highlights it, also after arrow
keys. Menus size to their full action labels up to the viewport boundary; at
narrow widths and Larger or 200% text, labels wrap instead of disappearing
behind an ellipsis, while checks, whole shortcuts and pointer targets remain
intact. A shortcut in a menu is plain trailing text in native modifier order
(⇧⌘C), never a key cap.
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
needs no fade, as a cut row already shows it. **Edges fade only while there
is more** (maintainer, 2026-10-30): a 14 px fade appears at an edge only while
content lies beyond it, like the soft scroll edge in Apple's own apps, so the
first row at rest and the last row at the true end are always crisp. A fade
yields to the whole active row (label, fill and shortcut), shrinking to the
free space beyond it. Increase Contrast and Reduce Transparency show no fade. Keyboard/hover focus and reflow update this
without taking scroll ownership from React Aria.

**Tables** (the keyboard shortcuts list, Settings lists). Columns size to their
content and never break a word; a table that does not fit scrolls or drops a
column, it does not hyphenate.

**Labels and long strings.** Menu action labels remain readable by widening
then wrapping at the viewport limit. Outside menus, action labels stay on one
line: the layout widens, or the label truncates with a tooltip.
Settings rows and segmented choices follow the wrapping rule in **Settings**.
Data truncates with a tooltip; file names shorten in the middle and
keep their extension. Budget +35 % for German and Greek. RTL uses logical
properties (tile, year and chevron mirror; key caps keep platform order;
the beak quote mirrors because it is punctuation, while the Ibis mark is a
logo and never flips, wherever it sits);
mixed-direction strings are isolated and citation keys stay LTR.
**Wrapped text keeps its padding** (maintainer, 2026-10-30). When text takes
two or more lines (Larger text, long titles, translations), its container
grows to fit it with the same padding on every side: text never touches or
crosses a row's fill, a card's edge or a control. Wrapped lines start where
the first line starts, the trailing control or year stays centred on the
block, and in Settings the control moves below the label rather than
squeezing it. In results, a title wraps to at most two lines and then ends in
an ellipsis; the byline stays one line and shortens authors first ("Okonkwo,
Haddad et al."), then the journal; the year never shrinks and sits level with
the title's first line. The full title and byline are in the tooltip and the
reference's details. Making a window or column wider is never the fix. Check every
row, menu item and Settings row at normal, Larger and 200% text.

**The Ibis mark in the picker.** The bird from `brand/`, eye included, in the
brand gradient for the theme (light `#CF4628`→`#B5331A`, dark
`#FF7F5E`→`#F2552F`); never a plain recoloured silhouette. In the search box it
is 24 px, 2 px outset on each side so the query and results stay aligned; its
eye opens to at least 2 px across at small sizes so it still reads as a bird.
At 40 px it leads the welcome and the opening screens. The empty-search hint shows the
magnifier, the sign for "type", whichever search icon is chosen. The mark never
appears on a control.

**Icons.** Ibis's own drawings on every platform: 20 px grid, 1.5 px rounded
stroke (1.6 px macOS, 1 px Windows, 16 px symbolic `ibis-…-symbolic` on GNOME);
"on" is the filled form. Seven type icons (article, book, chapter, thesis, web
page, report, other) never reuse an attachment drawing. Platform icons only
where the platform owns the meaning.

**Footer and status** [C8]. Status dot and text at the leading edge, then
"Paste into Word · APA ↵" or "Copy N as <format> ↵" (the primary hint,
text colour), "Shortcuts ⌘/" and
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
libraries, check times and problems. **One place for each status**
(maintainer, 2026-10-30): the footer's leading status is the only place
library state lives, and nothing is ever inserted above the results, so a
problem never pushes the list down. The status is a button: it opens the
**library menu** (the library with its count and age, a Reload button, the
Zotero libraries as toggle chips when there are several, Choose Library…,
Reload Library). A problem shows there in a few words with a qualified age and
one inline action: "Zotero not updated · last read 2 h ago · Open Zotero"
(Open Zotero launches the app), "thesis.bib moved · Find…". "Last read" is the
last successful read. Background work (reading a large library, downloading an
update) shows as a small progress ring before the words, never a bar across
the window. When nothing usable is left, the middle area shows the choice
screen instead.
While references are usable nothing else interrupts. Names and ages are isolated for RTL; the
age ticks every 30 s outside the live region.

**Notices.** "Copied" or "Inserted" shows after the picker hides, in a window
that never takes focus: the same pill near the top centre on every system. In-window toasts ("Removed · Undo") last 8 s and pause
on hover and focus. Errors are one sentence beside their cause, never a modal.
On macOS, system-wide Secure Event Input only copies and says "Copied. Secure
typing is on, possibly in another app. Press ⌘V to paste." An unavailable or
timed-out paste check says "Copied. Press ⌘V to paste." Keep these reasons
distinct: the flag does not identify the destination's field and does not
detect every password control. Neither notice calls the destination a
password field.

**Empty states.** First run, no library, an actually empty library and a library that could not be opened
use the choice screen [C12]. Otherwise one template, centred: a 48 px symbolic icon in
`--text-tertiary`, a title (title type), one line of at most 40 characters
(meta type, `--muted`, up to three rows), then one suggested action 16 px below;
quieter text buttons for alternatives. No skeletons or illustrations; the opening screen
only after 400 ms (**Opening a library**). The result heading appears only
above rows.

| Case | Title | Line | Below |
|---|---|---|---|
| No matches | No matching references | Try a surname, year or fewer words. | Clear search; quieter Search all libraries when some are left out |
| Loading or reloading | Opening thesis.bib… / Reloading thesis.bib… | None | Nothing for 400 ms, then the opening screen; retained rows stay |

**Shelf** [C7]. What Enter pastes, one 40 px line above the footer's Copy
action: the beak mark hangs at the tile column's end and the citation starts on
the title column. Shown while there are results or a citation is being built,
including through empty searches and refresh failures; hidden on choice
screens. Results never move when a reference is added. With several
references the citation itself holds them as segments (no chips line): each
reserves 1 px on each side; under the pointer it becomes a soft pill (radius 6),
so nothing moves, no control ever covers its text and neighbours are never
covered (also for zero-width separators). From the keyboard ← → select a
segment as an accent-filled token and Delete removes it. On copy the shelf
shows a closing beak and "Copied" at the line end.

**One line, details on demand** (maintainer, 2026-10-30). The shelf is always
one line: no Page field or form choices beneath it. Clicking a segment (or
Return or Space on a selected one) opens its menu: the reference's short name
as a heading, **Page** with a field, then **Name in brackets**, **Name in the
sentence** and **Year only**, each with a quiet example of what it pastes in
the current style, then **Remove from citation ⌫**. With one reference the
whole citation is its segment. Removing needs no click into the menu
(maintainer, 2026-10-30): a pointed segment shows a 16 px round × badge on its
top-end corner, outside the text and the pill, so the key is never covered;
clicking it removes the reference with an Undo notice. The badge never shows
for keyboard focus or while the segment's menu is open (⌫ removes there). Pages also come from typing (`garcia 2021 p12`
shows a page token beside search) and, from 0.5, the PDF pane's "Cite this
page", where the Page field lives beside the page itself.

- **Text.** Text colours; key formats monospace with punctuation in `--muted`;
  formatted styles in the UI font. A read-only `<bdi dir="auto">` line with an
  ellipsis; the full text is in the tooltip and the wide pane.
- **Page and form** (#1366). A reference's menu holds its Page field (a page
  or range, including Roman numerals) and the three name forms. Opening the
  menu from a selected segment puts focus in Page. With no built selection,
  the same menu on the highlighted result edits it; the first edit adds that
  result to the shelf; subsequent searches cannot change its page or form.
  Editing keeps focus in the menu, clears a queued copy and prepares a new
  preview.
  Clearing Page removes only that reference's page. Optional page labels such
  as `p. 12` or unquoted `pp. ١٢–١٥` use the shared locator parser before
  formatting; Unicode numbers keep their written digits. A recognised page
  label must have a valid value: `pp.` is refused with a request to enter a
  page or range, or clear Page. It never becomes literal page text. Page values
  keep the shared parser's length limit. The field keeps the typed text.
  Only Page edits are parsed as raw input. A locator captured from search is
  already canonical: quoted text such as `S.12` or `pages of notes` keeps its
  literal value through preview, form changes and copying.
  Actionable formatting refusals
  remain visible with the paste preview hidden. Engine losses name the reference
  and field beside the preview; an unsupported form explains the refusal there
  and blocks copying until corrected. Enter leaves that explanation visible.
  Mixed narrative clusters, other locator controls, prefixes and suffixes
  remain in #24/#1315. The menu keeps the shared keyboard focus and touch
  targets.
- **Year-only citations.** Keep supported style forms available through conditional
  layouts. Each selected item's output must show its issued year, or the style's
  own no-date term for an undated or zero-year item; otherwise refuse the citation.
  A matching phrase in a title or locator, or another item's year, does not
  establish a year-only result.
- **Beak mark** (#888). The supplied two-head SVG in `currentColor` from
  `--citation`, 20 px tall (`--space-5`), scaled with text, `aria-hidden`,
  mirrored in RTL, 45 % while pending, `CanvasText` in contrast modes [C16].
- **States.** Pending: the old text dims for 300 ms, then "Formatting…"; Enter
  waits. Failed: a Stop icon, "Couldn't format in APA. Choose another style, or
  copy keys.", no Enter hint. Stale: a Look icon, "The library changed. Reading
  it again…". Warnings: a trailing pill ("1 warning"; "Reference entry" for a numeric
  style, which copies the entry because its number depends on the document).
- **Show what will be pasted.** On by default (`show_paste_preview`, decision 0020),
  ⌘I / Ctrl+I, in More and Settings › General. Off hides only the line and
  closes the wide pane; the chosen references and copy readiness stay. If the
  output had focus, Search receives it first. Formatting refusals stay visible
  until corrected, including refusals received while the output is hidden.
  Copied while the window is kept
  open says "Copied" without repeating text only when the visible output is
  exactly the copied string.
- **Per OS** [C13]. A tint on the macOS glass, a stroked card on Windows,
  full-bleed between dividers on GNOME. Without room it scrolls in its own area,
  keeping 24 px of results reachable.

**Preview** [C17]. Until the reader arrives (Later), the preview is Peek: the
reference's full details and the full citation text, without the page area.
At 960 px or wider the list keeps 400 px and the pane takes
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

The global default is Command+Shift+Y on macOS and Ctrl+Shift+Y elsewhere.
On macOS its letter follows the active keyboard layout; a Command shortcut
uses that layout’s Command layer (including Dvorak–QWERTY ⌘), without Shift,
Option or Control changing the letter. Defaults stay unset in settings, and
an explicitly stored custom shortcut is never replaced.

| Input | Does |
|---|---|
| Click a row | Selects it; the shelf shows its citation [C5] |
| Double-click, Enter, or the footer action | Pastes or copies what the shelf shows, as the footer says; success hides an unpinned window |
| ⌘-click / Ctrl-click, the + on a hovered tile, Shift+Enter | Adds to the citation, in order (no Shift-click ranges) |
| Right-click a row | Opens More for that row |
| Esc | Steps back one layer: menu or dialog, preview, search and filters, then hide |
| ↑ ↓ (focus stays in search) | Move the selected row (`aria-activedescendant`) |
| ⌘K / Ctrl+K | More, the one door to every other action, each with its shortcut |
| ⌘/ / Ctrl+/, or `?` outside a text field | The keyboard shortcuts list |
| ⇧⌘P / Ctrl+Shift+P | Keep window open (the pin) [C6] |
| ⌘I / Ctrl+I | Show or hide what will be pasted |
| Peek control on the focused row, or its key | Shows the reference's full details; arrow and text-editing keys in search keep their usual meaning |
| Typing outside a text field | Types into search |

**Pin** [C6]. A neutral icon button at the header's end, always visible. On:
the filled glyph on the pressed fill, never the accent; More says "Keep window
open" / "Stop keeping open". Escape never unpins.

**Typed locators** (#354/#24). A recognized trailing locator is shown beside
search with **Search as typed** to undo interpretation. Adding a reference captures
its locator; later searches keep prepared references unchanged. **Apply locator**
updates only the highlighted, already added reference. A shared locator token
names its type and value and offers an accessible remove action that keeps the
reference. Both key and formatted previews/copies use these per-reference values.
Interpretation uses the engine's shared parser and all 29 CSL locator types,
including Unicode page numbers; the typed digits stay as written.

**Focus order.** Tab: search box (Tab picks the first tag or author suggestion
while they are open), header pop-ups, the pin, the browsing tabs while visible
(one stop in a `tablist`; ← → choose, Home and End), the footer actions, More,
then back. The list is not a stop. Inside an open menu Tab closes it and moves
on. Nothing asks for focus except the search box on open.

**Filters and browsing.** `#tag`, `y:2019-2022` and `a:name` plus a space become
removable chips (Backspace in an empty box removes the last). Recently cited,
Recently added and With PDF show while the query is empty; suggestions for `#`
and `a:` take their place while typing.

**Search rules** (#1204, in `ibis-core` so every front end shares them). The
index is built once per library change; a keystroke only scans it.

- Pasted lists (#1326): every key syntax in the writer's capability table,
  including BibLaTeX multicites, literal Typst `label("…")` constructors,
  Org, MyST, MultiMarkdown, AsciiDoc, HTML and the key-bearing JSON outputs;
  bare `@key` (also `-@key` and `@{any key}`), keys separated by newlines,
  tabs, commas or semicolons, and identifiers separated by whitespace. Citation
  syntax ignores surrounding prose and locators; it does not attach those
  locators to the new citation. Keys and identifiers can be mixed in paste
  order. Complete identifiers are read before their internal punctuation;
  whitespace ends a DOI token, so a semicolon and `@` in its suffix stay literal.
  Plain lists need at least two entries and every unqualified key must resolve
  exactly, otherwise ordinary word search runs. In comma-separated bare-key
  output, semicolons inside keys stay literal.
  Keys and aliases ignore case; identifiers use their canonical forms.
  Results keep paste order, deduplicate by record identity and ignore the
  ordinary result limit. The heading is “Pasted list”. Missing explicit
  keys and identifiers appear in “Not in your library: …”, including when all
  are missing. Missing identifiers deduplicate by their canonical value.
  With no explicit selection, the preview, Enter and Copy use every current
  list result; the footer says “Copy N citations”. A selection takes priority.
  A pending or stale search cannot supply list results or a missing-key notice.
  A pasted list takes priority over trailing-locator interpretation, so a
  numeric key stays a key. Closest matches never join a recognized list.
  Unclosed citation syntax runs to the end of input.
  Multiline paste replaces line breaks with tabs in the single-line search
  field, preserving whitespace inside citation arguments without adding punctuation.
  Recognition stops at 1,000 entries or 64 KiB; overflow falls back to search
  rather than offering a truncated list. No document text is saved.

- Any word order; case, accents and letter variants folded; "et al." and an
  "and" between names ignored. Every word must match somewhere.
- Per word, the best of: a whole word, the start of a word, inside a word
  (three or more characters, or Chinese, Japanese and Korean), then one typo
  (one letter changed, added, left out or two swapped) in words of five or
  more Latin, Greek or Cyrillic letters, first letter right, at most one
  misspelt word per search. The word still being typed may be a misspelt
  start of a word. Numbers, years, identifiers and tags are never guessed.
- Single letters and two-letter words match word starts only, so `j smith`
  reads as an initial; `jrr tolkien` matches J. R. R. Tolkien.
- Other spellings: `mueller` finds Müller (also `oe`, `ue`, `aa` for ö, ø, ü,
  å; `muller` finds it anyway, accents being folded), and a Greek letter
  standing alone is found by its name: `beta amyloid` finds β-amyloid,
  `nf kappab` finds NF-κB. Greek text keeps its letters, and μ before a
  letter is a unit (μm), not mu. Other spellings never join with nearby words
  into a quoted phrase the text does not have.
- Hints limit the word after them to one field: `a:` any person (authors,
  editors, translators…), `t:` title (and short title), `j:` journal (and its
  abbreviation), `k:` citation key, `#` tag or keyword (a whole word once typed
  out), `y:2019`, `y:2019-2022`, `y:-1990`, `y:2015-`. Quotes keep a phrase
  together and make text literal: `t:"cell signaling"`, `"a:b"`. A bare year
  is searched everywhere, never as a filter. An exact citation key always
  matches, even one that looks like a hint.
- Ranking follows ordered rules, each breaking the ties of the one before
  (as Meilisearch, Algolia and Typesense do): exact citation key, then exact
  title; matches as typed, then with a typo, then those found only in
  abstracts; the field-weighted score (key, title, people and identifiers over
  year, tags, journal and abstract; whole words over starts over insides);
  newest year; a stable order. Typos and abstracts are only searched when the
  passes before them found fewer results than were asked for, so a shorter
  list is always the start of a longer one.
- A typo match is marked (`typo`) so the list can say "Close match".
- When nothing matches every word, the closest matches show instead, marked
  (`partial`) so the list can say so: records matching at least half the words
  (numbers and one- or two-letter words not counted, abstracts not searched)
  and every hinted word, most words first, then by the rules above. One-word
  searches, pasted identifiers and long pasted text get none.
- Partial results use the existing results heading, labelled "Closest matches" instead of "Best matches"; the screen-reader result count includes that label.
- A selected partial result keeps a "Closest match" warning pill on its shelf chip across searches; marked chips retain their width before folding, folded choices name "Closest matches", and selection announcements and copy/paste warnings retain that provenance.

**Learning and parity** (#788). First minute: type, Enter, paste; first week:
one footer tip at a time, retired once done (local "seen" flags only); power:
filters, the format menu, More, pin. No tour or "what's new". Every task works
by mouse, touch, voice or switch access: the tray and app icons open the picker,
every key has a visible control, hover is never the only way to see something,
and visible labels match accessible names.

## Per platform

**One macOS design for now** (maintainer, 2026-10-26). Every system gets the
macOS design below; one code path, with a per-system value only where the
system demands it for correctness.

| | Rule |
|---|---|
| Material [C13] | One glass sheet for the whole panel (Liquid Glass; popover vibrancy fallback) under a veil of the surface colour; menus on glass. Elsewhere, an opaque surface. |
| Accent [C14] | The system accent where the webview reads it, else the `--accent` fallback in `tokens.css`, chosen so white text keeps 4.5:1. |
| Selected row [C4] | Accent fill with white selected text. |
| Search box | Frameless, large. |
| Format pop-up | Borderless pop-up, ⌃⌄, 28 px in a 32 px target, capsule. |
| Switch | 38 × 22. |
| Follows the system | Modifier keys and their key-cap labels (`KeyCaps`: ⌘K joined on macOS; Ctrl K one cap per key on Windows; Ctrl + K on Linux), the UI font, accessibility settings and window handling. |

**Look freeze until 1.0** (maintainer, 2026-10-25). What has shipped stays and
is kept working, but is not extended. Native accent injection, versioned
radii and per-platform depth wait in the Later milestone.

- **Materials** [C13]. Never glass on glass or CSS blur posing as the desktop.
  The macOS veil is the surface at 75%, so the glass shows light and colour from
  behind but never readable shapes under the list. **Readable overlays**
  (maintainer, 2026-10-30): menus, the keys sheet, dialogs and the shelf's
  reference menu are layers for reading, so they sit on `--surface-overlay`
  opaque enough that nothing behind them shows as shapes or text (as macOS
  menus and sheets do); only the panel itself shows the desktop's colour.
  Over the macOS veil, citation and status colours go one step deeper to keep
  4.5:1; fills, hairlines and key caps are translucent; the window edge is a
  1 px hairline, never a white rim. Ibis sets no blur or opacity of its own.
  Reduced transparency and contrast modes make every layer solid.

**Per-platform designs later.** When Windows and GNOME designs return, start
from these recorded choices: Windows Acrylic with a footer band, Fluent radii
(window 8, control 4) and a deeper selected row in dark mode (#0067C0, white
text); GNOME opaque `window_bg`, libadwaita radii (window 15, row 7) and menu
checks after the label. Until then they are not built or extended.

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
User changes to Rows announce the applied view once in a persistent polite
region; loads and save acknowledgements stay silent, and hiding retires the
feedback.
"Copied" is announced politely. From 0.3 every release passes the whole
citation (search, choose, page, paste, back in the document) with VoiceOver
and NVDA and with Chinese, Japanese and Korean input methods; Orca joins for
1.0 (decision 0032). An input method's confirming Enter never copies or
pastes. Every action also has a visible, named control, so no task needs a
key chord, and the full text of a truncated title or citation is reachable
without hover.

## Window behaviour

- **Opening.** On the focused window's display, at the last position there,
  with a 120 ms fade and focus in search. The first time on a display it
  opens centred, with its top in the upper third, never under the notch or
  the menu bar. A search from under a minute ago
  returns selected (memory only), including a search cleared by Escape before
  hiding, even if native activation arrives after the query returns.
  Reopening from a focused result or another control still starts typing in
  search. Text received by Search while the
  window opens survives activation, with the caret at the end; only the
  untouched previous query is selected. On X11, keys sent before the picker
  is mapped and has input focus go to the previously focused window. Ibis does
  not capture those keys with a global keyboard grab. The at-cursor form opens under the caret.
- **Moving and resizing.** Drag the empty header, never the list; snap to the
  centre line and upper third within 18 px (Option or Alt places freely). Drag
  an edge to resize [C10]. Place and size are remembered per display. **Reset
  Window Position** in the menu bar menu and Settings › Advanced restores the
  default place and size.
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
  Settings and a pinned picker do), or animate past 280 ms. On X11 the first press over a
  full-screen app is ignored and a second within two seconds opens Ibis;
  Windows is #510.
- **Platform limits.** macOS: a non-activating floating panel, also over
  full-screen apps. Windows: comes forward only when asked; copies instead of
  typing into elevated apps. Wayland: centred, pinned means "stays open", copies
  and says "Press Ctrl+V" ([compatibility.md](compatibility.md#linux-shortcuts-and-tray)).

## First run and choice screens

[C12] First run, no library, an actually empty library and a library that could not be opened share one pattern: a title and
one line on the tile column's edge, then result-style rows (tile, title, one
line of help, optional trailing tag, chevron).

Rows are 2 px apart (`--row-spacing`) and are a list box: arrow keys move,
Enter or a click chooses, and ↓ from search enters the list. The focused row
fills as the active result row; one suggested row at most gets an accent tile.
A choice screen hides the result heading, the shelf and the Copy hint.

Choice screens are one calm group centred in the steady frame
(maintainer, 2026-10-30): the title (17 px) and line centred, then the rows in
a column at most 540 px wide. On the welcome screen the 40 px mark sits above
the title. Space is shared above and below, never left as one empty band.

**Opening a library.** Nothing for 400 ms, then the same centred group: the
40 px mark, "Opening example.bib…", one quiet line ("Reading your library.
Zotero can stay closed."), and a thin 160 px progress bar (determinate once the
count is known). With Reduce Motion the bar is still and the mark does not
breathe. Rows kept from before stay instead, with only the footer saying
"Reloading…".

- **No library:** "Where do your references live?": Zotero ("Found on this
  Mac/PC/computer" once looked for), BibTeX, BibLaTeX or CSL-JSON file, then "Look for
  libraries on this computer". Nothing is searched until that row is chosen;
  found .bib files join as rows above it (folder as the tag), and the row
  becomes "Look again" with the outcome as its help line. Zotero refusing other
  apps and folders too large to search are one quiet note below. A CSL JSON row
  joins when that source ships (0.2).
- **Empty library:** only after a successful load reports zero references: "No
  references in thesis.bib", then "Choose a library with references." Zotero
  and BibTeX, BibLaTeX or CSL-JSON file are the two rows; a detected Zotero library gets
  "Found on this Mac/PC/computer". Below: "You can drop a file here too."
  A search with no matches keeps its search actions; a failed refresh keeps
  its searchable snapshot and failure notice. No shelf or Copy is shown on
  these choice screens; they lay out inside the steady frame [C10]. An empty first
  library offers recovery before asking where the person writes.
  When a focused citation control disappears, the shelf returns focus to
  Search and keeps its selection announcement mounted; unrelated focus stays.
- **Could not open, nothing kept:** the title names the library and the fix;
  the backend sorts each failure by `problem`:

  | Problem | Title | Line | Rows (first suggested) |
  |---|---|---|---|
  | `missing` file | Can't find thesis.bib | It may have moved, or its drive is disconnected. | Find thesis.bib, Use another library |
  | `unreadable` | Can't read thesis.bib | The reason, as a sentence | Try again, Use another library |
  | `zotero_closed` | Zotero isn't open | Open Zotero, then try again. | Try again, Use another library |
  | `zotero_access_off` | Zotero isn't sharing your library | Where to turn it on | Try again, Use another library |
  | `other` | Can't open thesis.bib | The reason, as a sentence | Try again, Use another library |

  The title and line are announced once; "Use another library" shows the no
  library screen. A failure with references kept stays a notice above them.
- **First run:** one step at most, and none when a library is found. Ibis
  never asks "What do you write in?". The format pop-up beside search names
  the exact output and the captured app, such as `\cite · Browser` or
  `APA · Word`; a long value ends with an ellipsis but keeps its full accessible
  name. ⇧⌘F (Ctrl+Shift+F elsewhere) opens the chooser from search. A format
  pick is remembered for that app. "Just this once" makes the
  next pick apply to one successful copy, then restores the app's output;
  reopening also clears it. "Automatic" removes only that app's remembered
  choice. Priority is one-time override, remembered app output, app-table
  suggestion, then the global default (Pandoc until chosen). "Use as global
  default" deliberately changes the fallback without erasing app choices.
  Markdown editors suggest Pandoc and known LaTeX editors BibLaTeX; formatted
  apps suggest the global style, or APA when the global output uses keys.
  Browsers and code editors cannot identify a document's syntax: Overleaf,
  web Typst and Google Docs share a browser ID, and several writing syntaxes
  can share an editor. The first paste in each of them opens the chooser
  once, and the pick becomes that app's default; it never identifies the
  document (Overleaf and Google Docs share it). Closing the chooser uses the
  global default for that paste and asks again next time; "Automatic" or
  "Forget formats for apps" brings the question back. Enter, the footer action
  and a result's copy action all ask through the same chooser before copying.
  Picking a format resumes that citation once, after the save finishes and its
  exact preview is ready; a failed save uses the pick for this citation and
  asks again next time. "Just this once" keeps the question open for a pick.
  Changing the search or citation, hiding or reopening the picker, or switching
  to another menu or dialog cancels the held action, including after dismissal
  or a format pick while its preview or save is still pending. The question uses known app IDs;
  unknown apps keep the global fallback. App capture currently works on macOS;
  Windows and Linux keep their existing global output until capture is built.
  Ibis never reads or stores a document title or URL to choose an
  output. No format question interrupts each citation. Setup steps finish
  themselves: when Zotero is not sharing, the screen shows where to turn it on and continues on
  its own once it is on; when Zotero is closed, Ibis reads the library anyway;
  with several Zotero profiles, Ibis asks which one; with no library at all,
  built-in sample references let people try Ibis, labelled "Sample" on every
  row and in the footer. The explicit **Try sample references** choice uses
  three bundled invented CSL-JSON references for this run; it never saves a
  library preference. The website demo and help screenshots use this same set
  from `apps/desktop/src-tauri/resources/samples.csljson`: lantern gardens,
  clockwork clouds and the invented sea. The practice task is identical on
  every surface: Search for “lantern”, choose the lantern gardens reference, then press Enter. A
  **Use your own library** choice row returns to setup.
  Choosing a real source replaces samples; sample selections and copy notices
  leave with them. Samples never join a real library. Every sample citation
  ends with the visible text ` [Sample]`, including key outputs and every
  clipboard representation (plain text, HTML and RTF). The marker belongs to
  the prepared citation and its content hash: previews and receipts show the
  same marked text that lands in the document. These are demo citations;
  choosing a real library produces ordinary citations. The
  footer, receipt and announcement describe the action actually taken:
  "Paste into Word", "Copied. Press ⌘V to paste." (the right key per system)
  when pasting is off or unavailable, or "Copied" with the reason. The footer shows the library, Shortcuts and More.

### Everyday details (rebuild, 2026-10-26)

Ranked by how much they help a first-time user; each is built with the release
that owns its area in ROADMAP.md.

1. Output per app, remembered (above).
2. Setup that finishes itself (above); a permission detour keeps the prepared
   citation, notices the grant and waits for a fresh Enter. The two grants are
   named apart: "read your references" and "paste".
   References, locators, search and the prepared output survive the detour,
   including longer than the usual minute away. On return after a read-back
   grant, the guide closes and the picker captures the destination again;
   it never replays the earlier Enter. Its prepared output applies once, even
   if the destination changed, without changing that app's remembered format.
   Reopening before the grant keeps the detour active; destination refreshes
   in the same opening keep its prepared output. A successful copy clears
   the shelf even if hiding fails; only a confirmed permission refusal keeps
   the copied selection for another attempt.
3. Enter does what the footer says ("Paste into Word · APA"). Enter pressed
   while results or the citation preview load waits for the current search and
   output, including reconciliation when results arrive before library status,
   then runs once. Repeated Enter, row and footer activations on the
   unchanged citation during copying, hiding or delivery cannot start another
   copy, even when only the search or chooser scope changes. Returning to a
   citation whose copy is still outstanding cannot repeat it. Deliberately
   choosing a different citation permits a fresh copy; an earlier reply cannot
   replace its shelf or current copy feedback. A changed query,
   reference, page, form or output, arrow navigation, Escape or hiding cancels
   a waiting Enter. Every add/remove activation cancels it, including Shift+Enter
   with no highlighted result. A prepared shelf can copy while the next search
   loads or finds nothing. The footer uses the captured app and native paste policy;
   kept-open citations and citations with warnings say Copy. Final delivery
   still checks the target and may copy with a reason instead. After the
   window closes the existing brief, pointer-ignoring receipt says what
   happened without taking focus. More keeps the actual last outcome and its
   reason, in memory for this run, after the brief confirmation expires; it
   does not retain citation text there. A failed or unfinished later copy
   attempt does not discard a successful copy's delivery receipt; a newer
   successful copy does.
   A fresh Enter for the same prepared citation joins its outstanding copy
   with the current context and scope, so that copy can finish with one
   delivery and receipt. Every activation goes through the same readiness
   checks before starting or joining a copy. A failed reconciliation status
   read cancels waiting Enter; a later library update never replays it.
4. Open in under 100 ms with the last snapshot; text received by Search during
   opening is kept (see **Opening** for X11 input routing). The Linux smoke
   types once as soon as the picker is mapped, without waiting for focus.
   Timing is reported on Linux only; the 100 ms p95 gate is
   enforced nowhere yet. macOS/Windows open time is judged in the maintainer
   hand test [#79](https://github.com/ibiscite/ibis/issues/79).
   Typing replaces the engine's previous search and preview
   work by cancelling it, including queued formatting. Hidden Ibis waits for
   events; it does not poll for library notifications. Fixed speed/resource
   limits and the release-machine measurement protocol are in
   [architecture](architecture.md#native-release-measurements-1369).
5. Forgiving search: accents and transliteration folded, any word order, one
   typo tolerated, initials, "et al." ignored, Chinese, Japanese and Korean
   names with or without spaces, Enter safe while an input method composes.
6. The shelf stays when the next search finds nothing, so a citation being
   built never vanishes.
7. A format pick says its scope: "Pandoc remembered for this browser" or "APA
   for this citation". Output names say what lands: "Chicago · full note
   text", "APA · text", "Pandoc key". The chosen scope is visible in the
   format button and announced; remembering is claimed only after a successful
   save. Names describe the actual bundled output: today's Chicago author-date
   style says "Chicago · author-date text", and IEEE says "IEEE · reference
   entry". A full-note label waits for a full-note output.
8. "Cited here recently": up to three rows at an empty query, per app, local
   only and forgettable. Choosing one picks the reference afresh; it never
   brings back an old page.
9. Change the last citation: an explicit action in More (with its key)
   reopens that shelf, pages included. Ibis replaces text only when the old
   citation is selected; otherwise it copies the new one and says so.
10. A page example in the empty state and a Page field for the focused shelf
    reference (#1366, above); a row shortcut to it remains planned.
11. Empty results name the obstacle and its one fix: a filter, a library
    scope, or "Not in your libraries".
12. Peek: the focused reference's full title, edition, year, identifiers and
    abstract on a visible control and a key; Escape returns to the same row.
13. Plain words in the picker ("Page", "Citation style", "Your library"); jargon
    only in Settings descriptions and docs.
14. "Selected in Zotero" as the first row when Zotero has a selection.
15. Undo instead of confirmation for every removal and reset.
16. A row's menu: Copy key, Copy citation, Copy reference entry, Show in
    Zotero, Open PDF. More adds "Copy as plain text" and "Copy reference as
    Markdown".
17. Text size with ⌘+ and ⌘− (Ctrl elsewhere) inside the picker.
18. Adding the identical reference again focuses it and says "Already in
    citation"; the same reference with another page or affix is added as a
    new segment.
19. Drag the citation from the shelf into any app; reorder with Alt+← → or by
    dragging. A `.bib` or CSL-JSON file dropped on the window becomes a
    library; a PDF dropped there finds its reference (0.5).

Customisation follows "defaults first, then choice": automatic (from the app
and the system) → one key in the picker → the Settings window → a documented,
validated config file for power users → the command line.

## Settings

[C18] Its own window (Dock, taskbar and Alt+Tab while open), opened with
**Settings…** (⌘, / Ctrl+,) in More and the menu bar or tray. A sidebar of
pages runs under the window buttons, as in System Settings; each page is a
stack of titled groups of rows: a label and optional description, then one
control at the end, at least `--settings-row-height` (48 px) or
`--settings-described-row-height` (56 px with a description), on
`--surface-grouped-card` over `--surface-grouped` [D17]. Controls are the
shared `Switch` (38 × 22), `Segmented` and shortcut recorder in `ui/`. Changes
apply at once and reach an open picker; preference resets offer Undo. Built so
far: General (login, pasting, picker look), Shortcuts (open Ibis and the picker's
key list) and About (version, updates); Libraries and Citations follow (#776).

**Pages for everyone** (2026-10-30). Most people never open Settings, so
defaults carry the product (principle 9). Seven pages, each with one job, in this
order: **General** (the opening shortcut, Open at Login, Show in Menu Bar,
pasting), **Look** (rows, text size, window size, search icon, show what will
be pasted; each visual choice a row of small pictures), **Libraries**
(each source with its status in words, which Zotero libraries, Add Library…,
Remove with Undo), **Citations** (default format, formats remembered per app
with Forget, styles in the format menu), **Shortcuts**, then, after a gap,
**Advanced** and **About Ibis**. No page opens with more than seven rows. Labels
are plain words; a one-line description only where a label could be misread.
Visual choices show pictures: each row density is a small drawing of rows,
each window size a small frame, each search icon itself; text size is a
segmented Default, Larger, Largest. A choice is picked by clicking its
picture, never by a menu item that cycles values. Advanced is a page, not a
mode: Reset Window Position, the settings file (Open, Show in Finder; checked
when saved, errors explained on this page), installing the `ibis` command,
copying a problem report, and Reset All Settings with Undo; nothing on it is
needed for everyday use. A search field heads the sidebar: it matches labels,
descriptions and common words ("dark", "startup", "paste"), opens the page and
tints the matching row for two seconds. A setting lives in one place; the
picker keeps only actions with their own keys.

**Show in Menu Bar** is on by default. Turned off, the shortcut still opens
the picker, and opening Ibis from Applications (or Start, or the app grid)
opens Settings; the switch's description says so. The menu bar icon is a
16-point one-colour bird the system tints, from `brand/`, and opens a menu,
never a popover: Open Ibis (with its shortcut), the library line, Reset Window
Position, Settings…, Check for Updates…, Quit Ibis.

The shell fills the viewport at every window size: a two-column grid with a
reserved `--titlebar-height` row above the content. The sidebar background
runs the full height; its navigation starts below the window buttons.
`ui/SideNavigation` uses React Aria's single-selection listbox: every built
page is an option, with exactly one selected option and `aria-current="page"`,
drawn with the shared selected fill and paired text. One Tab stop enters the
list; arrows and Home/End move focus and select the page. Typeahead moves
focus to a matching page; Enter chooses it. Its width is
`--settings-sidebar-width`, capped in proportion to the window so the page
still fits at the minimum 480 × 400 size.

The content page and bounded navigation list scroll independently. The
document, shell, sidebar background and reserved window-button area stay
still. The list fills the height left below the title bar and may scroll when
long labels wrap at large text sizes; rows keep their natural height and
remain reachable by scrolling. The title-bar drag region belongs to the shell, outside both
scrollers. Neither text nor controls may slide behind the transparent title
bar or traffic lights. Never reserve that space with a sticky spacer or a
negative margin.
Opening Settings normally, selecting a page or requesting General again
starts that page at the top, with a large `--type-page-title-*` heading and
`--settings-page-inset` around the content. A single click on the current
sidebar page also returns that page to the top. An explicit Pasting deep link
may reveal that group, after its data loads, by moving only the page scroller.
Presenting an existing Settings window without a page (shortcut or tray)
emits the General page event too, selecting General and resetting its scroll.

`ui/SettingsGroup` owns grouped cards (`--r-card`, `--settings-group-width`)
and inset rows. Groups and cards keep their natural height; the page must
never shrink them to squeeze a long form into the window. Row heights are
minima, include their padding, and grow in rem with text. Labels wrap, controls
wrap below when necessary, buttons inherit the surrounding type and wrap
their labels, and switches keep their shape. This holds at the minimum window
size and with system text scaled up to 2×; no fixed row heights or clipped
labels to make the form fit.

Permission-guide artwork, numbered instructions and actions form one column
at every width. Instructions and **Open System Settings** start at the card's
leading row inset, with only the list-marker indentation; they never occupy a
trailing control column or align to the trailing edge.

Queued choices finish saving before the window closes, with at most a two-second
wait. A failed save shows an alert and restores the saved choice; reloading
choices on return never replaces an edit still being saved.
Each window saves only fields deliberately changed, under the native settings
lock, and uses the returned saved state. Queued view changes never replay an
already-saved output choice. Out-of-order reads never replace newer choices.
When a row's text and control do not fit side by side, the control wraps below
and its choices or labels wrap within the card, including at the minimum
window width and larger system text sizes.

**Shortcuts page and keys sheet** (2026-10-30). The opening shortcut comes
first as its own card: the Ibis mark, "Open Ibis from any app" with one line
on where it works, the shortcut in key caps and **Change…**. Then the picker's
keys in short titled groups (While searching, Anywhere in Ibis), each row an
action in plain words ("Paste the citation", "Add another reference") with its
keys as `KeyCaps` at the end, never key names as text. Standard menu keys
(arrows, Home, End, Tab) are left out of the list. The picker's ⌘/ sheet shows
the same groups.

**Shortcuts.** On Wayland the desktop owns the opening shortcut. There is no
recorder or **Use Default**: show the desktop's trigger text verbatim, or
**Not assigned** when none is reported. Portal version 2 offers **Change…**,
which opens the desktop's configuration; version 1 names the desktop's keyboard
settings. Changes arrive from the portal without polling or saving a trigger
to `settings.json`. Cancellation or portal loss clears the trigger and explains
how to bind `ibis toggle`, with the exact Sway and Hyprland configuration lines
and Nightly's command when applicable. Restart Ibis to retry portal consent.
If a successful session reports no usable trigger, both the picker and Settings
show these binding instructions, keeping the session and its **Change…** capability
active. A later reported trigger clears the instructions.
XWayland's focus or full-screen state never decides whether this shortcut opens
Ibis; the picker uses its toolkit's own focus state.

On macOS, Windows and X11, the recorder button shows the current shortcut in the system's
own notation (⇧⌘Y on macOS). Clicking it says **Type a shortcut…**; Esc or
moving focus cancels. Tab and Shift+Tab cancel and move focus normally.
ASCII letters are saved as logical letters and follow the typed letter on macOS;
all other keys are saved as physical codes and stay at that position, including
after a restart or layout change. Recorded modifiers are Control, Alt, Shift, Command, in that
order. A shortcut needs Control, Alt/Option or Command; **⌘ + one key is
refused** (Ctrl + one key elsewhere) because apps use it for their own commands.
Picker commands, including the format chooser, pin and keyboard help, are also
reserved: the opening shortcut cannot take them.
On macOS, the check uses the active keyboard layout and preserves the recorder's
logical letter or physical key intent; on AZERTY, ⇧⌘? on physical M belongs to
keyboard help. Refusals name the command that owns the chord. Saved shortcuts
pass the same validator at startup; a refused one uses the default with an
explanation.
On Windows, Ctrl+Alt with a printable key (also with Shift) is refused because
it can consume AltGr typing; a Windows-key combination or a non-typing key is available instead.
An unsafe saved shortcut falls back to the default at startup with an explanation.
Refusals explain how to choose another and keep the working shortcut.
The new shortcut is registered before the old one is released. A failed save
keeps it working for this run and says it was not saved for next time.
**Use Default** appears only for a different shortcut and restores ⇧⌘Y
(Ctrl+Shift+Y elsewhere). **In the picker** is a read-only list from `KEY_TABLE`.

**Permissions.** Ibis asks for a system permission only through a guide that
says why in one sentence, names the exact list in the system's current words
(macOS 27: "Device Control and Data Access"; earlier: "Accessibility"), shows
a picture of the switch to turn on, opens that pane with one button, and
notices the change on its own (read again every second and on return), then
confirms in words. It never claims success it has not read back. The guide
opens by itself once per run, the first time a paste is refused, and lives in
Settings › General › Pasting afterwards. Because macOS keys the permission to
the code signature, opening the guide first clears Ibis's own entry when the
permission is not working, so the list shows one fresh switch (#1292, #1242).
Automatic listing stays first. Beside **Open System Settings**, the Ibis app
icon is a file drag source, followed by one line: "If Ibis is not in the list,
drag it there." It drags the running bundle's file URL, including a renamed,
moved or Nightly copy, with only a copy operation; it never moves the app or
changes the citation clipboard. The icon uses the shared `ui/FileDrag` and
button states; clicking or pressing it opens System Settings as the keyboard
alternative. An unbundled development executable cannot be dragged; a failed
drag points to the list's **+** button. Neither starting nor completing a drag
claims a grant: the same live permission read must confirm it (#1390).
Only the guide opened after a refused paste returns to the picker on grant;
an ordinary Settings visit stays open and confirms the grant in words. A
background grant never brings Ibis forward: completion waits for the person
to return. Hidden pages make no permission reads; pasting switched off stops
the one-second checks, keeping the capability read on return.
Zotero's sharing line explains reading references; the macOS guide's line
explains pasting. These are separate grants, never instructions to give both.

**Start Ibis when you log
in** is off until turned on (App Review 2.4.5(iii)), uses each system's words
("Open at Login", "Start when you sign in", "Start on Login"), and is read from
the system, never `settings.json` (`login_item.rs`, [PRIVACY.md](../PRIVACY.md)).
**Check for updates…** sits in the menu bar or tray menu and in Settings ›
About with **Check for updates automatically**, using the same commands;
desktops without a tray reach them through More › Settings…. Builds
without an update key show neither. Both show only the saved choice, and clicks in
either are saved in the order they were made.
In Settings, a saved-choice event supersedes any older automatic-update read,
including a failed read; returning focus may read the choice again.
The tray click carries the absolute choice its item displayed, so a queued save
repainting the check cannot change an earlier click's intent.
On every OS, automatic update offers never show or focus the picker: while it
is hidden or unfocused, the tray menu and Settings › About quietly say **Update available…**.
Opening the picker re-checks before offering the latest update; waiting holds
no check lock or installer metadata. Turning automatic checks off keeps the
label but leaves further checks to an explicit command. An explicit update
command checks and presents its result immediately, opening the picker when needed.

## Brand and mark

[C16] The final master and settings in `brand/` ([brand/README.md](../brand/README.md)):
one silhouette with a top-to-bottom tone, top rim light, bottom rim shade and a
short shadow from one SVG filter. App icon: the scarlet bird at 62 % of a light
tile with the kit's optical centring (on transparency on Windows); entries
below 48 px are flat. Logos and icons come only from `brand/`; regeneration is
in the [icon source notes](../apps/desktop/src-tauri/icons/source/README.md).

## Website

The website's design rules live in [website/README.md](../website/README.md#design).

## Coming features

How each user-visible item planned before 1.0 in ROADMAP.md fits the
components above; "open" means not decided yet. Later work gets a row when it is
scheduled.

| Feature (release) | Fits as |
|---|---|
| Zotero while closed (0.2) | Footer status with words and a dot; detail in the tooltip and More |
| Output per app, self-finishing setup (0.3) | First run above |
| Paste into the app you came from (0.3) | Enter pastes; the footer says "Paste into Word"; a refusal copies and says why |
| Selected text as the search (0.3) | The picker opens with the selection in the search field, all selected so typing replaces it; Enter pastes over the selection |
| Citation forms, 29 locator types, prefix and suffix, reorder, any style, presets (0.4) | The shelf: each reference shows its page, prefix and suffix in place; the form and style in the format pop-up; Settings › Citations |
| `ibis://` links, Show in Zotero, Open PDF in Zotero (0.4) | A link opens the picker filled in, like the shortcut; the Zotero actions sit in More and the row's context menu |
| Settings window (0.4, #776) | **Settings**; General, Shortcuts, Libraries and Citations first |
| Browse, "Selected in Zotero", passages (0.5) | Collections and tags as search filters; a reference's highlights and notes unfold under its row, and choosing one fills in its page ("PDF page 8 · cite p. 417") |
| Bibliography, "References used this session", project `.bib` (0.4) | More and the format pop-up; the session list is editable with Undo; with a project `.bib` chosen, rows say "In refs.bib" or "Not in refs.bib" and More offers its missing entry |
| `ibis check`, DOI lookup (0.6) | Command line first; DOI lookup as a no-results fallback, opt-in |

## How the design changes

- A design change edits this page, `tokens.css` or a shared component, never one
  screen.
- A visual bug is fixed in the rule or component that allowed it, with a check
  (a regression test) so it cannot return.
- The repository is the single source of truth: this page, `tokens.css` and
  `src/ui/`. The maintainer's design canvas mirrors them for visual review and
  is brought up to date after a design change merges; where the two differ,
  the repository wins and the canvas is fixed. A choice explored on the canvas
  counts only once it is written here, in place, with a [C] tag; the stale
  text it replaces is deleted in the same change, never left as a
  "superseded" layer.
- Code that does not yet match this page is listed in one design-system issue
  and fixed in the code; the rule changes only when the maintainer changes it.
- Values marked as measured need native evidence on each platform (#774).
