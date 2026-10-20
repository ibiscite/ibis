# Picker design

This is the agreed starting point for the picker redesign (#223), accepted by the
maintainer on 2026-10-19 after five rounds of prototypes. It describes what the
picker should look like and how its window should behave. It is brought into the
app in small steps (see [Rollout](#rollout)); until a step lands, the app keeps its
current appearance. User-selectable looks may come later; this is the default.

The prototypes were built outside the repository. Everything needed to implement
the design is written down here.

This includes longer-term targets as well as the current picker. Release scope
comes from [ROADMAP.md](../ROADMAP.md); the rollout below separates remaining
work from behavior already implemented. Native acceptance is recorded separately
in [testing.md](testing.md), not established by this specification.

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
| `--tile` / `--icon` | 32 px / 20 px | Type tile (24 px with no fill in Compact) and its icon. |
| Footer | 40 px | Text starts at `--inset + --row-pad`. |
| Smallest target | 32 px, 44 px on touch | |

Edges line up: the search icon is centred over the tile column and the
placeholder starts where row titles start:

```
--lead     = row-pad + (tile − icon) / 2       field padding before the icon
--lead-gap = (tile − icon) / 2 + row-gap       icon to placeholder
text x     = inset + row-pad + tile + row-gap  64 px (60 px on Windows)
```

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

Not implemented yet: `tokens.css` still uses the earlier fixed radii (rows
10 px on macOS, 9 px on Linux; tiles 7 px, 4 px on Windows) and a macOS menu
shadow of `0 16px 40px` black 22%. On GNOME, rows are
flat with the 6 px radius of Adwaita's sidebar and popover rows: a boxed list
cannot be concentric at an 8 px inset in a 14 px window.

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
platform's icon is used. Only the base set is implemented today. Seven type
icons say what a reference is: article, book, chapter, thesis, web page, report
and other (chosen from its CSL type). They never reuse an attachment drawing
(the document or the globe), so a row never shows the same picture twice.

**Per platform.** Only these change:

| | macOS | Windows 11 | Linux (GNOME) |
|---|---|---|---|
| Material | Standard material for the window; Liquid Glass only on the search bar (Rollout 9) | Desktop Acrylic (today: Mica) | Flat, opaque |
| Corners | See **Corners** above | | |
| Selected row | Filled accent, white text | Soft grey with an accent pill on the left | GTK's selection colour (filled accent) with its paired text |
| Keyboard focus ring | 2 px accent, 1 px gap | 2 px in the text colour around a 1 px surface-coloured stroke (Fluent's focus visual) | 2 px, the accent mixed half toward the text colour, 1 px gap |
| Search box | Frameless, large; with Liquid Glass, a 40 px capsule that also holds format and pin | Text box with an underline that turns accent on focus | Framed entry |
| Shortcut labels | ⌘ ⌥ ⇧ | Ctrl Alt Shift | Ctrl Super Shift |
| Copied | Pill near the top of the screen | Notice bottom-right | Toast at the bottom |

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
  Today the picker uses Mica; the switch to Acrylic is not implemented.
- **Linux.** Flat and opaque on `window_bg`, with the search entry on
  `view_bg`. Ibis does not use the GNOME 51 blur API, since GNOME's own apps
  stay flat.

## What shows, and when

| Element | Visible | Why |
|---|---|---|
| Search box | Always | The largest element, focused on open. Its placeholder lists what can be searched. |
| Format (for example "Pandoc ▾") | Always | What is pasted depends on it. One word, no icon. |
| More (Ctrl/⌘ K) | Always | The one door to every other action; each shows its shortcut, so the menu is also the cheat sheet. |
| Pin | Always, faint at rest | Tertiary text colour at rest, full strength on hover or while on, so it never moves or appears from nowhere. Its tooltip names the action ("Pin: keep Ibis open beside your document") and, once the key table assigns one, its key. The first time a click outside hides Ibis, a one-time footer tip explains the pin. |
| Filter chips | Once added | `#tag`, `y:2019-2022` or `a:name` followed by a space becomes a removable chip. Backspace in an empty box removes the last one. |
| Recently cited, Recently added, With PDF | While browsing | Shown when the search is empty or one is selected; hidden while typing so rows get the room. Tab cycles them. |
| Tag and author suggestions | While typing `#` or `a:` | Take the place of the tabs. Enter or Tab picks the first. |
| Row: type tile | Always | A 32 px tile at the row's start with the type icon on a quiet fill (24 px and no fill in Compact, so rows stay 40 px). Decorative: screen readers hear the type word instead. |
| Row: title and year | Always | Year at the right edge, in the same place on every row. |
| Row: authors and journal | Comfortable and Spacious | Tells apart references with the same first author and year. The journal is the container title (journal, book or proceedings), else the publisher. Spacious adds the item type as one word ("Article", "Book"). One line: authors are shortened as in a citation ("García", "García, Okafor", or "García et al." for three or more; screen readers hear every name), then the journal, each with an ellipsis (the key and library label shorten first, never below a few letters, and the type word stays whole; only when enlarged text leaves no room do they move under them); a match in the journal is always named, since it may be shortened. Compact shows the first author ("García et al.") after the title. |
| Row: why it matched | Only when hidden | "In the abstract: …sleep…" or "Tagged: …" when the match is not in the title or authors. |
| Attachment chip | When a file is attached | The platform's document (or globe) icon only, 16 px in a 24 px chip. "PDF, 12 pages" is its tooltip and accessible name, and it opens the preview. No empty column otherwise. |
| "In 2 libraries" | Only for duplicates | Explains why one reference may differ between sources; it is listed once. |
| Add button | Hovered or selected row | Takes the place of the type icon, so the row never reflows: a plus, or a check on a row already added. A row in the citation always shows a scarlet tile with a white check, the only scarlet on the row. A 32 px pointer target with its name in the tooltip ("Add García 2021 to citation"); it adds no Tab stop, because a listbox option holds no controls, and keyboards use Shift+Enter. |
| Citation tray | Once a source is added | Page number per source and the exact text to be copied, in scarlet. |
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

## Learning without a tutorial

Three layers, each reached from the one before by something on screen:

- **First minute:** type, press Enter, paste. Clicking a row copies it.
- **First week:** → previews the attachment and "Cite page" fills in the page;
  Shift+Enter builds a citation with several sources; Tab shows recent items.
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

The focused-window display is a target for rollout step 4. Today the picker uses
the display with the pointer (the nearest display if the pointer is between
displays). If pointer position is unavailable, it keeps its current display or
uses the primary one; it does not yet offer a display-choice setting or save
positions across restarts.

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
hiding, the search is kept in memory for a minute (#314); bringing back a search
that Escape cleared is still to do (rollout step 3). Escape never unpins.

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
Ibis offers to start at login: once during first-run setup, beside the
shortcut it teaches, and always in More and the menu bar or tray menu. It is
off until the person ticks it: Apple's App Review Guideline 2.4.5(iii) asks
Mac apps for consent first, and Raycast, Alfred, PowerToys and 1Password offer
it as a setting. A login start
leaves the picker hidden until the shortcut is pressed. The tick is read from
the system, never from `settings.json`, each time the picker or More opens and,
on macOS and Windows, each time the menu bar or tray icon is clicked. So
turning Ibis off in System Settings > General > Login Items (macOS) or Task
Manager's Startup apps (Windows) shows at once. Most Linux tray hosts do not
report clicks, so there the tray tick catches up with the desktop's startup
settings the next time the picker or More opens. If the system refuses, the tick shows the real state and a
sentence says where to change it; while macOS waits for the person to approve
the login item there, the picker says so each time it opens. Ibis Nightly and development builds register under their own
names and never replace stable Ibis's entry.

- macOS 13 and later: `SMAppService.mainApp` (Open at Login). Older macOS
  shows no choice.
- Windows: Ibis writes only the current user's `Run` key. Reading and
  removing also look at the all-users key, where an administrator may have
  added Ibis.
- Linux: an XDG autostart file in `$XDG_CONFIG_HOME/autostart`, skipped by the
  session if the program has been removed (`TryExec`). When an AppImage has
  moved, Ibis repoints the entry it wrote and keeps the person's own keys; a
  linked or hand-written entry is left alone.

## Rollout

Each step is one pull request against #223 (or an issue split from it), reviewed
with the states gallery on every platform. Steps 1–3 need no new data or
settings. Settings format 2 is recorded in [ADR 0018](adr/0018-settings-format-2.md);
[ADR 0019](adr/0019-update-checks.md) and
[ADR 0020](adr/0020-picker-preferences.md) add optional update and picker fields.
Density and Larger text are already remembered. Remaining choices kept across
restarts (window position per display, display choice, how long a search is kept,
which tips were seen) need an ADR before new `settings.json` fields are added.

1. **Done (#307).** Design values in `tokens.css`: type scale, densities, radii,
   motion and colour roles, without changing layout. Remaining motion work:
   enforce the 280 ms cap and reduced-motion behavior for every transition.
2. **Done in structure (#313); proportions and details in progress.** Picker
   layout: header (search, format, pin), rows, footer with library status and
   More, empty and no-result text. Remaining: header height, the per-platform
   search box, selected row and the citation colour (#490); the footer's Enter
   hint, the `?` key list and no-result actions (#491); rows that tell similar
   references apart (#492).
3. **Partly done (#314).** Window behaviour that needs no settings:
   outside-click hiding, keeping the search in memory for a minute, the Escape ladder, and staying on top only
   when pinned. Hide the quick picker from the taskbar, Dock and Alt+Tab; verify
   each platform separately (macOS already uses an accessory panel). Add the
   full-screen/presentation shortcut guard and its second-press override.
   Separately verify the macOS panel opens over full-screen apps in the current
   Space; the panel already has this configuration, which is not evidence that
   every native scenario passes.
   Status (#495): the quick picker asks to stay out of the taskbar on Windows
   and Linux, and the Linux native smoke checks the X11 hints. The full-screen
   guard (a second press within 2 seconds opens Ibis anyway) works where the
   window manager reports full-screen windows (X11). Windows and macOS have no
   detection yet: it needs native calls the safe dependencies do not offer, so
   there the shortcut always opens Ibis. Hiding the picker from Windows Alt+Tab
   needs a tool-window style for the same reason. (#509, #510)
   Also still to do in this step: undoing an Escape that cleared the search
   (reopening within a minute brings it back), the "Copied" notice after hiding, and a click on a row copies it
   (Shift+click adds it to the tray).
4. Density and Larger text are settings under ADR 0020 (#407, #408). Remaining
   work: an ADR for display choice and remembered per-display position, then
   change the opening default from the pointer's display to the focused window's
   display, with the pointer and last-position alternatives.
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
