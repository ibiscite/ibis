# Picker design

This is the agreed starting point for the picker redesign (#223), accepted by the
maintainer on 2026-10-19 after five rounds of prototypes. It describes what the
picker should look like and how its window should behave. It is brought into the
app in small steps (see [Rollout](#rollout)); until a step lands, the app keeps its
current appearance. User-selectable looks may come later; this is the default.

The prototypes were built outside the repository. Everything needed to implement
the design is written down here.

This page describes the target, including longer-term parts. Release scope
comes from [ROADMAP.md](../ROADMAP.md). What is built and what is left is tracked
in #223 and the issues named under [Rollout](#rollout), not on this page.

More detail:

- [Design system foundations](design/system.md): decisions D1–D17, per-OS tokens, shapes, states and components.
- [Settings window and shortcut customization](design/settings.md): pages, the shortcut recorder and conflict rules.
- [Citation preview](design/preview.md): the citation shelf and the wide pane.

## Eight rules

1. **One anatomy everywhere.** Search, list, detail, citation tray and footer, in
   that order, in every form from a one-line bar to the large settings window.
2. **Native material, Ibis voice.** Each platform's material, used by layer:
   content on the standard material, glass only on controls (macOS), Acrylic
   for the Windows picker, flat on Linux. Type, spacing, icon drawings and
   wording stay the same on every platform.
3. **Readable first.** Nothing smaller than 12 px, metadata at 13 px, titles at
   15 px. Secondary text passes 4.5:1 contrast.
4. **Easy to hit.** Every button is at least 32 px; 44 px on touch screens.
5. **Show less, reveal more.** An element appears when it becomes useful and
   steps back when it is not. Nothing is removed, only folded one key away.
6. **Say what happened.** Every action ends in a visible result: copied,
   inserted, or why not. Colour is never the only signal.
7. **Calm motion.** Movement only explains where something came from, and it
   switches off when the system asks for reduced motion.
8. **No glow, no edge rings.** Selection is a fill, never an outline. No
   coloured borders, outer glows or bright rims. A focus ring appears only when
   the keyboard moved focus.

## Design language

**Type.** The system font on each platform, one scale:

| Role | Size / line | Example |
|---|---|---|
| Search | 22/28 on macOS (frameless, sized like Spotlight's query); 16/22 on Windows and Linux, in a framed text box like PowerToys Run (16) and GNOME's search entries | Search references… |
| Title | 15/20, semibold | Sleep spindles predict memory |
| Body | 14/20 | Spindle density predicted recall. |
| Meta | 13/18 | García, Okafor · 2021 |
| Label | 12/16, semibold | Best matches |
| Citation | 13, monospace | `[@garcia2021, p. 4]` |

**Colour roles.** Selection, focus, the main button and the menu highlight
use the system accent ("you are here, act on this"). Ibis scarlet means "this is
what gets pasted" and marks only the citation: the text that will be copied,
the citation tray, the tile of a row added to the citation, and the logo. Ibis
sets no app accent of its own, so a red or orange system accent never meets
scarlet on a fill (on macOS, the Multicolor accent stays system blue). Status
uses Done (copied, inserted), Look (skipped, needs a look) and Stop
(unreachable, failed), always with words or an icon as well as colour.

| Role | Token | Light | Dark | Over the macOS veil | More contrast, forced colours |
|---|---|---|---|---|---|
| Citation text | `--citation` | `#c23a1c` | `#ff7a5c` | `#982d16` / `#ff9d87` | `CanvasText` |
| Added tile, behind a white check | `--citation-tile` | `#c23a1c` | `#d2401f` | as light / dark | `CanvasText` |
| Brand scarlet | none in the app | `#d2401f` | `#d2401f` | | |
| Done / Look / Stop | `--status-*` | `#1a7f37` / `#8a5a00` / `#c4314b` | `#5bc26f` / `#e3b341` / `#ff8a8a` | one step deeper | `CanvasText` |

Text roles pass 4.5:1 on their surface; the added tile stands 3:1 off the
surface and keeps its white check at 4.5:1. Brand scarlet is for the logo, the
app icon, the website and store art. In the app it fills only the dark-mode
added tile, never a control, a selection, a link or a focus ring. Stop is a
rose, kept apart from scarlet in hue and lightness, and never used for the
citation.

**Space and size.** A 4 px grid. Rows come in three densities: Compact 40 px
(one line), Comfortable 58 px (default, two lines) and Spacious 76 px (three
lines). A Larger text setting scales the whole window by 1.15, not just the
letters.

| Token | Value | Notes |
|---|---|---|
| `--inset` | 8 px (Windows 4 px) | Window edge to the search field, the list and the footer. The band above the field is the drag handle. |
| `--field-h` | 40 px | Search field, so the search band is 56 px (48 px on Windows). |
| `--row-pad` / `--row-gap` | 12 px / 12 px | Row side padding; tile to text. |
| `--tile` / `--icon` | 32 px / 20 px | Type tile and its icon. Compact draws a 24 px tile with no fill, centred in the 32 px column, so titles stay aligned with the search text. |
| Footer | 40 px | Text starts at `--inset + --row-pad`. |
| Smallest target | 32 px, 44 px on touch | |

Edges line up: the search icon is centred over the tile column and the
placeholder starts where row titles start:

```
--lead     = row-pad + (tile − icon) / 2       field padding before the icon
--lead-gap = (tile − icon) / 2 + row-gap       icon to placeholder
text x     = inset + row-pad + tile + row-gap  64 px (60 px on Windows)
```

These numbers are also in [`design-geometry.json`](design-geometry.json), which
`node scripts/gallery-geometry.mjs` measures the evidence gallery against;
change both together (a test checks that they agree).

**Corners.** Nested shapes are concentric: an inner radius is its container's
radius minus the inset between them, never larger. When the inset is close to
or larger than the outer radius, the inner corner drops to a small floor
instead of going square or looking like a separate floating slab.

```
--r-row  = max(--r-min, --r-win  − --inset)     rows, search field
--r-tile = max(--r-min, --r-row  − --row-pad)   type tile, footer buttons
--r-mi   = max(--r-min, --r-menu − --menu-pad)  menu items
```

| | macOS | Windows 11 | Linux (GNOME) |
|---|---|---|---|
| `--r-win` | 28 on macOS 26 and later; 12 pt before that (the native panel corner, #236) | 8 | 14 |
| `--inset` · `--r-min` | 8 · 6 | 4 · 2 | 8 · 4 |
| `--r-row` | 20 on macOS 26 and later; 6 before | 4 | 6 |
| `--r-tile` | 8 on macOS 26 and later; 6 before | 2 | 4 |
| `--r-menu` · `--menu-pad` · `--r-mi` | 12 · 6 · 6 | 8 · 4 · 4 | 12 · 6 · 6 |
| `--e2` menu shadow, plus a 1 px hairline | `0 12px 32px` black 16% | `0 8px 16px` black 14% | `0 2px 8px` black 20% |

On GNOME, rows are flat with the 6 px radius of Adwaita's sidebar and popover
rows: a boxed list cannot be concentric at an 8 px inset in a 14 px window.

Each look sets only `--r-win`, `--inset` and `--r-min` (and the menu pair);
every inner radius follows from them. With Larger text the inset grows with the
text and the window corner does not, so inner radii shrink toward the floor.

**Menus.** The operating system's own menus are used only where the system owns
them: the menu bar, the tray menu and right-click context menus ([ADR 0021](adr/0021-ui-foundation.md)).
In-window menus (format, library scope, More) are Ibis's, but match that
platform's native menu metrics: radius and padding from the table above, the
native item padding (items stay at least 32 px high, rule 4), separators, and
open and close motion. Check them side by side with a real
menu on each platform before calling them done.

**Motion.** 120 ms for feedback, 200 ms for panels, 280 ms for layout, all with
`cubic-bezier(.2, .8, .2, 1)`. No bounce. With reduced motion, panels fade
instead of sliding. Hover never moves anything.

**Icons.** Ibis's own set: 20 px, 1.5 px stroke, rounded ends, the same
drawings on every platform, so a source or a reference type always looks the
same. Each platform gets them at its own weight:

| | Base | macOS | Windows 11 | Linux (GNOME) |
|---|---|---|---|---|
| Grid | 20 px | 20 px | 16, 20, 24 px masters | 16 px, drawn natively |
| Stroke | 1.5 px | 1.6 px (SF Symbols Regular) | 1 px (Fluent Regular) | 1.5 px (Adwaita symbolic), solid small details |
| "On" state | filled | the `.fill` form | the Filled style | the button's checked fill, same drawing |

GNOME icons are named `ibis-…-symbolic` and recoloured by GTK. Where the
platform already has an icon with the exact meaning (menus, system dialogs), the
platform's icon is used. Seven type
icons say what a reference is: article, book, chapter, thesis, web page, report
and other (chosen from its CSL type). They never reuse an attachment drawing
(the document or the globe), so a row never shows the same picture twice.

**App icon (#579).** Direction 1, the ibis, variation B (upright stance and
shortened neck), gradient G1 (`#e5532d` to `#c0361a`), with separate small and
symbolic masters. How the assets are regenerated is in the
[icon source notes](../apps/desktop/src-tauri/icons/source/README.md).

**Per platform.** Only these change:

| | macOS | Windows 11 | Linux (GNOME) |
|---|---|---|---|
| Material | Standard material for the window; Liquid Glass only on the search bar (Rollout 9) | Desktop Acrylic | Flat, opaque |
| System font | SF Pro (`-apple-system`) | Segoe UI Variable (Text) | Adwaita Sans or Cantarell (GTK's `system-ui`) |
| Neutrals | Translucent fills over the material (below) | Fluent: surface `#f3f3f3` / `#202020`, text `#1b1b1b` / `#fff`, secondary `#606060` / `#cfcfcf`; controls on `ControlFillColorDefault` | Adwaita `window_bg` `#fafafb` / `#222226`, `window_fg`; buttons and entry on the control fill (text at 8%) |
| Corners | See **Corners** above | | |
| Selected row | Filled accent, white text | Soft grey with an accent pill on the left | GTK's selection colour (filled accent) with its paired text |
| Keyboard focus ring | 2 px accent, 1 px gap | 2 px in the text colour around a 1 px surface-coloured stroke (Fluent's focus visual) | 2 px, the accent mixed half toward the text colour, 1 px gap |
| Search box | Frameless, large; with Liquid Glass, a 40 px capsule that also holds format and pin | Text box with an underline that turns accent on focus | Framed entry |
| Shortcut labels | ⌘ ⌥ ⇧ ⌃, joined in one key cap (⌘K) | Ctrl Alt Shift, one cap per key (Ctrl K) | Ctrl Super Shift, one cap per key with "+" between (Ctrl + K) |
| Header menus | Borderless pop-up button with ⌃⌄ | ComboBox with a chevron | Flat drop-down with `pan-down-symbolic` |
| Switch | 38 × 22, the regular `NSSwitch` size | 40 × 20, outlined when off | 46 × 26 |
| Copied | Pill near the top of the screen | Notice bottom-right | Toast at the bottom |

On macOS, use WebKit's paired selected-content background and alternate-selected text when both are supported. `AccentColorText` can resolve to black even on saturated blue; keyword support alone does not establish a native pair or custom-accent following. Generic fallbacks remain for other engines, and increased contrast and forced colours retain their system Highlight pair.

The Linux selected row uses the full selection colour rather than a tint: focus
stays in the search box while ↑ ↓ move the selection, so the fill is the only
cue, and libadwaita's list tint (25 % accent, `$view_selected_color`) or a
plain neutral fill reaches only about 1.1–1.3:1 against the rows beside it.
The full accent reaches about 3.7:1 (light) and 4:1 (dark) with Adwaita blue.
Adwaita's half-accent focus ring is mixed toward the text colour instead of
made transparent for the same reason.

High contrast and reduced transparency replace every material with the opaque
system surface, as [native appearance groundwork](https://github.com/ibiscite/ibis/pull/232#issuecomment-6282577029) already does.

On macOS the glass is AppKit's popover material (`NSVisualEffectView`) behind
the page, under a light veil of the surface colour at 55%: web text cannot use
AppKit's vibrant blending, and without the veil secondary text drops below
4.5:1 over a bright or colourful wallpaper. Over the veil, the citation and
status colours are one step deeper (scarlet `#982d16` light, `#ff9d87` dark) so
they keep 4.5:1; the opaque surface keeps the usual scarlet. Fills, hairlines
and key caps are translucent so the material shows through, and the window edge
is a 1 px hairline (black 10% in light, white 10% in dark), never a white rim.

**Materials by layer.** Every platform separates a content layer (the result
list, preview and tray) from a control layer (search, format, pin).

- **macOS.** Content stays on the standard material described above. Liquid
  Glass (`NSGlassEffectView`, macOS 26) is a later step (Rollout 9) and then
  covers only the search capsule: the regular variant, untinted, never on rows,
  lists or the preview, and never glass on glass. Selection stays the system
  accent. Ibis sets no blur or opacity of its own, so the macOS 27 glass
  opacity setting, Reduce Transparency and Increase Contrast apply unchanged.
- **Windows.** The picker is a transient, light-dismiss surface, which is what
  Microsoft reserves Desktop Acrylic for. Content sits directly on it, with no
  second layer. Mica is for long-lived windows such as settings. Both become the
  solid surface when transparency effects are off or battery saver is on.
- **Linux.** Flat and opaque on `window_bg`, with the search entry on
  `view_bg`. Ibis does not use the GNOME 51 blur API, since GNOME's own apps
  stay flat.

## What shows, and when

| Element | Visible | Why |
|---|---|---|
| Search box | Always | The largest element, focused on open. Its placeholder lists what can be searched. |
| Format (for example "Pandoc ⌃⌄" on macOS) | Once a library is chosen | What is pasted depends on it. One word, no icon; each platform's pop-up button (see **Header**). |
| More (Ctrl/⌘ K) | Always | The one door to every other action; each shows its shortcut, so the menu is also the cheat sheet. In the footer it reads "More ⌘K", like the hints beside it (see **Footer**). |
| Pin | Always, faint at rest | Tertiary colour at rest, full strength on hover or while on, so it never moves or appears from nowhere. Its tooltip names the action ("Pin: keep Ibis open beside your document") and, once the key table assigns one, its key. The first time a click outside hides Ibis, a one-time footer tip explains the pin. |
| Filter chips | Once added | `#tag`, `y:2019-2022` or `a:name` followed by a space becomes a removable chip. Backspace in an empty box removes the last one. |
| Recently cited, Recently added, With PDF | While browsing | Shown when the search is empty or one is selected; hidden while typing so rows get the room. Tab reaches the current browsing tab; ← and → choose another (see **Keyboard focus order**). |
| Tag and author suggestions | While typing `#` or `a:` | Take the place of the tabs. Enter, or Tab while the suggestions are open, picks the first. |
| Row: type tile | Always | A 32 px tile at the row's start with the type icon on a quiet fill (24 px and no fill in Compact, so rows stay 40 px). Decorative: screen readers hear the type word instead. |
| Row: title and year | Always | Year at the right edge, in the same place on every row. |
| Row: authors and journal | Comfortable and Spacious | Tells apart references with the same first author and year. The journal is the container title (journal, book or proceedings), else the publisher. Spacious adds the item type as one word ("Article", "Book"). One line: authors are shortened as in a citation ("García", "García, Okafor", or "García et al." for three or more; screen readers hear every name), then the journal, each with an ellipsis (the key and library label shorten first, never below a few letters, and the type word stays whole; only when enlarged text leaves no room do they move under them); a match in the journal is always named, since it may be shortened. Compact shows the first author ("García et al.") after the title. |
| Row: why it matched | Only when hidden | "In the abstract: …sleep…" or "Tagged: …" when the match is not in the title or authors. |
| Attachment chip | When a file is attached | The platform's document (or globe) icon only, 16 px in a 24 px chip. "PDF, 12 pages" is its tooltip and accessible name, and it opens the preview. No empty column otherwise. |
| "In 2 libraries" | Only for duplicates | Explains why one reference may differ between sources; it is listed once. |
| Add button | Hovered or selected row | Takes the place of the type icon, so the row never reflows: a plus, or a check on a row already added. A row in the citation always shows a scarlet tile with a white check, the only scarlet on the row. A 32 px pointer target with its name in the tooltip ("Add García 2021 to citation"); it adds no Tab stop, because a listbox option holds no controls, and keyboards use Shift+Enter. |
| Citation tray | Once a source is added | Page number per source and the exact text to be copied, in scarlet. The chips and the preview are bands above the list that never cover it. The tray shows at most two whole rows of one-line chips (a long key ends with an ellipsis; its tooltip and Remove button name it whole), scrolls by rows and fades at its bottom edge while more chips are below. |
| Footer key hints | Always | What Enter does now, Shortcuts and More, label first and keys after (see **Footer**). |
| Footer tip | One at a time | Tied to what the user is doing, retired once they have done it. Otherwise the footer shows library status. |
| Library status | Always, quietly | A dot, the source and the count; the age only when it is news or a warning (see below). A sentence above the list only when something needs the user. |

**Library status.** One footer line, as Mail's "Updated just now" and VS Code's
status bar do it: short words, colour only alongside them, detail on demand.

| Case | Footer | Dot |
|---|---|---|
| Starting, first run | "Checking library…", "No library chosen" | Grey |
| Opening | "Zotero · opening…" | Grey |
| Loaded | "Zotero · 1,204 references" or "thesis.bib · 1,204 references" | Done |
| Changed while open (ten minutes) | "… · updated 2 minutes ago", only when a reread brought different references | Done |
| Reloading | "… · reloading…", count kept | Unchanged |
| Reading notices, empty | "… · 2 reading notices", "Zotero · no references" | Look |
| Update failed, old copy kept | "Zotero · not updated · checked 2 hours ago" | Stop |
| Switch to another library failed | The library still searched, as when loaded | Stop |
| Never opened | "thesis.bib · not opened" | Stop |

Several Zotero libraries still read "Zotero"; rows name their library. The
tooltip, and the top of More (also the name of its library group, so keyboard and
screen-reader users reach it), give two lines: a short one ("Zotero · 1,204
references") and a quieter one with the libraries, when they were last checked
(relative and clock time) and any problem ("My Library, Lab group · checked 2
hours ago, 3:19 PM · Zotero could not be reached"). "Updated" always means the
references changed; "checked" means Ibis last read them or found them unchanged.
The footer always names the library that is searched, even after a switch to
another one failed. A long name is shortened before the state words, and names
are isolated so right-to-left names keep their place. Times
use `Intl.RelativeTimeFormat` in the app's language ("just now" under 45 seconds,
a date after a week) and tick every 30 seconds without redrawing the picker. The
live region holds only the words before the age, so a ticking clock is never
announced. The tray line has no time, because a native menu is not redrawn while
open; after a failed update it says "not updated" and why.

## Header, footer and empty states

The picker's chrome: what sits around the result list, and how it lines up.
Sizes come from **Space and size** and **Corners**. Four role tokens are
new, from the design-system foundations, and are defined here:

| Token | Meaning | macOS | Windows 11 | Linux (GNOME) |
|---|---|---|---|---|
| `--r-control` | Buttons, fields, pop-up buttons | 6 (a capsule in the header on macOS 26) | 4 | 6 |
| `--r-key` | Key caps | 4 (5 on macOS 26) | 4 | 4 |
| `--surface-raised` | Cards and grouped forms, one step above `--surface` | `#fff` / `#2c2c2f` | white 70 % / white 5 % | `#fff` / `#2e2e32` |
| `--text-tertiary` | Glyphs at rest (pin, indicators), at least 3:1; never sentences | black 45 % / white 45 % (`tertiaryLabelColor`) | black 45 % / white 54 % (`TextFillColorTertiary`) | `currentColor` at 45 % |

`--key-cap` (20 px) is the existing key-cap height.

**Edges.** Visible edges line up with the content; hit targets may overhang
them. The leading edge is the tile column (`inset + row-pad`), the text column
starts at `text x`, and the trailing edge is where row text ends
(`W − inset − row-pad`). The pin's glyph, the last key cap in the footer and
the year column all end on that trailing edge; a 32 px target around a 20 px
glyph overhangs it by 6 px. Controls in one bar share a centre line, and text
in one bar shares a baseline.

**Header.** In order: search glyph, query, clear button, the header menus
(format, then library scope when there is one), pin.

- **Clear (×).** Inside the field at its trailing end, only while there is a
  query: a 16 px glyph in a 32 px target, secondary colour. Its name is
  "Clear search"; it is not a Tab stop, because Escape does the same. The
  glyph follows the platform, as the **Icons** rule allows for meanings the
  system owns, but is Ibis's own bundled SVG (a webview cannot load SF
  Symbols or Segoe icons): a filled circle with a × on macOS, like its search
  fields; a plain × on Windows and GNOME, like a Fluent text box and a GTK
  search entry.
- **Header menus** are pop-up buttons: they show the current choice as their
  label, and the menu marks it with a leading check. The label is the value,
  in the text colour at 13 px; the indicator is secondary and never turns
  when the menu opens. 4 px between the field and the first menu, and between
  two menus.

  | | macOS | Windows 11 | Linux (GNOME) |
  |---|---|---|---|
  | Native model | Pop-up button, borderless (toolbar) style | ComboBox | Drop-down (`GtkDropDown`), flat in a header |
  | Indicator (bundled SVG, drawn like) | ⌃⌄, as `chevron.up.chevron.down`, 8 × 12 | a chevron, as Fluent's `ChevronDown`, 12 px, 1 px stroke | a chevron, as `pan-down-symbolic`, 16 px |
  | Rest | No fill | Control fill, 1 px stroke with a darker bottom edge | No fill |
  | Hover / open | Quiet fill / pressed fill | Hover fill / pressed fill | Quiet fill / pressed fill |
  | Height, radius (`--r-control`) | 28 px in a 32 px target, capsule | 32 px, 4 px | 32 px, 6 px |
  | Label | Regular weight | Regular weight | Bold, as libadwaita buttons are |

  These are in-window menus drawn in the webview (**Menus**, ADR 0021), so
  typing filters them. The menu opens below the button, aligned to its
  trailing edge. Space, Enter or ↓ opens it (Alt+↓ too on Windows and Linux,
  as their combo boxes do). A pick or Escape closes it and returns focus to
  the search box; Tab closes it and moves on (see **Keyboard focus order**).
  Each button is named by what it sets and its value ("Copy as Pandoc",
  "Search in All libraries"), with `aria-haspopup="menu"` and
  `aria-expanded`; items are `menuitemradio` with `aria-checked`. While no
  library is chosen the header menus are hidden, because first-run setup asks
  the same question in its own words.
- **Pin.** Always visible: `--text-tertiary` at rest, full strength on hover
  or while on, so the header never shows a hole where it appears. A toggle
  button named "Pin" with `aria-pressed`.
- **Targets.** Every header control keeps a 32 px target (44 px under
  `pointer: coarse`), even where it draws 28 px.

**Keyboard focus order.** Tab moves through the following stops, skipping
hidden controls, then back to the first; Shift+Tab reverses the order and
nothing holds focus:

1. The search box. Tab leaves it, except while tag or author suggestions are
   open, when Tab picks the first (as a combo box's autocomplete does).
   Escape closes suggestions without clearing the query; Tab then leaves the
   box. Suggestions stay closed until the query changes.
2. The header toolbar (`role="toolbar"`, named "Search options"): one stop;
   ← and → move between the format menu, the scope menu and the pin (WAI-ARIA
   toolbar pattern).
3. The browsing tabs, only while visible: one stop on the current tab in a
   `tablist` named "Browse references". Each `tab` has `aria-selected` and
   names its result `tabpanel`; only the current tab has `tabindex="0"`.
   ← and → focus and choose the previous or next tab, wrapping at the ends;
   Home and End choose the first or last. These keys apply only while a tab
   has focus, leaving text editing and attachment preview in the search box
   unchanged. Tab leaves for More; Shift+Tab returns to the header toolbar.
   Clicking or tapping a tab chooses the same view. There is no required
   Ctrl+Tab shortcut: ordinary Tab followed by arrow keys is the keyboard
   route on every platform, including when browser accelerators are consumed.
4. More.

The result list is not a stop: ↑ and ↓ in the search box move through it
(`aria-activedescendant`). Inside an open menu, Tab closes the menu and moves
to the next stop, as if focus were on its button; from More that next stop is
the search box. The [shared key table](../apps/desktop/src/keys.ts) lists the
implemented keys; add these browsing-tab keys when the tabs are implemented.

**Footer.** Library status at the leading edge; at the trailing edge, key hints
and More. One hint pattern everywhere outside menus: **the label first, then
its keys**, 8 px between them, 16 px between hints, all at 12 px on one
baseline, each key cap 20 px high and centred on it.

- **Order.** What Enter does now ("Copy as Pandoc ↵", the primary hint, in the
  text colour), then "Shortcuts ⌘/", then "More ⌘K" (both secondary). The
  hint that comes and goes is first, so the cluster's trailing end never moves.
  With Alt held: "Add to citation ⇧↵", then the Escape step ("Clear search
  Esc" or "Hide Esc"), then More.
- **More** looks like the other hints but is a button: a quiet fill on hover
  and while open, drawn 28 px high and centred in the 40 px footer, with a
  32 px target, radius `--r-tile`. Its name is "More", with
  `aria-haspopup="menu"`, `aria-expanded` and `aria-keyshortcuts`; its caps
  are hidden from screen readers.
- **Shortcuts** is a hint, not a button: it names the key that opens the
  keyboard shortcuts list. The name matches each system's own term (macOS and
  Windows both say "Keyboard shortcuts"); the footer shortens it to one word,
  and More and the list's title say it in full. Changing shortcuts belongs to
  the settings design, not to this list.
- **Screen readers** hear the hints as one group named "Keys", each as its
  label then its keys ("Copy as Pandoc, Enter"). The Enter hint is hidden
  from them while focus is outside the search box, where Enter does
  something else.
- **Narrow windows.** The library status shortens first (its own rules).
  Under 520 px, or with Larger text, the Shortcuts hint is dropped (More
  still lists it). If the rest still does not fit, the hints wrap onto a
  second line, which keeps 8 px above and below.
- **Library status** keeps its rules (see **Library status**): the dot is
  6 px, centred on the text's x-height, and starts on the leading edge.
- **Key caps.** A 20 px fill (`--key-cap` on the control fill), radius
  `--r-key`, 5 px side padding, 12 px text in the UI font, never an outline
  and never scarlet. macOS joins the glyphs in one cap, as its menus write them
  (⌘K, ⇧↵). Windows gives each key its own cap, 4 px apart, as PowerToys does
  (Ctrl K). GNOME gives each key its own cap with a "+" between, as GTK's
  shortcut labels do (Ctrl + K). In menus a shortcut is plain trailing text,
  as in each system's own menus, never a cap.

**Empty states.** One template on every platform, centred like
`ContentUnavailableView`, `AdwStatusPage` and Fluent's empty state: a 48 px
symbolic icon in `--text-tertiary`, a title (15/20, semibold), a line of
at most 40 characters per row, wrapping to at most three rows (13/18,
secondary), then at most two actions, or key hints, 16 px below. A file name
in a title is shortened in the middle, keeping its extension
("my-very-long…library.bib"). The title and line are plain text after the
list, so screen readers reach them by reading on; the existing polite live
region already announces "0 results". Key hints in an empty state are hidden
from screen readers, because the keyboard shortcuts list says the same. No skeletons; a spinner only after 400 ms. It starts 48 px below the search band. The
result heading ("Best matches", "20 results") appears only above rows: with
no rows there is nothing to name.

| Case | Title | Line | Below |
|---|---|---|---|
| Nothing typed, library loaded | Type to find a reference | Search by title, author, year, citation key or DOI. | Two key hints: "Copy ↵" and "Add to citation ⇧↵" |
| Nothing typed, with history (Rollout 6) | | | No template: "Recently cited" rows, ready for ↓ and Enter |
| No matches | No matching references | Check the spelling, or try fewer words, an author's surname or a year. | Clear search; Search all libraries when some are left out |
| Empty `.bib` library | No references in thesis.bib | Choose a BibTeX or BibLaTeX export with references. | Choose library… |
| Empty Zotero library | No references in My Library | Add items in Zotero, then reload. | Reload; Choose library… |

Spotlight, Alfred and GNOME's search show nothing until something is typed;
Raycast and the Windows 11 Start menu show recent items. Ibis does the second
once it keeps a local history (Rollout 6, which can be turned off and cleared),
and until then the first, with the two keys that make the first minute work.
No illustration and no tips beyond those keys.

**First run.** A grouped form, as macOS System Settings, Windows Settings
cards and libadwaita preference groups are: a title ("Choose a library to get
started") and one secondary line saying what was found, then one group of
labelled rows on `--surface-raised`, each at least 48 px high (56 px with a
second line) with `row-pad` sides and one control or value at the trailing
edge, separated by hairlines, radius `--r-row`. These are the same rows the
Settings window will use.

1. **Library:** Use Zotero and Choose library…. Exactly one is the primary
   button (accent fill, paired text): Use Zotero when Zotero is open,
   otherwise Choose library…. Files found nearby follow as rows of their own,
   name above folder, each with a Use button.
2. **I write in:** a segmented choice, "Markdown (Pandoc)", "LaTeX", "Typst"
   (and "Word" when citation styles are available). The copy format has one
   name everywhere: the header menu, the footer ("Copy as Pandoc") and this
   choice all say "Pandoc"; first run adds only the place people know,
   Markdown, in front of it.
   It is a radio group named "I write in" (each segment a `radio` with
   `aria-checked`; ← and → move and choose).
3. **Open Ibis from any app:** the shortcut, in key caps. A value, not a
   control: changing the shortcut belongs to the Settings window.
4. **Start Ibis when you log in:** a switch (`role="switch"`, named by its
   row), off until turned on.

"Look again" is a text button under the group. The footer shows "No library
chosen", Shortcuts and More, so nothing else changes once a library opens.

**Selection and focus.** One rule set for every control:

| | Pointer over it | Chosen or current | Keyboard focus |
|---|---|---|---|
| Result row | Quiet tint | The platform's selected-row fill (see **Per platform**) | The same fill; never a ring |
| Menu item | The menu highlight | Leading check | The menu highlight; never a ring |
| Segmented choice | Quiet tint | A raised thumb on a recessed track; never a coloured border | Ring around the focused segment |
| Pop-up button, More | Quiet fill | Pressed fill while its menu is open | Ring |
| Buttons | One step darker fill | Primary: the accent fill | Ring |
| Switch | No change | Accent track | Ring around the switch |
| Search box | None | None | None (the caret; on Windows its underline) |

The ring is the platform's focus ring from **Per platform**, only after
keyboard movement (rule 8). Increased contrast and forced colours add their
solid edges as described under **Accessibility**. Forced colours remove
fills, so every chosen state there uses system colour pairs as well as its
shape: the chosen segment is `Highlight` with `HighlightText`; a switch has a
1 px `ButtonText` edge, a `ButtonText` thumb on the start side when off, and
a `Highlight` track with a `HighlightText` thumb on the end side when on;
menu items keep their check; the pressed pop-up button keeps a 1 px
`ButtonText` edge.

## Learning without a tutorial

Three layers, each reached from the one before by something on screen:

- **First minute:** type, press Enter, paste. Clicking a row copies it.
- **First week:** → in the search box previews the attachment and "Cite page"
  fills in the page; Shift+Enter builds a citation with several sources;
  Tab reaches the browsing tabs, where ← and → choose recent items or PDFs.
- **Power:** `#tag`, `y:`, `a:` filters; the format menu; Ctrl/⌘ K
  for every action; pin; per-app behaviour.

The format menu offers all seven key formats from [ADR 0016](adr/0016-settings-format.md):
Pandoc, `\autocite`, `\parencite`, `\textcite`, `\cite`, Typst and bare keys.
The five bundled citation styles are separate choices ([ADR 0020](adr/0020-picker-preferences.md)).
Number-key bindings are not implemented or assigned here; the shared key table
for #223 must name each binding before any are advertised.
That table is `apps/desktop/src/keys.ts`; the `?` overlay (Ctrl/⌘ / inside the
search box), the footer hints and the More menu all read their labels from it.

No pop-up tour and no "advanced mode". Tooltips appear after 600 ms and name the
action and its key. Empty and no-result states say what to try next. Footer tips
are stored locally as "seen" flags only, never as usage data.

## Window behaviour

**Opening.** On the display with the focused window (settings: display with the
pointer, or last position), at the position last used on that display. Focus is
in the search box at once; typing must work without a click. If Ibis was closed
less than a minute ago, the last search returns, selected so typing replaces it
(adjustable up to three minutes; Raycast and PowerToys Command Palette offer
the same setting in their documentation). The kept search lives in memory only
and is never written to disk.
It appears in 120 ms with a short fade. The at-cursor form opens under the text
caret and flips above it near the bottom of the screen.

**Moving.** Drag the handle or the empty part of the header; the list never
starts a drag. It snaps to the centre line and the upper third within 18 px,
showing a thin guide; Alt places it freely. Position is remembered per display;
double-clicking the handle re-centres. It cannot be dragged more than half off
screen. If a remembered display is gone, it opens centred.

**Clicking outside.** Ibis hides and the click reaches what was clicked. The
search and selection wait a minute. When pinned, Ibis stays. It never hides while
a page number is being typed, a dialog or file chooser is open, a drag is in
progress or a sign-in is under way.

**Hover.** Moving the mouse selects the row under it; a row sliding under a
still pointer (while typing or scrolling) does not. Hover only tints: no new
columns, no height change. The wheel scrolls without changing the selection.

**Sizes.** Opening the preview widens the window toward the side with more room,
so the list does not jump under the pointer. Resizable from any edge, remembered
per display. On small screens, the preview replaces the list instead of sitting
beside it.

**Giving focus back.** After Enter, Ibis hides and focus returns to the window it
came from, and a short "Copied" (or "Inserted") notice appears where the
table above says, drawn by Ibis in a small window that never takes focus;
native system notifications may replace it later. If inserting fails, the
citation is on the clipboard and a notice says why. Shift+Enter adds to the tray and keeps Ibis open. When pinned, Enter inserts
and Ibis stays open.

**Escape** steps back one layer: close a menu or dialog, close the preview,
clear the search and filters, then hide. Nothing asks for confirmation. After
hiding, the search is kept in memory for a minute. Clearing keeps the chosen
references. Reopening within that minute restores even a query cleared by
Escape, unless new typing or a completed copy replaced it; reopening later
clears the search and selection. Escape never unpins.

**Pinned** Ibis floats above the document, keeps its place, and does not take
focus until clicked or summoned. It is the only time Ibis stays on top.

**Never** take keys people type with (Ctrl+Shift+Space is the non-breaking space
in Word and LibreOffice, so Windows and Linux use Ctrl+Shift+Y; Ctrl+Alt types letters on many European layouts; Ctrl+Space
switches input languages), open by itself, appear in the taskbar,
Dock or Alt+Tab (only the settings window does), or animate longer than 280 ms.
The shortcut is ignored while a full-screen app or presentation runs, unless it
is pressed again.

**Platform limits.**

- macOS: a non-activating floating panel, like Spotlight, so the writing app
  stays in front; it opens over full-screen apps in the current Space.
- Windows: an app may come to the front only when the user asked (shortcut or
  tray), which is when Ibis opens. Ibis remembers the window it came from.
  Typing into apps running as administrator is blocked, so Ibis copies instead
  and says why.
- Linux on Wayland: the compositor places windows and pinning on top is not
  available, so Ibis opens centred and "pinned" means "stays open". Pasting into
  other apps is generally not allowed, so Ibis copies and says "Press Ctrl+V".
  A future global shortcut goes through the system's shortcut dialog; the
  preview has no GlobalShortcuts portal integration. Its existing shortcut
  backend requires reachable X11/XWayland with XKB, and otherwise explains
  the limitation while keeping the application launcher available. See
  [Linux preview shortcut capability](compatibility.md#linux-preview-shortcut-capability).
  On X11, Ibis
  can place its window, stay on top when pinned and register its shortcut
  directly, as on Windows.

**Accessibility.** Reduced motion uses fades only. Screen readers hear the
result count and, per row, title, first author, year, journal or container
(Comfortable and Spacious), its type in words and why it matched;
"Copied" is announced politely. Every action has a key. Focus rings appear only for keyboard
focus (`:focus-visible`), never after a click, in the platform's style (see the
table above), and reach 3:1 against the surface with the platform's default
accent. The search box never shows one: it always has focus, the caret is
the cue, and on Windows its accent underline. Rows moved with ↑ ↓ show their
fill, not a ring. With increased contrast or forced colours, every control and
the search box show a solid 2 px ring and the active row a 3 px edge; a row
in the citation keeps its check tile in system colours, so the check, not
colour, says it was added. Right-to-left titles align right within their row.

## Settings

Until the settings window exists (rollout step 9), settings live in the More
menu and the menu bar or tray menu, and each takes effect at once.

**Start Ibis when you log in.** The shortcut only works while Ibis runs, so
Ibis offers to start at login: once during first-run setup, beside the shortcut
it teaches, and always in More and the menu bar or tray menu. It is off until
the person turns it on: Apple's App Review Guideline 2.4.5(iii) asks Mac apps for
consent first, and Raycast, Alfred, PowerToys and 1Password offer it as a
setting. The menu item uses each system's own words, the same in More and the
menu bar or tray: "Open at Login" on macOS (Apple's term), "Start when you sign
in" on Windows (Microsoft says *sign in*), and "Start on Login" on Linux (GNOME
capitalizes menu items; GNOME has no official term). A login start leaves the
picker hidden until the shortcut is pressed. The switch is read from the system
each time, never from `settings.json`, so turning Ibis off in the system's own
startup settings shows at once; if the system refuses, the switch shows the real
state and a sentence says where to change it. Ibis Nightly and development
builds register under their own names. How each platform stores the entry is in
`apps/desktop/src-tauri/src/login_item.rs` and [PRIVACY.md](../PRIVACY.md).

## Rollout

Each step is one pull request against #223 (or an issue split from it), reviewed
with the states gallery on every platform. Settings format 2 is
[ADR 0018](adr/0018-settings-format-2.md); [ADR 0019](adr/0019-update-checks.md)
and [ADR 0020](adr/0020-picker-preferences.md) add optional update and picker
fields. Any other choice kept across restarts (window position per display,
display choice, how long a search is kept, which tips were seen) needs an ADR
before it adds a `settings.json` field.

1. Design values in `tokens.css` (#307, done); per-platform corner radii and
   spacing from **Corners** (#576); the 280 ms cap and reduced motion for every
   transition.
2. Picker layout (#313), header and platform looks (#490), footer, `?` key
   list and no-result actions (#491), rows that tell similar references apart
   (#492); all done.
3. Window behaviour that needs no settings: outside-click hiding, the Escape
   ladder, the kept search and pinning (#314, done); "Copied" after hiding and
   copy on row click (#496, done); staying out of the taskbar and Alt+Tab, and
   the full-screen guard (#495, #509, #510).
4. Density and Larger text (ADR 0020, done); then an ADR for display choice and
   remembered per-display position, and opening on the focused window's display.
5. Footer tips, and `#tag`, `y:` and `a:` filters (tags need source support).
6. Recently cited and Recently added (needs a local history that stays on the
   device and, as PRIVACY.md promises, can be turned off and cleared).
7. Citation tray with page numbers (0.3, with #24).
8. Attachments: the attachment chip and the preview pane (needs attachment
   data from Zotero).
9. Later: one-line and at-cursor forms, the settings window, and alternative
   looks users can choose, including macOS Liquid Glass (which needs the newest
   macOS material API); native Wayland GlobalShortcuts portal integration
   and its system shortcut dialog need their own implementation and evidence.

Also open: Desktop Acrylic instead of Mica on Windows (#577), per-platform icon
weights, and the layered macOS app icon (#663).
