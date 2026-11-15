# Foundations

The values every part reads, in [`tokens.css`](../../apps/desktop/src/tokens.css); principles and the model are in [design](../design.md).

## Tokens

- **Primitives:** raw values named by what they are (the 4 px space scale, ink, measured hues such as `--scarlet`). Only roles read them.
- **Semantic roles:** what a value is for, per mode (light, dark, Increase Contrast, forced colours): labels, fills, separator, surfaces, accent, selection, status, the corners (`--r-small`, `--r-control`, `--r-large`, `--r-win`), `--inset`, `--action-h`, `--key`, `--hit`.
- **Component values:** derived from roles by a written rule, never typed as a second number (`--key-gap`, `--footer-h`, `--heading-offset`). A part never names a value a token already holds: no second name for a radius, a target or a ring offset.
- Components read roles and component values only: no raw colours, `GrayText`, literal radii or literal shadows [D1]. A new need picks an existing step before adding a token.
- Light and dark come from `color-scheme` and `light-dark()`; components never read `prefers-color-scheme`. Roles default to CSS system colours so an unknown webview still follows dark mode and high contrast.
- `data-system` records the OS for fonts, modifier labels and native material; it never chooses a layout.
- **Exact macOS values:** colours, type styles, control heights and menu metrics are AppKit's own, measured by `docs/macos-metrics.swift` and written into `tokens.css`. Rerun it on each new macOS.

## Colour

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

## Materials and layers

- **Surfaces** [D4]: `--surface` (window), `--surface-grouped` (grouped content), `--surface-overlay` (every overlay), `--surface-field`. Settings cards are grouped content. No other surface colours.
- **Window glass** [C13]: on macOS one glass sheet for the whole panel (Liquid Glass, popover vibrancy fallback) under a surface veil, so colour shows through but never readable shapes. Never glass on glass or CSS blur posing as the desktop. Elsewhere an opaque surface.
- **Views, not cards**: the palettes and sheets are views of the window (`ui/ViewFrame`), not layers over it. The picker's content swaps out; the view keeps the picker's header and footer geometry and its list grid, and has no card, border, corner, shadow or dimmed backdrop of its own. Back (the chevron where the bird sits, the footer's Back, Esc) returns.
- **Readable overlays** [C9][C13]: everything that floats (menus, pop-overs, the peek card, the reader's controls) is one raised layer, `ui/Overlay` (`.ui-overlay`): `--surface-overlay`, a step off the window in either mode as a Mac menu is (never the window's own white or black), the panel's glass where it applied, and one shadow. No part paints its own.
- **Elevation** [D11]: one overlay shadow, on that layer only; none on rows, chips or inside an overlay. In dark mode fills and hairlines separate layers first.
- **Layers** [D15]: one z-order (`--z-*`); one overlay of each kind at a time; overlays keep `--inset` from the window edge and never leave it.
- **Increase Contrast** uses the Mac's stronger set: stronger labels and marks, a solid ring, no scroll fades.
- **Reduce Transparency** makes every material opaque.
- **Forced colours** collapse roles to system pairs (`Canvas`, `CanvasText`, `Highlight`, `ButtonText`, `GrayText`): opaque materials, no shadows, a 1 px control edge, scarlet becomes `CanvasText`.

## Type

- The system UI font (SF Pro on macOS), never another platform's; `--mono` only for citation keys and key-format output. One exception, for reading: a reference's **title** in the Inspector and the format preview uses Ibis's serif, the website's reading face, as iA Writer and Ulysses set reading text apart from chrome. Everything else stays in the system font.
- The Mac's text styles as `--type-*` roles: search (title 1, frameless like Spotlight), title (row titles), body (text and menus), meta (descriptions, footer, shelf), label (the smallest), display (Settings headings and the welcome).
- Weights 400, 500, 600; no uppercase transforms; italics only for "why it matched"; `tabular-nums` on years, counts and pages.
- Type and vertical sizes are rem; radii and hairlines stay px. Larger text multiplies the root by `--larger-text-scale` and grows the window by the same factor; system scaling multiplies with it, capped at 2.

## Spacing and sizes

- **Spacing** [C1]: S = `--inset` on every OS. Every fill keeps S from the window edge and from any divider it touches; the list's viewport starts S below the header divider and ends S above the shelf. 4 px grid; 2 px only as an optical nudge.
- **Control sizes**: the picker uses Apple's large controls and Settings the regular ones, as Spotlight and System Settings do. Targets grow to 32 px without changing the drawn size.
- **The window grid** [C3] (decision 0053): three lines, set once in `tokens.css` and drawn by `ui/Band`, so no screen places anything.
  - **Leading edge** (`--row-start`, 20 px from the window): where a tile's plate starts. Anything without a glyph that starts a line starts here: list headings, the notice standing in for a heading, the footer's library status.
  - **Lead column** (`--tile`, 32 px from the leading edge): one glyph per band, centred: a row's type glyph or tile, the bird, Back's chevron, the shelf's beak, a held row's digit. A glyph in it is always 20 px (`--icon-tile`). A band either has a lead column (`icon` given, `null` for an empty one in a list of glyph rows) or none (headings, notices, status); nothing else sits in it.
  - **Title column** (`--label-start`): row titles, the query, view titles, the shelf citation.
  - **Trailing edge** (`--row-start` from the far side): where the last ink ends: years, the heading's count, palette keys, the format pop-up's chevrons and the footer's last key cap (`--cap-end` puts a cap's fill there). A quiet button's hover plate may run past an edge; its words and glyph never do.
  Distances run from the logical start edge, also in RTL. Menus are the one exception, as NSMenu draws them: a menu's section heading sits on its items' label column.
- **Type, four sizes in the picker** (`--type-*`): search 22/26 (the query and a view's search), title 15/20 semibold (result titles), body 13/16 (everything else that is read: palette and command rows, meta lines, menus, notices), label 12/15 (headings in semibold, the footer, key caps). Lead 17 is for an empty state's or choice screen's title and display 20 for Settings headings and the welcome; nothing else. A one-line action row (palette, commands, menus) is body, never title.
- **Spacing, one 4 px scale** (`--space-1` to `--space-10`): the inset 8, a row's padding and the gap after its glyph 12, the gutter between sections 16. Parts read the roles (`--inset`, `--row-pad`, `--row-gap`, `--gutter`), never a step directly, and a screen sets none of them (`ui/ownership.test.ts`).
- **Rows** [C4]: two densities: Compact (one line; a long title ends in an ellipsis) and Comfortable (two, the default). Density changes rows only. Rows are 2 px apart in results.
- **One steady window** [C10]: the picker keeps one size the whole time it is open, in every state; nothing measures content to choose a height and nothing animates a resize. Medium 720 × 480 is the default, minimum 480 × 360; Look › **Window size** offers Small, Medium and Large. A dragged size shows as Custom and is kept per display.
- **Nothing clipped, one scroller**: no state is ever cut off; only the middle area or an open menu scrolls, never two at once. The inspector is the one exception: a sidebar scrolls apart from the list beside it, as in Preview, and its actions stay pinned at its bottom so none scrolls out of reach at the smallest window or with Larger Text. In a short frame with large text the header gaps and footer hint labels give way first.
- **Width classes** [C17]: below 28.75 em footer hints show keys only; at 60 em the preview sits beside the list, below it replaces it.

## Corners

- **Corners** [C15][D3]: three steps from `--radius`, and nothing else: `--r-small` (6: tiles, key caps, citation tokens, small marks), `--r-control` (8: controls, rows, fields, menu items) and `--r-large` (12: menus, cards, the copy receipt), plus the window's `--r-win` (16, the Mac's own), capsules and the app icon. Nested shapes are concentric by choosing the step below: the window (16) inset 8 holds rows at 8; a menu (12) padded 4 holds items at 8; a control (8) holds a key cap or citation token at 6.
- **Neighbours share a radius**: shapes side by side at one depth use one radius and one gap.
- Overlays and Settings cards take `--r-large`.
- **Capsules** (`--r-full`) only for switches, count and tag pills and progress tracks. Nothing hangs off a shape's edge: a control that belongs to a shape sits inside it.
- The miniature pictures in Settings keep the picker's radii; see [Previews](settings.md) in Settings.

## Icons

- One `Icon` primitive draws **Phosphor Regular** (MIT, `@phosphor-icons/core`), vendored unchanged in `brand/icons/` and built by `python3 brand/build.py` ([decision 0053](../decisions/0053-window-grid-and-icons.md)). Phosphor draws on a 256-unit grid that is a 16 px pixel grid: at 16 px its stroke is 1 px and its horizontals and verticals fall on whole pixels, so it stays crisp at 1× on Windows and Linux (checked) and reads like SF Symbols beside system text; at 20 px the stroke is 1.25 px.
- **Two sizes, chosen by place, not by part:** 16 px (`--icon`) inside controls and menus; 20 px (`--icon-tile`) in the lead column of any list. These stay fixed under Larger Text.
- **Named by meaning:** each file is named for what it stands for (`peek`, `settings`, `quit`, `reset`, `history`), and `brand/icons/SOURCES.json` names its upstream glyph. One meaning, one glyph: a glyph is never borrowed for a second meaning (no contrast circle for Peek, no × for Quit). A new meaning adds a file; nothing is redrawn or normalized.
- A glyph Phosphor lacks is drawn on its grid at Regular weight (a 16-unit stroke, round caps and joins, outlined to a fill), so it cannot be told apart from the set.
- App icons come from the system, including the installed Zotero icon on its setup card (bundle id `org.zotero.zotero`, read through NSWorkspace). Rasterize at the displayed size before exporting; a failed read or decode keeps the action's distinct fallback.
- Ibis never ships Zotero's logo or other apps' artwork.
- Seven type icons (article, book, chapter, thesis, web page, report, other) stay distinct from attachment drawings. Toggled-on keeps the same outline; the control's fill carries state.
- Settings page icons use Apple's icon shape: a white glyph from the set above on a `--page-*` tile, one family for every page. Mac icon layers are flat, with no added bevels or shadows.

| Source action | Icon (shared `Icon` glyph) |
| --- | --- |
| Show in Zotero / Open Zotero | Installed app icon; fallback `app-window` |
| Open thesis.bib | `file-text` (`article` in the shared set) |
| Show in Finder / Show in Folder | `folder-search` |
| Choose Library… / Choose a Library File… | `library-big`, in More, footer recovery and empty choices |

## The bird

The bird appears only where Ibis itself is the subject, in the one treatment of that context, and a view holds at most one beyond the search bar's. It never labels a control, a page, a row or a setting, and never mirrors. In contrast modes every flat bird is `CanvasText`.

| Context | Treatment | Size |
| --- | --- | --- |
| Ibis as an app: About, the Dock, Finder, the paste guide's drag icon | The real app icon on its tile, never redrawn | `--mark-app` |
| Greeting: welcome, the first open | The depth mark, no tile | `--mark-greeting` |
| Waiting: opening a library | The flat mark, breathing | 2.5 rem |
| Working: the search bar | The flat mark in `--scarlet`, eye open, in the lead column | `--mark-search`: the column's 32 px, so its ink is the query's line height |
| System chrome: the menu bar | The one-colour bird the system tints | The system's |

## Key caps

- One cap per shortcut, written as menus write it: ⌘K, ⇧⌘F, ↵ on macOS; Ctrl+K and Ctrl+Alt+Shift+K, modifiers in that order, on Windows and Linux (`ui/KeyCaps`). A pointer binding reads ⌘-click (Ctrl+click elsewhere).
- Caps appear only in the footer, the notice slot and key lists. Menus, Settings and a held row's number write shortcuts as plain text in native order (⇧⌘C), secondary like a menu's key equivalent [C8] (`kbd[data-plain]`).
- A cap is `--fill-control` with `--r-small`, label type, never an outline or scarlet. Inside a quiet button it keeps its fill; on the primary it is the pressed fill. A trailing cap in a small button is concentric: `--key-gap` above, below and at the end. A one-symbol cap (↵) is square, and ↵ is lowered .1em so it sits optically centred.
- "or" alternatives keep two caps. Decorative caps are hidden from screen readers; `aria-keyshortcuts` names the keys.

## Motion

- **Three steps** (`tokens.css`): `--dur-1` (120 ms) answers a touch: a press, a check drawing. `--dur-2` (180 ms) brings something in: a menu or pop-over, a line appearing, a delight. `--dur-3` (240 ms) moves a region: a sheet, a panel. Entrances use `--ease-out`, exits `--ease-in`; what follows the pointer or the keys settles on `--ease-spring`, which never overshoots.
- **The standard movements**, each in its shared part: everything that floats (`ui/Overlay`) unfolds from the edge next to its button (a clip and a fade, so its measured place never changes) and closes at once; palettes and sheets share `ui/ViewFrame`'s rise and fade into the window, leaving at once; a pressed button shrinks to `--press-scale` and springs back. Selection, focus and hover change at once. Chevrons never rotate; nothing bounces.
- **Never** animate anything longer than 280 ms. Never animated: list content while typing, row height, the caret, layout on hover, the citation text, ticking counts, focus rings, anything that loops except a busy indicator.
- Reduce Motion: no translate, scale or slide; entrances may use the short `--dur-1` fade (100 ms), `--dur-2` and `--dur-3` end at once, and `--press-scale` is 1. Other effects show their end state.

## Accessibility

- Every action has a key and a visible, named control; hover is never the only way to see something; visible labels match accessible names. Editing the query is typing, not an action: Esc clears it, as in Spotlight, and the field has no clear button.
- Focus order: search, header pop-ups, pin, browsing tabs, footer actions, More. The list is not a Tab stop (↑ ↓ move it from search). Nothing takes focus except search on open.
- Screen readers hear a row as title, first author, year, container, type and why it matched. "Copied" is announced politely; a Rows change is announced once.
- An input method's confirming Enter never copies or pastes. Every release passes the whole citation with VoiceOver and NVDA and with CJK input methods (decision 0032).
- **Capitalisation:** on macOS, menu items, buttons, window, sheet titles and choice rows use title case ("Keep Window Open", "Try Again", "Find Them on This Computer"); everything else is sentence case. One string never appears in both styles; a sentence that names a command writes it as the command does ("Copy Only is set for this app").
- **Settings words:** a row label is sentence case, as System Settings writes it ("Window size", "Open at login", "Show in menu bar"); buttons and menu items are title case ("Back Up…", "Check for Updates"). A row's description is one or two sentences, each ending in a full stop. Shortcut rows describe an action in sentence case ("Move through results"), naming a sheet or command by its title ("Show Keyboard Shortcuts").
- **Punctuation:** typographic apostrophes and quotes (’ “ ”) and the ellipsis character (…), never three full stops, in everything a person reads; a test reads `en-US.ftl` for typewriter ones.
- **Long text:** containers grow to fit wrapped text with the same padding; in Settings the control moves below the label. Data truncates with a tooltip; file names shorten in the middle. Budget +35 % for German and Greek. RTL uses logical properties; citation keys stay LTR. Check at normal, Larger and 200 % text.

## Per platform

- **macOS is the standard now.** Every system uses the macOS design: one code path, the same sizes, radii, controls and layout.
- **Windows and Linux** share the macOS look with their own UI font, modifier keys and key labels (Ctrl+K), selection colours where the system demands them, and window corners (the system's own: square on Windows 10). On Linux, System appearance follows the desktop's dark preference ([decision 0041](../decisions/0041-linux-system-appearance.md)), and a Copied notice without a compositor is an opaque toast, drawn at once with no fade.
- OS-specific designs for Windows and GNOME come later as their own value layers in `tokens.css` on the same components. Start from these recorded choices: Windows Acrylic with a footer band, Fluent radii (window 8, control 4), a deeper dark selected row (#0067C0, white text) and Fluent settings-card rows; GNOME opaque `window_bg`, libadwaita radii (window 15, row 7), AdwActionRow settings rows and menu checks after the label.

| | Rule |
| --- | --- |
| Material [C13] | One glass sheet on macOS; an opaque surface elsewhere. |
| Accent [C14] | The Accent chosen in Look; with Follow the Mac, the system accent where the webview reads it, else `--accent`. |
| Selected row [C4] | Accent fill with its contrast-safe paired `--selected-text`. |
| Switch | Measured from `NSSwitch`, the same everywhere. |
| Window corner [C15] | `--r-win` on macOS, matching the native panel mask; the system's own elsewhere. |
| Follows the system | Modifier keys and key-cap labels, the UI font, accessibility settings and window handling. |
