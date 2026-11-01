# Design

The one current rule book for how Ibis looks and behaves: picker, first run,
Settings, preview, brand and website. [Decisions](decisions.md) win over it. Values live in
[`apps/desktop/src/tokens.css`](../apps/desktop/src/tokens.css): this page names
tokens by role and gives numbers only where no token exists. Settings has
General, Look, Citations, Shortcuts and About. The Settings
window's remaining controls wait in #776; scope is
in [ROADMAP.md](../ROADMAP.md) and progress in the issues. Tags such as [C9]
(design canvas) and [D7] (token foundations) are cited by code and tests; keep
each beside its rule. This page, `tokens.css` and the components in
`apps/desktop/src/ui/` are the design in code; the design canvas shows the
same design and must never differ from them (see **How the design changes**).

## Principles

1. **One anatomy everywhere.** Search, list, shelf and footer, in that order,
   at every size. Layouts fold rather than reorder.
2. **One design, macOS first.** Until per-platform designs return (after the
   rebuild), Ibis uses its macOS design on every system: the same sizes,
   radii, controls, selection and layout. Only what is correctness, not look,
   follows the system: modifier keys and their labels (Ctrl on Windows and
   Linux), the system font and accessibility settings. Ibis keeps its anatomy,
   icon drawings, wording and citation scarlet everywhere. The macOS design
   follows Apple's Human Interface Guidelines and the shipped system exactly:
   Apple's sizes, type styles, colours, materials, controls and wording
   conventions (maintainer, 2026-11-01). Colours are AppKit's exact values;
   contrast rises the way the Mac raises it, with Increase Contrast. Each
   departure is named in **Where Ibis departs from Apple**. Windows and GNOME
   designs come later as their own value layers on the same components.
3. **Readable first.** Nothing smaller than 12 px; with Increase Contrast on, secondary text passes 4.5:1.
4. **Easy to hit.** Every target is at least 32 px; 44 px under `pointer: coarse`.
5. **Show less, reveal more.** An element appears when it becomes useful and
   steps back when it is not. Nothing is removed, only folded one key away. No
   modes and no "advanced" switch.
6. **Say what happened.** Every action ends in a visible result: copied,
   inserted, or why not. Colour is never the only signal. Nothing asks "are you
   sure?"; destructive actions apply at once with an 8 s Undo [D16].
7. **Calm motion.** Movement only explains where something came from, and it
   switches off when the system asks for reduced motion.
8. **No glow, no edge rings.** Selection is a fill, never an outline.
   Exception: picture choices in Settings (Appearance, Rows, Search icon,
   Window size) select as macOS System Settings does: a 2 px accent ring 2 px
   outside the picture, following its corner, and a semibold caption; no tinted
   box behind it, so each choice has one corner. No other coloured
   borders, outer glows or bright rims. A focus ring appears only when the
   keyboard moved focus.
9. **Great defaults, then choice.** Every setting starts from one default that
   works for most people on every platform, so most never open Settings.
   Customising is there for those who want it, never required. No setting
   exists because a choice was hard to make.


## Where Ibis departs from Apple

Every place Ibis's macOS design differs from Apple's guidelines or the shipped
system, and why. Anything not listed here follows Apple; a new departure is
added here in the same change, or it is a bug.

| Where | Apple | Ibis | Why |
| --- | --- | --- | --- |
| Hit areas | Regular controls are 24 px | Drawn at Apple's size, hit area at least 32 px; full-width menu rows keep Apple's 24 px | Principle 4, easy to hit. |
| Smallest text | Apple uses 10 and 11 pt captions | Nothing under 12 px | Principle 3, readable first. |
| Accent choices | System accent only for apps that follow it | Follow the Mac by default, plus Ibis scarlet and tuned Mac colours | Maintainer's choice (2026-10-30); every choice keeps the same three roles and contrast. |

## Tokens

**Three layers, each value declared once** (maintainer, 2026-11-01: "like
professional teams do it in Figma"). One stylesheet holds them for the canvas
and the app; `tokens.css` uses the same names and values.

1. **Primitives:** raw values named by what they are: the 4 px space scale,
   the ink (black in light, white in dark), measured hues (`--mac-blue`,
   `--scarlet-500`, `--green-600`). Only layer 2 reads them.
2. **Semantic:** what a value is for, set per mode (platform × light or dark):
   `--text`, `--surface`, `--fill-1`…`--fill-5`, `--separator`, `--accent`,
   `--selected`, `--ring`, and the corner and size roots (`--r-win`,
   `--r-menu`, `--r-control`, `--r-card`, `--inset`, `--menu-pad`,
   `--action-h`, `--key`). Increase Contrast and forced colours are modes here.
3. **Component:** what a part needs, derived from layer 2 by a written rule
   and never typed as a second number (`--r-row`, `--r-key`, `--r-mi`,
   `--key-gap`, `--footer-h`, `--selected-hover`, `--key-fill`). A platform may
   override a derived value only where its own guidelines differ.

Components read layers 2 and 3 (and the space scale), never raw colours,
`GrayText`, literal px radii or literal shadows [D1]; canvas boards draw with
the same components and never restyle them locally. Role tokens default to CSS
system colours so an unknown webview still follows dark mode, the accent and
high contrast (decision 0021). Light and dark come from
`color-scheme` and `light-dark()`; components never read `prefers-color-scheme`.
`data-system` records the actual OS for fonts, modifier labels and native
material correctness; it does not choose a different layout. The copied
notice uses the same 560×100-point frame near the top centre on every OS,
including its 16-point shadow margin; the frame ignores the pointer. The
notice never clips its words: a citation that would be cut short moves to a
second line. The notice is one card (maintainer, 2026-10-30): radius 18, 8 px
inset, a 36 px tile of radius 10, then the title ("Pasted into Word", 14/18
semibold) and at most two lines of the citation (12/16 mono), ending "+4 more"
when longer. Pasted shows a `--done` tile with a `--surface` check;
copied without a paste shows a `--fill-control` tile with a `--text-secondary` check and the
reason in words.
If Windows cannot release the keys Ibis injected, the notice says "Copied.
Press and release Ctrl and V". Ibis retains the unsent releases and retries
them before any later delivery; it sends no new paste while they remain blocked.

| Role | Tokens | Rule |
|---|---|---|
| Surfaces [D4] | `--surface`, `--surface-grouped`, `--surface-overlay`; `--raised`, `--surface-field`, `--scrim` | Three levels, as on the Mac: the window material, grouped content on it (the quaternary fill over `--surface`), and the menu material for every overlay. Fields keep their own background. `--raised` is the Mac's under-page colour, only for cards and the segmented thumb. No other surface colours. |
| Text [D5] | `--text`, `--text-secondary`, `--text-tertiary`, `--text-quaternary`, `--text-disabled` | Apple's label ladder (label, secondary, tertiary, quaternary) and disabled control text, at AppKit's exact alphas. Tertiary is for placeholders and glyphs at rest, never sentences; quaternary only for watermarks. Increase Contrast raises them. |
| Accent [D6] | `--accent`, `--accent-text`, `--selected`, `--selected-text`, `--text-on-accent` | The system accent by default; Look's Accent row can override it (maintainer, 2026-10-30). Selected rows and the menu highlight use `--selected-text`, paired to the chosen accent; the system and existing fixed accents use white, while Ibis scarlet uses dark ink in dark mode (maintainer, 2026-10-31). Selected rows, the menu highlight and a focused citation use the selection fill and `--selected-text`. The default button (Copy, a primary button) is Apple's default button: `--accent` with `--text-on-accent`. |
| Selection [D7] | `--selected`, `--selected-text`, `--selected-text-secondary`, `--row-inactive-background` | See **Selection and focus**. |
| Focus [D8] | `--ring`, `--ring-offset`, `--ring-shadow` | See **Selection and focus**. |
| Fills | `--fill-1` … `--fill-5`; roles `--fill-control(-hover/-pressed)`, `--fill-quiet-hover`, `--fill-primary-*`, `--fill-disabled` | Apple's fill ladder (system, secondary, tertiary, quaternary, quinary fill), black on light and white on dark at the Mac's own alphas. Roles only point at the ladder: control rest, hover and pressed are tertiary, secondary and system fill; quiet row hover and grouped content are quaternary. Pressed is darker on macOS and GNOME, lighter on Windows. |
| Strokes | `--separator`, `--stroke-field`, `--border-width`, `--contrast-border-width` | Two strokes: the separator (Apple's separator colour) for hairlines and dividers, and a field edge that meets 3:1. Never a coloured border on a control. **One edge per shape** (maintainer, 2026-11-01, on lines doubling up): a shape's edge is exactly one of a border, an inset shadow or an overlay's shadow ring (on macOS with the glass edge), never two stacked, so translucent edges never paint twice and nested content keeps its exact gap. A divider is one element owned by one side; two neighbours never both draw it. |
| Citation [D9] | `--scarlet`, `--scarlet-tile`, `--on-scarlet` | Scarlet; see **Colour roles**. |
| Status [D10] | `--done`, `--look`, `--stop` | Ibis-owned hues, always with words or an icon. Look is a golden amber, `#b07a00` light and `#f5c542` dark (maintainer asked for lighter, 2026-10-30), at least 3:1 as a glyph. Stop is a rose kept apart from scarlet and from Fluent's and Adwaita's red. Info is `--accent-text`. |
| Progress | `--progress-track`, `--progress-fill`, `--progress-border`, `--progress-border-width` | Accent fill on a quiet track; contrast modes use Canvas with CanvasText fill and edge. |
| Elevation [D11] | `--shadow-card`, `--shadow-overlay`, `--shadow-dialog`, `--shadow-toast` | One overlay shadow for every menu. None in contrast modes, none on rows, chips or inside an overlay; on Linux only cards, overlays and dialogs. In dark mode fills and hairlines separate layers first. |
| Space [D12] | `--space-1…10`, `--inset`, `--row-pad`, `--row-gap`, `--row-spacing` | 4 px grid; 2 px only as an optical nudge or between rows. |
| Shape [D2] | Roots `--r-win`, `--r-menu`, `--r-control`, `--r-card`, `--r-floor`, `--r-floor-mark`, `--r-full`; derived `--r-row`, `--r-action`, `--r-tile`, `--r-mi`, `--r-seg`, `--r-key` | See **Corners**. |
| Type [D13] | `--type-*`, `--font`, `--mono` | See **Type**. |
| Motion [D14] | `--dur/-panel/-layout/-exit`, `--ease`, `--ease-in` | See **Motion**. |
| Layers [D15] | `--z-content`, `--z-chrome`, `--z-overlay`, `--z-modal`, `--z-toast`, `--inset` | One z-order; overlays keep 8 px from the window edge and never leave the webview. |

**Apple's semantic ladders** (maintainer, 2026-11-01: simplify,
standardise and learn from Apple; platform-specific looks come later). Colour
tokens follow the Mac's semantic colours, not invented greys: one label
ladder for text, one fill ladder for every fill, one separator, three
surfaces, and the accent roles below. Each token has a light and a dark value
taken from AppKit exactly; Increase Contrast supplies the stronger set. A component never mixes its
own colour; it names a role, and the role names a ladder step. New needs pick
an existing step before adding a token.

**Colour roles.** The system accent means "you are here, act on this":
selection, focus, the primary button, switches and the macOS menu highlight
(the ones carrying words use its deeper selection shade).
Scarlet marks the citation by default: the shelf's beak mark, the added tile,
the Copied notice's citation, the format preview and the logo [C14]. It also
serves as a control accent when the person explicitly chooses **Ibis scarlet**
in Look (maintainer, 2026-10-31). Status and errors retain their own roles.
On an accent-filled row the added tile becomes a `--selected-text` tile with
a scarlet check, keeping citation and selection legible even with a warm accent.
Ibis offers Appearance (System, Light, Dark; System by default) and Accent:
the Mac's own row of colour dots, first **Follow the Mac** (the default), then
blue, purple, pink, red, orange, yellow, green and graphite in Apple's order,
then a divider and **Ibis scarlet** as the one extra (maintainer, 2026-11-01:
plain, stock macOS first; Ibis's own styling is secondary). All swatches have Fluent names and tooltips. The Ibis swatch uses
the real `scarlet` and `scarlet_dark` from `brand/settings.json`, through
`--accent-ibis`: `#C23A1C` light, `#FF7A5C` dark. Its
`--accent-ibis-on` pairs white in light mode and `#171717` ink in dark
mode. Text on the accent, including selected rows and hover/pressed primary
buttons, passes 4.5:1. Selected switch thumbs use the same paired foreground,
including the permission-guide illustration; white must never disappear into
the brighter dark-mode scarlet. Meaningful accent fills, rings and glyphs pass 3:1 on
their adjacent surfaces in both themes. Contrast modes retain system pairs.
**One accent, three roles, both modes** (maintainer, 2026-11-01). Every
Accent choice, Follow the Mac included, resolves to the same three roles, each
with its own light and dark value: the accent (controls, switches, the focus
ring, progress) uses the Mac's colour for that mode, such as blue `#007AFF`
in both modes on macOS 27; the selection fill is the deeper shade of the same
hue that keeps its paired text at 4.5:1, such as `#0064E1` light and
`#0059D1` dark, never brighter in dark mode than in light; and accent text on
surfaces passes 4.5:1. The focus ring is Apple's keyboard focus indicator
(the accent's focus shade at 50%), 2 px with a 1 px gap; with Increase
Contrast it is the solid accent. Hover and pressed selection fills (`--selected-hover`,
`--selected-pressed`) darken toward black by 10% and 20% in both modes, so
white text keeps 4.5:1 (mixing toward the text colour would lighten them in
dark mode). The System page of the design canvas
shows these roles labelled side by side in light and dark.
The accents are a fixed, tuned set rather than a free colour picker
(maintainer, 2026-11-01): Follow the Mac stays the default, then the Mac's
colours in Apple's order, then Ibis scarlet, each with values chosen to look right
and pass contrast in both light and dark; an arbitrary colour could not keep
those pairs.
**Selection and the default button, as on the Mac** (maintainer,
2026-11-01: Apple's values exactly). Selected rows, the macOS menu highlight
and a focused citation use `--selected` with `--selected-text`. The default
button (Copy, a primary button) is `--accent` with `--text-on-accent`, as
AppKit draws it; switches, checkboxes, progress and indicators use `--accent`
too.

**Exact macOS values**, measured from AppKit on macOS 27.2 (2026-11-01; the
script is in `docs/macos-metrics.swift`, rerun it on each new macOS). Ibis
uses them unchanged. Light / dark; black or white at the given alpha unless a
hex:

| Token | Apple role | Light | Dark |
| --- | --- | --- | --- |
| `--surface` | window background | `#FFFFFF` | `#1E1E1E` |
| `--text` | label | .847 | .847 |
| `--text-secondary` | secondary label | .498 | .549 |
| `--text-tertiary` | tertiary label | .259 | .247 |
| `--text-quaternary` | quaternary label | .098 | .098 |
| `--text-disabled` | disabled control text | .247 | .247 |
| `--fill-1` … `--fill-5` | system … quinary fill | .098, .078, .047, .027, .008 | the same |
| `--separator` | separator | .10 | .12 |
| `--accent` | control accent (Multicolor) | `#007AFF` | `#007AFF` |
| `--selected` | selected content background | `#0064E1` | `#0059D1` |
| `--accent-text` | link | `#0068DA` | `#419CFF` |
| `--selected-text-secondary` | text on selection, secondary | .82 white | .82 white |
| `--ring` | keyboard focus indicator | `#0067F4` at .5 | `#1AA9FF` at .5 |

Increase Contrast (`prefers-contrast: more`) uses the Mac's stronger set; until
it is measured, interim values give secondary text 4.5:1, glyphs 3:1 and a solid
accent ring.

Roles point at the ladder: control rest, hover and pressed are the tertiary,
secondary and system fills; quiet row hover and grouped content are the
quaternary fill (the quinary fill, .008, is too faint to group).

**Type** follows the Mac's text styles (macOS 27.2): large title 26/32, title
1 22/26, title 2 17/22, title 3 15/20, headline 13/16 semibold, body 13/16,
callout 12/15, subheadline 11/14, footnote and captions 10/13; menus 13.
Ibis uses body for text and menus, callout for labels, title 3 for row titles
and title 1 for the search field; nothing below callout (principle 3).

**Control sizes** (macOS 27.2 heights): mini 16, small 20, regular 24, large
28 for push buttons, pop-ups and segmented controls. The picker uses large
(28) and the Settings window uses regular (24), as Spotlight and System
Settings do. Hit areas stay at least 32 px (principle 4) without changing the
drawn size. Measured with `docs/macos-metrics.swift` on macOS 27.2: menu items
24 px with 5 px above the first and below the last, separators 11 px in all;
sidebar rows 24 small, 32 medium (the default) and 40 large; checkboxes and
radio buttons 16 px at regular size; the window title bar 32 px with 14 px
window buttons 9 px from the corner; search field text 13 pt.

**Accent choices, per mode** (accent · selection · accent text; selection
text is white unless noted). Mac colours, darkened in light or lightened in
dark only where they miss 3:1 as a control or 4.5:1 as text; dark selection
is never brighter than light:

| Accent | Light | Dark |
| --- | --- | --- |
| Follow the Mac, Blue | `#007AFF` · `#0064E1` · `#0068DA` | `#007AFF` · `#0059D1` · `#419CFF` |
| Ibis scarlet | `#C23A1C` · `#C23A1C` · `#C23A1C` | `#FF7A5C` · `#FF7A5C` (ink `#171717`) · `#FF7A5C` |
| Purple | `#953D96` · `#953D96` · `#953D96` | `#A550A7` · `#904291` · `#BD71BF` |
| Pink | `#F74F9E` · `#E4086C` · `#E4086C` | `#F74F9E` · `#E4086C` · `#F74F9E` |
| Red | `#E0383E` · `#DE3036` · `#DD2C32` | `#FF5257` · `#E80005` · `#FF5257` |
| Orange | `#E77008` · `#B55C08` · `#B55C08` | `#F7821B` · `#B55C08` · `#F7821B` |
| Yellow | `#B48A00` · `#916E00` · `#916E00` | `#FFC600` · `#916E00` · `#FFC600` |
| Green | `#56A33F` · `#468337` · `#447F35` | `#62BA46` · `#468337` · `#62BA46` |
| Graphite | `#8C8C8C` · `#747474` · `#747474` | `#8C8C8C` · `#747474` · `#8C8C8C` |

The dots in Look show each Mac colour as Apple draws it; the table is what
Ibis paints.
A chosen accent overrides the system's in Ibis only. Ibis offers no per-element
colours, fonts, radius, spacing, custom CSS or themes.

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
`--type-display-*` (20/26) for Settings page headings and the welcome. Dialog headings
(20/28) have no token yet.
Weights 400, 500 and 600; no uppercase transforms; italics only for "why it
matched". Monospace (`--mono`) only for citation keys and key-format
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
| `--tile` / `--icon` | 32 px / 20 px | Compact draws a 24 px tile with no fill, centred in the column. |
| Footer | 40 px | 24 px actions + 2S; Windows and GNOME add a 1 px top divider outside it. |
| Smallest target | 32 px, 44 px on touch | A smaller drawn control keeps the target with `::before`. |

**Rows** [C4]. Rows are 2 px apart in results and Recently added; choice screens keep their own 4 px (see choice screens).
Densities (decision 0020): Compact 40 px (one line), Comfortable 58 px (the default,
two lines) and Spacious 76 px (three lines). Density changes rows only, never
controls, targets, type or the header. The citation key is not shown in rows
(it stays in the accessible name and tooltip); the year (or "No date") is
trailing and sits level with the title's first line in every density, size
and view, so it never moves when rows change; in one-line Compact rows that
line is the row's centre. Spacious keeps the year there and its third line
reads only the type ("Article") (maintainer, 2026-10-30; one year position
everywhere, maintainer, 2026-11-01).

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
pictures, and any dragged size shows as Custom. A choice applies to the display
the picker opens on next, grows around the window's centre and keeps its top. **Compact** (maintainer,
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
The middle keeps a minimum of about one text line. In a short frame with
enlarged text, the header's vertical gaps and the footer hint row give way;
the shelf, footer and control targets retain their normal sizes. The keys
still work and More opens their reference. Long format labels shorten within
their row, preserving space for Pin. The shelf keeps its existing folded counts
and one-line form. Notices use the full middle content width and the
body-text size, wrapping naturally.

**Empty search: first time and every day** [C11]. Same frame, different
content. While no library is chosen, the welcome screen (**First run**). For
the first days with a library, the hint: the magnifier, "Type to find a
reference" and what to search by, centred in the middle area as a choice
screen without rows (same type and spacing). It lists no keys: the footer and
the keys list teach them, and no key is shown in two places. Once there is history, the middle area shows what people most
likely want: Cited here recently (up to three, for the app Ibis was opened
from, #45) then Recently added (three, #1070), under their headings, and the
hint shrinks to the search placeholder and the footer keys. These are ordinary
result rows (maintainer, 2026-10-30): same density, selection, Copy, add and
⌘-click; only the headings say they are not a search. Arrow keys move through
both groups as one list. The Compact bar shows Cited here recently only, as
Compact rows. Typing replaces these rows in place.

**Corners** [C15][D3]. Fixed CSS px at every text size; nested shapes are
concentric, and an inner corner never goes below the per-OS floor.
**Equal gaps, derived radii** (maintainer, 2026-10-30): where a shape sits
near its container's edge, the gap is the same on every side it shares with
that edge, and its radius is the container's radius minus that gap. Window 16
→ 8 px inset → footer action and shelf 8 → 4 px gap → key cap 4. The footer is
44 px (`--footer-h`: a 28 px action plus 8 above and below) so its actions sit
8 px from the window's bottom and side, like the shelf. A shape farther from
the edge than its container's radius keeps the floor. **Neighbours share a
radius** (maintainer, 2026-11-01, on corners close together): shapes at the
same depth side by side (footer actions and Copy, rows, menu items) use one
radius and one gap, so their corners line up. Every derived radius is computed
in the tokens from its container, never typed, so changing a root moves every
corner inside it. Check every new nested pair (key caps in hints, chips in menus, fields
in rows) against this before adding it. What the rule covers (settled
2026-10-30 after an audit of every nested shape):

- **Floors:** 6 px for surfaces (rows, tiles, fields, menu items); 4 px for
  key caps and other text-sized marks, which is why a key cap can be 4.
- **Exempt:** capsules and circles (radius is half the height: switches, chips,
  the Copied capsule, the shelf's × badge, dots) and the miniature pictures in
  Settings, which are the real picker scaled and keep its radii.
- **Badges** that straddle an edge (the shelf's ×) are circles; their 24 px
  target stays inside the window.
- **Parents:** a shape's container is the nearest surface it sits on. Menus,
  sheets and popovers float on their own layer, so they keep `--r-menu` and do
  not derive from the window; their items derive from them. Every overlay
  stays inside the window (`--inset`), the Compact bar included,
  opening upward or shrinking to fit rather than spilling out.
- **Fields** with a fill derive from their container with equal gaps: in the
  52 px header that means a 36 px field, 8 px from each edge, radius 8.

```
--r-row    = max(--r-floor, --r-win − --inset)          8  rows, fields, shelf, sheet
--r-action = --r-row                                    8  footer actions, Copy
--key-gap  = (--action-h − --key) / 2                   4
--r-key    = max(--r-floor-mark, --r-action − --key-gap) 4  every key cap
--r-mi     = max(--r-floor, --r-menu − --menu-pad)      7  menu items
--r-seg    = max(--r-floor-mark, --r-control − 2px)     6  segmented thumb
--r-tile   = --r-floor                                  6  row tiles (12 px in)
```

| Root token | macOS value |
|---|---|
| `--r-win` | 16 (every macOS version; the native panel mask) |
| `--r-menu` · `--menu-pad` | 12 · 5 |
| `--r-control` | 8 |
| `--r-card` | 10, Settings group cards (24 px in from the window, so not derived) |
| `--r-floor` · `--r-floor-mark` | 6 · 4 |
| Citation segment pill | `--r-floor` |

Switches, chips, toasts and capsules use `--r-full`. Settings page icons use
Apple's icon shape (22.5% of the size), like app icons. About Ibis shows the
Ibis app icon itself (`brand/exports/ibis-app-icon.svg`, the `-dark` file in
dark mode) at the same 20 px, on its own with no tile behind it, as Mac apps
mark their own entry.

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
uses `--text-disabled`, `--fill-5` and `aria-disabled` (menu items stay
focusable). Busy shows a 16 px spinner after 400 ms with `aria-busy`. Invalid
fields show words and an icon, never a red ring.

**Progress.** Known downloads show their percentage through completion. Unknown
totals and checking/installing use a moving segment without `aria-valuenow`. Reduced
motion replaces it with a static full-width dashed indicator, including in
forced colours; it never resembles a partially completed download.

**Selection and focus.**

| | Pointer over it | Chosen or current | Keyboard focus |
|---|---|---|---|
| Result row | Quiet tint | The selected-row fill | The same fill; never a ring |
| Menu item | The menu highlight | Check in a leading column on macOS and Windows, after the label on GNOME | The menu highlight; never a ring |
| Segmented choice | Quiet tint | Raised thumb on a recessed track | Ring around the group |
| Pop-up button, More | Quiet fill | Pressed fill while open | Ring |
| Buttons | One step darker | Primary: selection fill with `--selected-text` | Ring |
| Switch | No change | Accent track | Ring |
| Search box | None | None | None: the caret (Windows: its accent underline) |

- **Selection is a fill** [D7]. Result rows use the full accent with paired
  text on every OS (a tint reaches only about 1.2:1 beside other rows), no side
  pill. On the
  selected row an added tile is the paired text colour with a scarlet check
  [D9]. A kept-open picker without focus uses `--row-inactive-background`. Menu highlight: accent on macOS, a subtle fill on
  Windows and GNOME. Selected picture choices tint the whole choice with
  14% of the accent and ring the picture with a 2 px accent ring; the caption
  stays `--text`, semibold. This selection mark stays visible at rest. The separate keyboard focus ring draws farther
  inward, with a gap from the selection ring, only on keyboard focus. **Picture
  choices share their row** (maintainer, 2026-11-01): a group of picture
  choices (Appearance, Window size, Search icon) splits the group's full width
  into equal columns, one per choice, with each picture centred in its column
  and every caption on one baseline; pictures keep their true relative sizes
  (Window size) and are never cut (the Search icon field shows its whole
  placeholder). The ring sits on the picture, the tint on the whole choice. Rows keep
  their full accent fill; the full accent primary action remains one per view.
- **Focus ring only on keyboard focus** [D8] (`:focus-visible`), never after a
  click. **Controls** (buttons, switches, segmented controls, picture choices,
  fields) take it outside, as the Mac draws it; **rows** (results, choice rows,
  the Settings sidebar, chips' remove buttons) draw it inside (offset −2 px), so
  no scroll area clips it and it never covers a neighbour. macOS 2 px accent
  with a 1 px gap; Windows 2 px
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
`--menu-pad` (5 px macOS, the Mac's own), items padded 10 px on both sides, `--menu-item-height` high (24 px macOS, as AppKit draws it; 32 px Windows and GNOME),
separators inset 10 px with 5 px above and below (11 px in all on macOS), native open and close motion. A menu row's full width is its target, so the 32 px floor applies to standalone controls, not to menu rows.
Checks sit where each system puts them: on macOS and Windows a menu with any
choice or toggle keeps a leading check column for every row, so labels share one
start and shortcuts line up at the end; on GNOME the check follows the label.
More's toggles are `menuitemcheckbox` items; a menu without them starts at its
labels. **More holds actions, not preferences** (2026-10-30): Show in Zotero
or Open thesis.bib (⌘O); then Rows › (showing the current density, opening a
submenu of Compact, Comfortable and Spacious with "1 line", "2 lines", "3
lines" as their quiet examples), the one look you change while working; Keep
Window Open
and Show What Will Be Pasted (both have keys); then Keyboard Shortcuts (⌘/),
Settings… and Quit Ibis. Library choices live in the library menu behind the
footer status. Text size follows ⌘+ and ⌘− as in any Mac app; starting at
login, Forget formats and update checks live in Settings (Check for Updates
also in the menu bar menu). Each thing appears in one place: Keyboard
Shortcuts lives in More, not the footer. Sections keep
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

**Buttons.** Primary: `--selected` with `--selected-text`, one per view.
Secondary: `--fill-control` (plus the Windows stroke). Quiet: no fill, `--text-secondary`
text, subtle hover. Icon button: a 20 px glyph in a 32 px square whose tooltip
names the action and key; a toggle shows a filled glyph on the pressed fill,
never the accent. Buttons size to content (at least 64 px wide) and never hold a
key cap: they use quiet trailing text ("Cite this page ⌘↩") [C8].

**Pop-ups.** The format pop-up shows its value and a secondary indicator that
never turns (⌃⌄, a Fluent chevron, `pan-down-symbolic`). Space, Enter or ↓
opens it (also Alt+↓ off macOS); a pick or Escape returns to search, Tab moves
on. Named by what it sets and its value ("Copy as Pandoc"); items are
`menuitemradio`. Hidden until a library is chosen.

**Key caps.** Only in the footer and in lists of keys, never in menus, which
write shortcuts as plain trailing text [C8]. A 20 px fill (`--key`,
`--key-fill`), `--r-key`, label-type UI-font text, never an outline and never
scarlet. On macOS a shortcut is always one cap, written as menus write it (⌘K,
⇧⌘F, ⌘Space; the Return cap is the ↵ glyph). Inside a footer action the cap
keeps `--key-gap` on every side it shares with the action, so its corner is
concentric. On the action's hover or pressed fill the cap steps up one fill
(`--key-fill`), so it never sinks into the shape around it; on Copy it is a
tint of Copy's text (`--on-selected-fill`). Windows gives each key a cap 4 px apart (Ctrl
K); GNOME adds "+" between caps (Ctrl + K). Decorative caps are hidden from
screen readers; `aria-keyshortcuts` names the keys.

**Scroll areas.** One scroll area (`ui/ScrollArea`) for the results, menus,
Shortcuts and notices: the system's own scroll bars (overlay on macOS and
GNOME) and Fluent's thin bar on Windows, from `--scrollbar-width` and
`--scrollbar-color`. The side inset sits inside the bar, so a classic bar never
takes it. An edge with more content beyond it fades over `--scroll-fade`,
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

**Capitalisation** (maintainer, 2026-11-01: consistent everywhere, from
platform guidance). On macOS, menu items, buttons, window and sheet titles use
title-style capitalisation, as Apple's guidelines ask: "Keep Window Open",
"Show in Zotero", "Remove from Citation", "Clear Search", "Keyboard
Shortcuts". Everything else uses sentence case: choice-screen rows ("Try
sample references", "Use another library"), descriptions, the keys list's
action descriptions, notes, status, settings labels and headings. One string
never appears in both styles; a keys-list line describes an action and stays
sentence case even when it names a menu item.

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
the first line starts, a trailing control stays centred on the
block (a result's year follows Rows above), and in Settings the control moves below the label rather than
squeezing it. In results, a title wraps to at most two lines and then ends in
an ellipsis; the byline stays one line and shortens authors first ("Okonkwo,
Haddad et al."), then the journal; the year never shrinks and sits level with
the title's first line. **Hover card** (maintainer, 2026-10-30): resting the
pointer half a second on a row whose text is cut shows a card beside it, on
the menus' denser glass and at most 320 px wide, with the full title, all
authors, journal, year and type. It never takes focus, leaves with the
pointer, and never appears for rows that fit. From the keyboard, ⌘Y (Quick
Look's key) opens Peek, the same details, for the selected row. Making a window or column wider is never the fix. Check every
row, menu item and Settings row at normal, Larger and 200% text.

**The Ibis mark in the picker.** The bird from `brand/`, eye included, in the
brand gradient for the theme (light `#CF4628`→`#B5331A`, dark
`#FF7F5E`→`#F2552F`). Increase Contrast and forced colours draw the same bird
flat in `CanvasText`, with its eye kept open and no gradient, rim or shadow.
Normal modes never use a plain recoloured silhouette. In the search box it
is 24 px, 2 px outset on each side so the query and results stay aligned; its
eye opens to at least 2 px across at small sizes so it still reads as a bird.
At 56 px it leads the welcome, at 40 px the opening screen. The empty-search hint shows the
magnifier, the sign for "type", whichever search icon is chosen. The mark never
appears on a control.

**Icons.** Ibis's own drawings on every platform: 20 px grid, 1.5 px rounded
stroke (1.6 px macOS, 1 px Windows, 16 px symbolic `ibis-…-symbolic` on GNOME);
"on" is the filled form. Each icon is **one path** from one icon source
(maintainer, 2026-11-01): an outline icon is a single stroked path, and a
filled icon is a single filled path with no stroke on top. Apple's label
colours are translucent, so two overlapping strokes, or a stroke over a fill,
paint twice and show a brighter seam; a test rejects icons with more than one
shape. Brand marks from `brand/` are opaque and exempt. Seven type icons (article, book, chapter, thesis, web
page, report, other) never reuse an attachment drawing. Platform icons only
where the platform owns the meaning.

**Footer and status** [C8]. Status dot and text at the leading edge, then
"More ⌘K" (quiet), then, last, on the rows' right edge, "Paste into Word · APA ↵"
or "Copy N as <format> ↵": the one primary action, filled with `--selected`
with a `--selected-text` label and key cap, because mouse users copy far more
often than they open anything else (maintainer, 2026-10-30; a Mac puts the
default button last). Every hint is label first, then keys: 8 px between them,
16 px between hints, label type on one baseline. Actions are quiet 24 px fills,
radius `--r-tile`, with 32 px targets; they invoke the same guarded actions as
the keys and sit in one group named "Keys". With Alt held: "Add to citation ⇧↵",
the Escape step ("Clear Search Esc" or "Hide Esc"), then More. When the footer
does not fit, shorten the status first, then drop the hint labels to keys
only. The status dot is `--status-dot`, at the footer's leading edge
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
| Update failed, old copy kept | "Zotero not updated · 2 h", then the fix button | Stop |
| Never opened | "thesis.bib · not opened" | Stop |

Reference counts include bibliographic records only, across the footer, library
status and per-source counts. Attachments, notes and other library objects do
not add references.

"Updated" means the references changed; "checked" means Ibis read them. The
footer always names the library searched; its tooltip and the top of More add
libraries, check times and problems. **One place for each status**
(maintainer, 2026-10-30): the footer's leading status is the only place
library state lives. Library problems stay there, so they never push the list
down; citation refusals and settings notices use the middle scroller. The status
is a button: it opens the
**library menu**: the library with its count and age; then, when Zotero has
more than one library, **Search in** (maintainer, 2026-11-01, like Finder and
Mail): a heading, All Libraries first, then one menu row per library with a
check in the check column, never chips. Ticking several rows searches all of
them. A long name shortens in the middle, with the full name in its tooltip;
past eight rows the list scrolls inside the menu. A library that could not be
loaded stays in the list, dimmed, with "Couldn't load" as its trailing text and
the reason in its tooltip (later: needs the engine to report each library's
failure). These problems live here, never in the footer, so the footer stays
one line. Last, Choose Library… ⇧⌘O (the Mac's "Open…": pick a
.bib file or a Zotero library) and Reload Library, the menu's only reload. **Open the source** (maintainer, 2026-10-30): ⌘O (Ctrl+O)
works on every screen and opens where the references come from: the selected
reference in Zotero (Zotero's own `zotero://select` link, starting Zotero if
it is closed), or the .bib file in its default app. **The words always name
the target** (maintainer, 2026-11-01), never a bare "Open": **Show in Zotero**
means this reference, selected inside Zotero; **Open thesis.bib** means the
whole file, whether or not a reference is selected, because editors cannot be
trusted to jump to an entry; **Open Zotero** means the app alone and appears
only when no reference is involved (nothing selected, a choice screen, the
status fix). ⌘O always does what More's first item says at that moment. More lists it first as
"Show in Zotero" or "Open thesis.bib"; a problem's fix and a choice screen's
row use the same words and show ⌘O. Ibis shows the system's own icon for the
source app or file at runtime and never ships another app's logo. With
several libraries ⌘O follows the selected reference, else the library
searched. With nothing selected, or on a choice screen, it opens Zotero
itself. Holding ⌥ turns "Open thesis.bib" into "Show in Finder". When the
source can't open (Zotero not installed, the file gone) the same place offers
Choose Library… or Find…, never a key that does nothing.
A problem shows there in a few words with a qualified age ("Zotero not
updated · 2 h"; the full sentence in its tooltip) and one fix: "Open Zotero",
"Find thesis.bib…". The fix shows ⌘O only when ⌘O does the same thing at
that moment; with a reference selected ⌘O means Show in Zotero, so the fix
shows no keys. **The fix is a button, not link text** (maintainer,
2026-10-30): the footer's one quiet action with a resting `--fill-control`, led by the
source's 16 px system icon and followed by its keys, right after the status.
It is never accent text, so Copy stays the only accent in the footer. "2 h" is
the time since the last successful read. Background work (reading a large library, downloading an
update) shows as a small progress ring before the words, never a bar across
the window. When nothing usable is left, the middle area shows the choice
screen instead.
While references are usable nothing else interrupts. Names and ages are isolated for RTL; the
age ticks every 30 s outside the live region.

**Welcome.** A choice tile's icon is centred on the tile's text block, never top-aligned; glyphs are optically centred in their tile.

**One pipeline.** The canvas is the design and `apps/desktop/src/tokens.css` carries its token names and values. A change starts on the canvas, then goes into `tokens.css` or the part's own stylesheet in `ui/`; a screen never styles a shared part. `design.test.ts` keeps this honest: every token read is defined, every token is read, parts take colours, corners and type sizes from tokens, no one-sided edge marks state, and text keeps its contrast. `apps/desktop/gallery/` screenshots every state in light and dark, and the browser probes check every layout, so a change is judged by its picture, not its source. Text never carries a system tooltip: what is cut off is whole in its menu.

**One foundation.** Rules every window shares live in `ui/Foundation.css`: one thin scrollbar, and icons that take their row's colour, so hover and selection cannot strand them. An overlay keeps 16 px from its edge to its text, its rows reach 8 px from the edge, and a status line is words, never a filled band. Short windows drop a row's second line rather than losing the menu (maintainer, 2026-10-31). Check a visual change against the screenshots from `apps/desktop/gallery/`.
**The picker settles before it appears.** The search reset and the paste target are ready before the window shows, so its first frame is already current. While typing, the shelf holds the last citation through a running search and keeps one height; it never blanks between keystrokes (maintainer, 2026-10-31).

**Notices.** No notice or alert carries a side bar or one-sided border: its words and its place say what it is (maintainer, 2026-10-31). "Copied" or "Inserted" shows after the picker hides, in a window
that never takes focus: the same card near the top centre on every system. In-window toasts ("Removed · Undo") last 8 s and pause
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
(meta type, `--text-secondary`, up to three rows), then one suggested action 16 px below;
quieter text buttons for alternatives. No skeletons or illustrations; the opening screen
only after 400 ms (**Opening a library**). The result heading appears only
above rows.

| Case | Title | Line | Below |
|---|---|---|---|
| No matches | No matching references | Try a surname, year or fewer words. | Clear Search; a quieter Search All Libraries when some are left out |
| Loading or reloading | Opening thesis.bib… / Reloading thesis.bib… | None | Nothing for 400 ms, then the opening screen; retained rows stay |

**Shelf** [C7]. What Enter pastes, one 40 px line above the footer's Copy
action: the beak mark hangs at the tile column's end and the citation starts on
the title column. Shown while there are results or a citation is being built,
including through empty searches and refresh failures; hidden on choice
screens. Results never move when a reference is added. With several
references the citation itself holds them as segments (no chips line): each
reserves 1 px on each side; under the pointer it becomes a soft pill (radius 6),
so nothing moves, no control ever covers its text and neighbours are never
covered (also for zero-width separators). The citation shows no system tooltip:
the whole text is in the reference's menu. From the keyboard ← → select a
segment as an accent-filled token and Delete removes it. On copy
(maintainer, 2026-10-30) the shelf shows a closing beak that is the opening
one turned 180°, the same size and the same 12 px from the citation, so the
pair is symmetric; a capsule "Copied" (24 px, radius 12, `--done` on
14% of itself) with a check that draws in 200 ms; and a 9% green wash over the
shelf for 160 ms. Reduce Motion shows the end state.

**One line, details on demand** (maintainer, 2026-10-30). The shelf is always
one line: no Page field or form choices beneath it. Clicking a segment (or
Return or Space on a selected one) opens its menu. It is an ordinary menu,
the same overlay as More and Copy as: the same glass, `--r-menu`, shadow, row
height, check column and type, with no extra border, card or field chrome
(maintainer, 2026-11-01). Every label starts at the check column. In order:
the reference's short name as a heading; **Name in brackets**, **Name in the
sentence** and **Year only**, each with a quiet example of what it pastes in
the current style; **Page…** with the current page or "None" trailing,
editing in place when chosen; then the source action with the same words and
keys as More ("Show in Zotero ⌘O" or "Open thesis.bib ⌘O"); then **Remove
from Citation ⌫**. With one reference the
whole citation is its segment. Removing needs no click into the menu
(maintainer, 2026-10-30): a pointed segment shows a 16 px round × badge in a
24 px target on its top-end corner, outside the text and the pill, so the key
is never covered;
clicking it removes the reference with an Undo notice. The badge never shows
for keyboard focus or while the segment's menu is open (⌫ removes there). Pages also come from typing (`garcia 2021 p12`
shows a page token beside search) and, from 0.5, the PDF pane's "Cite this
page", where the Page field lives beside the page itself.

- **Text.** Text colours; key formats monospace with punctuation in `--text-secondary`;
  formatted styles in the UI font. A read-only `<bdi dir="auto">` line with an
  ellipsis; the full text is in the tooltip and the wide pane.
  The selected citation's Preview region stays a keyboard focus target;
  hiding or removing it returns its focus to Search.
- **Page and form** (#1366). A reference's menu holds its Page field (a page
  or range, including Roman numerals) and the three name forms. Opening the
  menu from a selected segment puts focus in Page. With no built selection,
  the same menu on the highlighted result edits it; the first edit adds that
  result to the shelf; subsequent searches cannot change its page or form.
  With a pasted list, the first edit adds the complete list to the selection
  in paste order before changing that reference's page or form.
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
  The full refusal wraps in the middle notice area before Details is opened,
  with no line clamp or separate scroller. Each new refusal, even one with
  repeated wording, instantly reveals its start in the middle scroller after
  layout without taking focus; subsequent scrolling can reach its entire text while
  the shelf band stays one line. Automatic result highlighting and layout
  reflow preserve the refusal's reading position; explicit arrow navigation
  can reveal a newly highlighted result.
  Source-field and sentence-case warnings name the reference and appear in both
  preview details and copy feedback. A copy carrying either warning stays open
  with its warnings until dismissed; it never becomes a clean-copy receipt.
  Mixed narrative clusters, other locator controls, prefixes and suffixes
  remain in #24/#1315. The menu keeps the shared keyboard focus and touch
  targets. A form's whole row, including its label and example, is clickable;
  its native radio stays inside that row with the same pointer target even
  when it is outside the group's Tab sequence.
- **Year-only citations.** Keep supported style forms available through conditional
  layouts. Each selected item's output must show its issued year, or the style's
  own no-date term for an undated or zero-year item; otherwise refuse the citation.
  A matching phrase in a title or locator, or another item's year, does not
  establish a year-only result.
- **Beak mark** (#888). The supplied two-head SVG in `currentColor` from
  `--scarlet`, 20 px tall (`--space-5`), scaled with text, `aria-hidden`,
  mirrored in RTL, 45 % while pending, `CanvasText` in contrast modes [C16].
- **States.** Pending: the old text dims for 300 ms, then "Formatting…"; Enter
  waits. Resolving the destination can refresh recents; a waiting Enter keeps
  its reference through that read and cancels if it changes or cannot be read.
  Failed: a Stop icon, "Couldn't format in APA. Choose another style, or
  copy keys.", no Enter hint. Stale: a Look icon, "The library changed. Reading
  it again…". Warnings: a trailing pill ("1 warning"; "Reference entry" for a numeric
  style, which copies the entry because its number depends on the document).
- **Show What Will Be Pasted.** On by default (`show_paste_preview`, decision 0020),
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
| Esc | Steps back one layer: menu or dialog, preview, then clears the search and filters (the shelf stays), then hides. No extra clear key: ⌘⌫ also clears typed text, as in any Mac text field |
| ⌘O / Ctrl+O | Opens the source: the selected reference in Zotero, or the .bib file |
| ⇧⌘O / Ctrl+Shift+O | Choose Library…: open another .bib file or Zotero library |
| ↑ ↓ (focus stays in search) | Move the selected row (`aria-activedescendant`) |
| ⌘K / Ctrl+K | More, the one door to every other action, each with its shortcut |
| ⌘/ / Ctrl+/, or `?` outside a text field | The keyboard shortcuts list |
| ⇧⌘P / Ctrl+Shift+P | Keep window open (the pin) [C6] |
| ⌘I / Ctrl+I | Show or hide what will be pasted |
| ⌘Y / Ctrl+Y, or the Peek control on the focused row | Shows the reference's full details; arrow and text-editing keys in search keep their usual meaning |
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
| Accent [C14] | The accent chosen in Look; with "Follow the Mac", the system accent where the webview reads it, else the `--accent` fallback in `tokens.css`. Each accent pairs with a `--selected-text` that keeps 4.5:1 on `--selected`. |
| Selected row [C4] | Accent fill with its contrast-safe paired selected text. |
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
  (maintainer, 2026-10-30, revised the same day): menus, the keys sheet,
  dialogs and the shelf's reference menu use the panel's own glass, never a
  flat opaque block, but denser (92% for menus, 96% for sheets, with the
  panel's blur) so the rows behind them read as soft colour, never as shapes
  or words competing with the text in front. Reduced transparency makes them
  opaque. The keys list is the same overlay as a menu (radius `--r-menu`, the
same glass, edge and shadow), at most 600 px wide, in two columns of groups,
centred in the window since no single control owns it.
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

Feedback, panel, layout and exit durations come from `--dur`, `--dur-panel` and `--dur-exit`; layout is
the 280 ms hard cap. Entrances use the look's `--ease`, exits `--ease-in`. No
bounce. Windows menus slide 8 px and fade; macOS and GNOME menus appear at
once. Chevrons never rotate. Reduced motion: no translate, scale or slide,
fades of at most 100 ms. Never animated: list content while typing, row height,
the caret, layout on hover, the citation text, ticking counts, focus rings,
anything over 280 ms, anything that loops except a busy indicator.

## Small delights

Proposed 2026-11-01; the canvas's Small delights board lists them. Every delight
lives only in About, the welcome, status lines or feedback that already exists;
never changes search results, what is copied or pasted, or focus; happens once,
quietly, inside the motion cap; shows its end state under Reduce Motion; never
loops or plays a sound; and is counted only on this Mac.

- **Feel:** a pressed shortcut gives its footer key cap the pressed fill for
  100 ms; the menu bar mark dips its beak once after a paste or copy; a haptic
  alignment tick when a citation segment or the window snaps into place; holding
  ⌘ for one second shows every visible action's key.
- **Saves time:** Services › Search with Ibis from selected text; a `.bib` or
  CSL-JSON file dropped on the Dock icon becomes a library; ⌥ on About shows
  the full version, build and date, and a click copies it; one next-step tip
  after the first paste, shown once.
- **Fun:** clicking the About icon makes the ibis blink once, and five clicks
  show "Ancient Egyptians linked the ibis with Thoth, god of writing."; the
  mark pecks once when a library first finishes opening; a library's status line
  notes its first 1,000 and 10,000 references once.
- **Left out:** confetti, sounds, seasonal themes and joke search results.

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
  centre line, the upper third and the four corners within 18 px (Option or Alt
  places freely). Drag an edge to resize [C10]. Place and size are remembered per
  display. **Reset Window Position** in the menu bar menu and Settings › Advanced
  restores the default place and size.
- **Corners** (maintainer, 2026-10-30). Settings › Advanced › **Where Ibis
  opens** is one small picture of the screen with five places: Centre (the
  default above) and the four corners. A corner sits 16 px from the work area:
  below the menu bar, above the Dock, in from the screen edge, the same gap on
  both sides. Picking a place and dragging to it set the same remembered place,
  so Reset Window Position undoes either. In a bottom corner the full window
  opens, never the Compact bar, because the bar would grow upward and move the
  search box.
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
An earlier source search or found-file request reports nothing after it is
superseded, leaving the newer explanation visible.

Choice screens are one calm group centred in the steady frame
(maintainer, 2026-10-30): the title (17 px) and line centred, 6 px apart, then
24 px to the rows in a column at most 540 px wide, rows 4 px apart, any note
16 px below them. On the welcome screen the mark sits above
the title. Space is shared above and below, never left as one empty band.

**The welcome screen teaches the basics** (maintainer, 2026-10-30: first
impressions matter). Its line says what Ibis does and promises safety ("Ibis
finds a reference and pastes its citation where you're typing. It only reads
your library, never changes it."), and its note says how to come back ("After
this, open Ibis from any app with ⇧⌘Y." by default). It names the current
registered shortcut in the platform's notation or the desktop's exact trigger
text. With no reported trigger it says **No shortcut is set** and shows the
desktop's guidance or how to open Ibis from the menu bar or tray. A reported
trigger that is not registered shows its unavailable guidance; a pending or
failed status read never promises a shortcut. The welcome is the whole window:
with nothing to search yet there is no search header. The footer stays;
Escape dismisses any visible menu or sheet, then hides Ibis even when a query
is retained from samples or another library. Dismissed sheets return focus to a
visible control, never the hidden search box. A 56 px mark and a 20 px
title lead. The four starts (Zotero, a .bib
file, Find them on this computer, Try sample references) are a 2 × 2 grid of
tiles, 8 px apart and at most 600 px wide, each a tile icon, a title and one
short line on a quiet fill. Tiles in a row share one height; each icon is
centred on its tile's text block, never top-aligned, and each glyph is
optically centred in its square; every start has its own glyph (maintainer,
2026-11-01). The whole group fits the Medium window with
room above and below. Arrow keys move in two dimensions. Once
found, a library replaces the Find tile ("Use example.bib", "Found in
Documents"). A choice screen with two or three rows stays a list.

**Opening a library.** Nothing for 400 ms, then the same centred group: the
40 px mark, "Opening example.bib…", one quiet line ("Reading your library.
Zotero can stay closed."), and a thin 160 px progress bar (determinate once the
count is known). With Reduce Motion the bar is still and the mark does not
breathe. Rows kept from before stay instead, with only the footer saying
"Reloading…".

- **No library:** "Where do your references live?": the welcome grid above
  (Zotero, BibTeX, BibLaTeX or CSL-JSON file, "Find them on this computer",
  samples). Nothing is searched until Find is chosen; a found library takes
  its tile, and when several are found the tile opens a short list of them. Zotero refusing other
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
  name. ⇧⌘F (Ctrl+Shift+F elsewhere) opens the chooser from anywhere in the picker and closes it again; Escape or a pick returns to search with the query kept. Tab keeps its usual meaning. Items read as plain names with the output as a quiet example ("BibLaTeX brackets   \parencite{key}"), never three rows with the same name; the menu is at least 300 px wide, widens for longer labels (translations, Larger text) and wraps only at the viewport limit, as every menu does, and ends on a whole row. A format
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
  or a format pick while its preview or save is still pending.
  The first Enter in an unrecognised app with a proven paste target opens this
  same chooser, with **Paste into [app name] from now on** and **Copy only**.
  Either answer saves the displayed format and a separate delivery choice for
  that app ID; "Just this once" keeps its output temporary and unconsumed.
  It returns to Search with the citation kept and waits for a fresh
  Enter; answering, picking a format or dismissing this question never replays
  the held Enter. A format pick alone cannot approve pasting. A failed save
  leaves the app unapproved and asks again. Copy only skips the question on
  later openings and says "Copied. Copy only is set for this app".
  The chooser also offers these delivery choices for ordinary recognised apps.
  More offers **Copy only in [app name]** for the current ordinary app too.
  "Automatic" resets both choices for this app; "Forget formats for apps"
  resets both for every app and brings the unknown-app question back.
  An unlisted app at a different location asks again even when its app ID is
  unchanged. Only an opaque app-identity fingerprint is remembered.
  Unreadable app choices use automatic formats but block automatic paste with
  `paste-unavailable`. Saving a format cannot lift this refusal. A fresh explicit
  Paste or Copy only choice resolves it for that app; other apps keep the
  refusal, including after restart, until their own choice or Forget formats
  for apps. The chooser still offers delivery choices for a proven ordinary
  target during recovery. Uncertainty cannot discard a remembered Copy only rule.
  Refused terminals, remote viewers, virtual machines and editors with a
  different paste key never ask and cannot be approved. Without a proven
  target, Enter copies with its refusal reason. App capture currently works on macOS;
  Windows and Linux keep their existing global output until capture is built.
  Ibis never reads or stores a document title or URL to choose an
  output. No format question interrupts each citation. Setup steps finish
  themselves: when Zotero is not sharing, the screen shows where to turn it on and continues on
  its own once it is on; when Zotero is closed, Ibis reads the library anyway;
  with several Zotero profiles, Ibis asks which one; with no library at all,
  built-in sample references let people try Ibis, marked once, by the footer
  status "Sample references"; rows carry no extra label. The explicit
  **Try sample references** choice uses three bundled invented CSL-JSON
  references for this run; it never saves a
  library preference. The website demo and help screenshots use this same set
  from `apps/desktop/src-tauri/resources/samples.csljson`: lantern gardens,
  clockwork clouds and the invented sea. The practice task is identical on
  every surface: Search for “lantern”, choose the lantern gardens reference, then press Enter. A
  **Use your own library** choice row returns to setup. Per maintainer feedback
  on 2026-10-30, website demo rows show plain titles, including static copy and
  accessible labels. The website demo copies only inside its own page, so its
  citations carry no [Sample] suffix. Confirm each action once, in one place:
  the shelf's Copied capsule, never a second notice for the same copy.
  Choosing a real source replaces samples; sample selections and copy notices
  leave with them. Samples never join a real library. Every desktop sample citation
  ends with the visible text ` [Sample]`, including key outputs and every
  clipboard representation (plain text, HTML and RTF). The marker belongs to
  the prepared citation and its content hash: previews and receipts show the
  same marked text that lands in the document. These are demo citations;
  choosing a real library produces ordinary citations. The
  footer, receipt and announcement describe the action actually taken:
  "Paste into Word", "Copied. Press ⌘V to paste." (the right key per system)
  when pasting is off or unavailable, or "Copied" with the reason. The footer
  shows the library, More and the primary Copy or Paste action; Shortcuts lives
  in More.

### Everyday details (rebuild, 2026-10-26)

Ranked by how much they help a first-time user; each is built with the release
that owns its area in ROADMAP.md.

1. Output per app, remembered (above).
2. Setup that finishes itself (above); a permission detour keeps the prepared
   citation, notices the grant and waits for a fresh Enter. The two grants are
   named apart: "read your references" and "paste".
   References, locators, search and the prepared output survive the detour,
   including longer than the usual minute away. On return after a read-back
   grant, the guide closes. The person returns to the external document and
   summons Ibis again to capture the destination; no earlier Enter is replayed.
   Its prepared output applies once after a fresh Enter, even
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
   replace its shelf or current copy feedback. A changed delivery choice revokes
   outstanding paste permission. If a completed write's response arrives after
   that choice with no newer interaction, the open picker
   still confirms Copied; it neither hides nor replays delivery. A newer Enter,
   including one waiting for the save, keeps ownership. A changed query,
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
   Failed presentation reads after reopening get one recovery attempt. If that
   also fails, keep the citation in place, cancel waiting Enter and show the
   existing "Nothing was copied" alert. Becoming ready later does not replay
   that failed action; a fresh Enter can retry.
   Automatic paste uses a recognised app's normal paste shortcut, or explicit
   per-app paste approval from the chooser above. Unapproved unknown apps
   cannot paste; a copy that cannot ask says "Copied. Automatic paste is not
   approved for this app". A format choice never grants paste approval, and no
   choice overrides a refused app or the native target/permission checks.
   Approval is frozen before the clipboard write, so a later answer cannot
   authorize an earlier copy. Changing delivery choices revokes outstanding
   proofs; fresh Enter cannot join a copy from an earlier delivery policy.
   Saving Copy only cannot send an older paste. A proven target that changes,
   closes or is minimised copies with "Copied. The target app or window changed".
   An unavailable focus or safety proof uses `paste-unavailable` and the system's
   manual-paste key; it never claims that the target changed or the paste expired.
   A replaced clipboard says "Copied. The clipboard changed; copy the citation again".
   On macOS, sleep, lock and session changes expire pending delivery. Expired
   deliveries copy with "Copied. The paste expired; paste it yourself".
   A refusal never retries or replaces another clipboard owner's contents.
   No window title or document text is read. Window count and front-to-back
   order cannot establish keyboard focus, including with nonactivating panels.
   On macOS, Enter uses the captured document and top-level input surface,
   including when several windows of an app are open. Attached sheets/drawers
   have their own surface identity; opening a Save sheet cannot borrow its
   parent document's paste proof. An identity-only AX check requires
   Accessibility access as well as the separate PostEvent authorization
   (decision 0029, maintainer 2026-10-30 and #1529 review).
   It verifies global keyboard ownership as well as the active app, so a foreign
   nonactivating panel cannot borrow that app's target. Missing Accessibility
   copies with "Copied. Ibis needs Accessibility access to paste safely. Press
   ⌘V to paste." Failed/unsupported reads and the bounded 50 ms query timeout
   copy with `paste-unavailable`. Granting access never retries the old paste;
   return to the document, open Ibis again and press Enter to try again.
   No field roles, values or secure-field attributes, window titles or document
   text are read. The final comparison is followed immediately by
   process-directed posting, without a
   lock; a small OS focus-change gap remains and is not atomic window binding.
   Showing the picker again while it owns the keyboard keeps the original
   external destination.
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

### Source checks and passage reuse (planned)

The five source workflows in ROADMAP.md were approved on 2026-10-30; this section
is planned behaviour. Research on 2026-10-30 refined the limits below before
implementation. It adds no new notebook, source writer, cloud access or agent
permission.

**Check the bibliography.** This answers “Does the exact key I am about to insert
occur in the file I chose?” It is optional for key workflows, with no setup for
formatted references in Word or Google Docs. Start with one explicitly chosen
read-only `.bib`; name that file, not “the whole document”. Multiple-file or
Overleaf ZIP support must expose the checked file set rather than infer which
bibliographies a document uses. A downloaded copy is a dated snapshot, not a
connection to the online project.

| Checked-file state | What the person sees |
| --- | --- |
| One exact occurrence, no known identity conflict | In thesis.bib; inspect the project entry |
| No occurrence after a complete coherent scan | Missing from thesis.bib; copy an available entry or re-export from the manager |
| Unreadable, incomplete, changing or uncertain read | Couldn't check thesis.bib; retry or find the file |
| Several occurrences of the key | Key appears more than once; inspect its locations |
| Key exists but bibliographic identifiers or fields conflict | Key may name another reference; compare the entries |

Presence is not proof of matching metadata, the intended edition, a current
manager export or successful compilation. Do not silently replace a key with a
similar record's key. The check is informational; warnings follow the existing
Copy-with-reason policy instead of suggesting a verified automatic paste. Existing
engine key-conflict refusals still apply. An unknown result is never “missing”.

Keep the checked bibliography separate from loaded search libraries: loading a
Zotero export again would create artificial duplicate key ownership. Retain exact
key occurrences and scan diagnostics; successfully converted records alone cannot
prove absence or uniqueness. Bind reads, status and any prepared action to the
chosen file/context generation; a delayed earlier file cannot replace a newer one.
Refresh uses bounded existing source-reading rules, without hidden polling.

**Copy bibliography entry** retains the existing key and never edits a source.
A raw entry may need `@string`, `crossref` or `xdata` definitions. Supply a validated
self-contained entry/dependency closure or a qualified maintained export in the
chosen BibTeX/BibLaTeX dialect; do not silently recreate it from reduced CSL data.
When unavailable, explain how to export it from the source manager. Zotero-only
export needs version/key-parity qualification; documented API support is not proof
that every supported Zotero/Better BibTeX configuration emits the intended key.

**Find a passage.** Search existing Zotero highlights, notes and personal comments.
Label a result Highlight, Comment or Note; show matched text, separate comment,
parent reference when known, and location. A note can combine several papers or
have no parent: never treat its text as a quotation from a guessed reference.
Image/ink and comment-only annotations have “No text quotation”; no silent OCR or
fabricated excerpt. Source-adapter support and open/closed parity precede picker
wiring, with supported Zotero versions and library kinds stated explicitly.

**Open passage** resolves the annotation, attachment and library identities in the
engine. Prefer a verified Zotero annotation target; label page-only opening as
page-only. External-reader exact-page support needs its own qualification. A
successful app launch alone does not prove the intended passage was reached.
Distinguish missing, not-downloaded, cloud-placeholder, unreadable and unknown
attachments. Changed/deleted attachments or annotations invalidate prepared
locations; delayed work and close/reopen cannot restore obsolete targets.

Show physical and printed pages separately, e.g. “PDF page 8 · cite p. 417”. Unknown
printed labels show “Citation page unknown” and are omitted from output. Implement
an explicit validated unknown-page quote/export path; never bypass source/annotation
validation or substitute a physical index for a citation page.

**Copy quote with reference** is an explicit action for a text quotation. Preserve
its stored text and paragraph meaning, accurate printed page when known, reference
and source link as plain text or escaped Markdown. Keep personal comments separate;
never silently summarize, truncate or turn note HTML into trusted markup. The quote
is the saved annotation text, not a claim of verification against the current PDF.
Reuse engine identity, escaping and final source-validation contracts; quotation
and comment output must not use CSL note fields.

**Export evidence brief.** Select several passages in order, optionally enter a
question, and export one Markdown file containing quotations, separate comments,
references, pages and source links. Two passages from the same paper retain their
own locations. Include only chosen material, preserving each source relationship.
Label local Zotero links apart from public identifier links; neither exports private
attachments. Never include private paths or bypass the existing link policy.
Default to a new chosen output file, never a source library/export or an alias of
one. Atomic publication preserves any prior destination on failure/cancellation;
keep the selection for retry, and reject partial or obsolete output. This export
creates no persistent notebook or hosted share.

**Changed copied key.** Show “You previously copied oldKey; this item's current
key is newKey”. Offer Copy old/new keys, Open reference and Forget remembered key.
This is informational: an old project export or alias may still accept the old key.
Track only sources whose native identity survives the rename: Zotero, or CSL-JSON
with a stable `id` and separate changing `citation-key`. Plain `.bib` renames change
the current adapter's identity; do not infer continuity from a title or DOI.

Remember the actual emitted key after successful native clipboard publication,
including a copied fallback when paste is unavailable. Previews, failed copies,
formatted-only output, brief exports, Copy old/new keys, CLI stdout and MCP
formatting do not count. Copying the diagnostic explanation leaves the remembered
baseline and warning intact.
Several keys update as one successful-copy batch; older asynchronous storage cannot
overwrite a newer successful copy. Copying the new key replaces the remembered
baseline; it does not repair old documents or produce an “all clear” claim. Deleted
items, conflicting keys and failed refreshes keep their specific meanings. Planned
storage limits are in decision 0016.

These features retain the current agent-data boundary: MCP does not gain notes,
annotations or private links. Decision memory, resume workflows and an AI
companion remain deferred.

## Settings

[C18] Its own window (Dock, taskbar and Alt+Tab while open), opened with
**Settings…** (⌘, / Ctrl+,) in More and the menu bar or tray. A sidebar of
pages runs under the window buttons, as in System Settings; each page is a
stack of titled groups of rows: a label and optional description, then one
control at the end, at least 48 px, or 56 px with a description, on
`--grouped-card` over `--grouped` [D17]. Controls are the
shared `Switch` (38 × 22), `Segmented` and shortcut recorder in `ui/`. Changes
apply at once and reach an open picker; preference resets offer Undo. Built so
far: five searchable pages, with General (opening-shortcut recorder, login and
pasting), Look (real picker pictures, rows, Ibis mark or magnifier,
Default/Larger text and paste preview), Shortcuts (opening shortcut and an
always-open grouped key list and Undo for restoring the default), Citations (default
key syntax or style through the shared Ibis menu) and About Ibis (version and
update checks). Appearance and Accent apply in both Settings and the picker
through the same root-level roles. Unfinished controls, choices, rows and their
sentences stay out of Settings and its search index: no disabled promises or
"Coming soon" placeholders (maintainer, 2026-10-31). This includes menu-bar
visibility, welcome replay, window presets, None, Largest, and the unfinished
Libraries, Citations, Advanced and About actions. A page with no built controls
stays out of navigation. Ordinary Settings presentations open
General at the top; explicit page requests select that page and reset its scroller.
The shared shell uses the root's full height at Default, Larger and 200% text.
The page and navigation scrollers contain absolutely positioned descendants,
so focusing a control never scrolls the document or moves the window chrome.
General and Shortcuts share `ui/ShortcutSetting`: the same real recorder,
recording prompt, refusal and read-error messages beside the cause, default
reset and eight-second Undo. General edits in place; it never sends the person
to Shortcuts to record. Both consume the one Settings shortcut owner and save
queue, with no second subscription, persisted state or migration. Escape, Tab,
blur and leaving the page cancel recording; pending writes remain owned by
Settings across page changes. Desktop-owned triggers stay verbatim, or Not
assigned, with the desktop's guidance; Change opens the desktop configuration
only where supported, on either page (maintainer, 2026-10-31).
Preference notifications retire older replies immediately and refresh after
the queue actually completes; only closing uses the two-second limit. Both
windows request a ticket from one synchronous native counter when a choice is
made, before waiting for earlier saves. The locked writer compares the last
successfully written ticket per field and returns current saved values when
an older field is dropped. Retries keep their original tickets; unused tickets
block nothing and unissued tickets are rejected.
Notifications carry the publishing write's ticket. A self-notification keeps
that write's successful acknowledgement available for rollback, including
when the next save and recovery reads fail; other writes still retire stale replies.
One preference state owns the shown values and explanation: known saved value,
save refused, or saved value unknown. Any other window's notification moves it
to unknown until a real read succeeds, retiring an earlier refusal's saved-value
claim immediately. A failed read while unknown shows the read-uncertainty error;
edits and save acknowledgements cannot clear it. A refusal says the saved value
is shown only while that value is known.

**One Settings design** (maintainer, 2026-10-30). There is one Settings
window; every drawing and board uses it, and it is kept in step with the app as
both change. The Rows and Search icon pictures are the real picker drawn small
(sample references, the current appearance), never a separate sketch, so they
can't drift from it. Look opens with Appearance: System, Light and Dark as
pictures of the picker (System split diagonally), in one row at default and
minimum window widths, including Larger text. `ui/PictureChoice` gives every
picture an equal grid column and scales the contents inside the available
padding; never wrap three choices as two plus one. System is the default, then
Accent as the Mac's row of colour dots, Follow the Mac by default, then
Apple's hues, then a divider and Ibis scarlet, named in its tooltip
(maintainer, 2026-11-01: stock macOS first). The
website shows the app as it currently is. Each Appearance picture owns the
labelled theme for its body and controls; System is split across the whole
picture diagonally. Rows and Search icon use a crop of the real components at
about 60%, with readable sample titles, rather than shrinking the whole window.
The crop and its contents shrink together within the available row padding at
minimum width and 200% text.

**Pages for everyone** (2026-10-30). Most people never open Settings, so
defaults carry the product (principle 9). The eventual seven pages, each with one job, in this
order: **General** (the opening shortcut, Open at Login, Show in Menu Bar,
pasting), **Look** (appearance, accent, rows, text size, window size, search icon, show what will
be pasted; each visual choice a row of small pictures), **Libraries**
(each source with its status in words, which Zotero libraries, Add Library…,
Remove with Undo), **Citations** (default format, formats remembered per app
with Forget, styles in the format menu), **Shortcuts**, then, after a gap,
**Advanced** and **About Ibis**. No page opens with more than eight rows. Labels
are plain words; a one-line description only where a label could be misread.
Visual choices show pictures: each row density is a small drawing of rows,
each window size a small frame, each search icon itself; text size is a
segmented Default, Larger, Largest. A choice is picked by clicking its
picture, never by a menu item that cycles values. Advanced is a page, not a
mode: Where Ibis opens, Reset Window Position, the settings file (Open, Show in Finder; checked
when saved, errors explained on this page), installing the `ibis` command,
copying a problem report, and Reset All Settings with Undo; nothing on it is
needed for everyday use. A search field heads the sidebar: it matches labels,
descriptions and common words ("dark", "startup", "paste"), opens the page and
tints the matching row for two seconds. Like the picker's search, its glyph
centres on the page-icon column and its placeholder starts on the label column
(maintainer comment, 2026-11-01). A setting lives in one place; the
picker keeps only actions with their own keys.

**How Settings grows** (maintainer, 2026-11-01: flexible and extensible, with a
lot of back and forth). Every setting is one entry in a single settings
registry: its key, page, group, label, optional description, control, default
and search words. Pages and search are drawn from the registry, so a feature
adds or deletes its settings in one place without touching page layout, and a
feature removed in one pull request takes its settings with it. Only built
entries appear (see above). Growth uses a few shared patterns, never one-off
screens:
- **Row**: a label and one control at the end (switch, pop-up, segmented,
  shortcut recorder, button).
- **Pictures**: a visual choice as a row of real pictures in equal columns.
- **List**: items with their status in words, Add… and Remove with Undo, and a
  Details… sheet per item (libraries, formats per app, shortcut overrides).
- **Details sheet**: when a group would pass eight rows, its rarer rows move
  behind a "Details…" button that opens a sheet over the page, as in System
  Settings; the page itself never scrolls into a wall of rows.

Where planned work lands, from the open issues, so pages keep one job as they grow:

| Page | Now | Planned (issue) |
|---|---|---|
| General | Opening shortcut, Open at Login, pasting | Show in Menu Bar; interface language (#1381); selected text becomes the search and is replaced (#1526) |
| Look | Appearance, Accent, Rows, text size, search icon, paste preview | Window size (#1489); tips on the empty search (#1496) |
| Libraries | — | Sources list with status, which Zotero libraries, Add/Remove (#781); CSV template (#1379); a library over SSH (#1542) |
| Citations | Default format | Formats per app with Forget (#1240, #45); styles and locales (#19, #369); citation forms (#1315); locator types (#24); key-change warning (#1092); receipts and strict mode (#1383); Zotero fields in Word (#1543) |
| Shortcuts | Opening shortcut, key list | Conflicts, overrides and a test field (#779), from one command registry (#1203) |
| Advanced | — | Where Ibis opens, Reset Window Position, settings file, `ibis` command, problem report, Reset All; Export and Import Settings for another computer (#1561) |
| About Ibis | Version, update check | Download quietly and install on quit (#1382); licences (#1271); privacy in one line (#1375) |

A new kind of setting that fits none of these pages is a design question for
this file first, never a new page added in passing.

**Forget formats for apps** lives on Citations and says Ibis asks again the next
time you paste into each app. Disable its button while forgetting; show failures
beside the row with a retry, and a brief confirmation there only after success.

**Show in Menu Bar** is planned and stays hidden until implemented. Its intended
behaviour is on by default. Turned off, the shortcut still opens
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
drawn with the shared selected fill and paired text. Rows are 32 px, the Mac's
default sidebar row. One Tab stop enters the
list; arrows and Home/End move focus and select the page. Typeahead moves
focus to a matching page; Enter chooses it. The native window defaults to
920 × 560 points and has a minimum of 800 × 400 (maintainer, 2026-10-31).
`--sidebar-width` is 12 rem, capped at 26% of the window to preserve
content space with larger type. The page keeps its readable
40 rem cap rather than stretching rows across spare width.

The content page and bounded navigation list scroll independently. The
document, shell, sidebar background and reserved window-button area stay
still. The list fills the height left below the title bar and may scroll when
long labels wrap at large text sizes; rows keep their natural height and
remain reachable by scrolling. The title-bar drag region belongs to the shell, outside both
scrollers. Neither text nor controls may slide behind the transparent title
bar or traffic lights. Never reserve that space with a sticky spacer or a
negative margin.
Opening Settings normally, selecting a page or requesting General again
starts that page at the top, with a `--type-display-*` heading and
24 px on the sides and bottom. Above the heading, use only
8 px below the reserved title bar, avoiding a second
large blank band. A 24 px gap keeps the heading, cards and titled
groups apart; groups never touch. The page reserves a stable scrollbar gutter
in addition to the content inset, so an overlay scrollbar cannot cover text
or controls (maintainer, 2026-10-31). A single click on the current
sidebar page also returns that page to the top. An explicit Pasting deep link
may reveal that group, after its data loads, by moving only the page scroller.
Presenting an existing Settings window without a page (shortcut or tray)
emits the General page event too, selecting General and resetting its scroll.

`ui/SettingsGroup` owns grouped cards (`--r-card`, 40 rem wide at most)
and inset rows. Groups and cards keep their natural height; the page must
never shrink them to squeeze a long form into the window. Row heights are
minima, include their padding, and grow in rem with text. Every row shares
12 px; wrapped descriptions and controls retain the same
side and vertical inset. 4 px separates a label from its
description. Row buttons keep `--space-1` of block padding, including when a
recording prompt wraps. Labels wrap, controls move below when necessary,
buttons inherit the surrounding type and wrap
their labels, and switches keep their shape. This holds at the minimum window
size and with system text scaled up to 2×; no fixed row heights or clipped
labels to make the form fit. Check default and minimum widths at both Default
and Larger text; keep the last control reachable without moving the shell.

**No browser disclosure controls** (maintainer, 2026-10-31). Never use native
`details` or `summary` elements, raw disclosure triangles, or screen-specific
imitations anywhere in the app. Related settings and shortcut references use
always-open `ui/SettingsGroup` rows with a heading; the Shortcuts page's
**In the picker** group is always visible. Any future collapsible pattern must
first belong to the shared design system.

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
confirms in words. Auto-paste on Mac requires both PostEvent and Accessibility;
Settings says allowed only after both passive checks succeed. A missing
Accessibility grant has its own copied receipt; provider failure is not a
permission denial. The guide lives in Settings › General › Pasting; the
existing PostEvent refusal also opens it once per run. Because macOS keys
permissions to the code signature, an explicit Open System Settings click can
repair Ibis's stale PostEvent entry and request it (#1292, #1242). Permission
reads never prompt, and the guide never requests or resets Accessibility.
Automatic listing stays first. Beside **Open System Settings**, the Ibis app
icon is a file drag source, followed by one line: "If Ibis is not in the list,
drag it there." It drags the running bundle's file URL, including a renamed,
moved or Nightly copy, with only a copy operation; it never moves the app or
changes the citation clipboard. The icon uses the shared `ui/FileDrag` and
button states; clicking or pressing it opens System Settings as the keyboard
alternative. An unbundled development executable cannot be dragged; a failed
drag points to the list's **+** button. Neither starting nor completing a drag
claims a grant: the same live permission read must confirm it (#1390).
The guide opened after a refused paste closes after both grants are read back
on return. It does not reopen the picker or replay Enter: return to the external
document and open Ibis again, then press Enter to try again. An ordinary
Settings visit stays open and confirms the grants in words. A background
grant never brings Ibis forward. Hidden pages make no permission reads; pasting switched off stops
the one-second checks, keeping the capability read on return.
Zotero's sharing line explains reading references; the macOS guide's line
explains pasting. These are separate grants, never instructions to give both.

**Start Ibis when you log
in** is off until turned on (App Review 2.4.5(iii)), uses each system's words
("Open at Login", "Start when you sign in", "Start on Login"), and is read from
the system, never `settings.json` (`login_item.rs`, [PRIVACY.md](../PRIVACY.md)).
**Show in Menu Bar** (macOS, on by default). Turned off, the shortcut still opens
Ibis, and opening Ibis from Applications opens Settings, so it can be turned back on.
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
On Windows, this gate checks the exact picker HWND in the foreground, including
when its child WebView2 has keyboard focus. Another Ibis window does not qualify.
Automatic offers also wait while input is non-empty, a key or pointer is held,
composition or copying is in progress, or a picker overlay (including the format
chooser) is open. Opening and closing the format chooser invalidate the previous
idle ticket; closing starts a fresh idle period. For a pending
update, an empty, focused picker idle for 1.5 seconds requests a fresh check. Hide and reopen
invalidate old idle reports; no timer runs while hidden. No query text crosses
this readiness boundary. Every platform defaults to **Later**: Return, Space
on the initial control, Escape and close cannot install. **Install and Restart**
requires deliberately selecting or pressing that action. Safe defaults protect
the race where a key arrives as a native offer opens (2026-10-30, PR #1504 review).

## Brand and mark

[C16] The final master and settings in `brand/` ([brand/README.md](../brand/README.md)):
one silhouette with a top-to-bottom tone, top rim light, bottom rim shade and a
short shadow from one SVG filter. App icon: the scarlet bird at 62 % of a light
tile with the kit's optical centring (on transparency on Windows); entries
below 48 px are flat. Logos and icons come only from `brand/`; regeneration is
in the [icon source notes](../apps/desktop/src-tauri/icons/source/README.md).

## Website

The website's design rules live in [website/README.md](../website/README.md#design).
The demo footer uses the app's system key-cap labels and tokens at every width;
shortening action labels never changes key notation or cap size.

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
| Bibliography, "References used this session", project `.bib` (0.4) | More and the format pop-up; the session list is editable with Undo; with a project `.bib` chosen, rows say "In refs.bib" or "Missing from refs.bib" and More offers its missing entry |
| `ibis check`, DOI lookup (0.6) | Command line first; DOI lookup as a no-results fallback, opt-in |

## How the design changes

- A design change edits this page, `tokens.css` or a shared component, never one
  screen.
- A visual bug is fixed in the rule or component that allowed it, with a check
  (a regression test) so it cannot return.
- The canvas and the repository are one design, never two (maintainer,
  2026-11-01). The canvas is where the maintainer sees and corrects the design;
  this page, `tokens.css` and `src/ui/` are what the code uses. Every change
  lands in both on the same day: a canvas decision is written here, in place,
  with a [C] tag, and the stale text it replaces is deleted in the same
  change, never left as a "superseded" layer. To keep them from drifting, the
  canvas paints with the app's own values: its token names and numbers are
  those of `tokens.css`, and once `tokens.css` carries them it loads that file
  as is. Its components use the names in `src/ui/`. If a mismatch slips
  through, fix whichever side is wrong the same day, so both show the same
  thing.
- Each new macOS release (macOS 27 shipped in autumn 2026) gets one review of
  this page against Apple's Human Interface Guidelines and the shipped system:
  changed values, materials, menus and sidebars are recorded here and in the
  canvas in one change.
- Code that does not yet match this page is listed in one design-system issue
  and fixed in the code; the rule changes only when the maintainer changes it.
- Values marked as measured need native evidence on each platform (#774).
