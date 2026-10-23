# Design system foundations

This page holds the tokens, shapes, states and components every Ibis window
uses. It builds on [design.md](../design.md) (the picker spec), ADR 0021 (React
Aria plus three CSS token layers), `apps/desktop/src/tokens.css` and the soft
depth look (variant B, branch `design/depth-prototype`). Where it disagrees with
an accepted ADR, the ADR wins. The Settings window and shortcut recorder are in
[settings.md](settings.md); the citation shelf is in [preview.md](preview.md).
Work is tracked in #761–#775.

## Canvas decisions C1–C18 (2026-10-23)

The maintainer's design canvas is the visual source for the picker, first run,
Settings and the preview; this table is its written form. These decisions
supersede any older text on this page, in [design.md](../design.md),
[preview.md](preview.md) and [settings.md](settings.md) that disagrees. The PR
that implements one deletes or rewrites the text it supersedes. Work is
sequenced in #940 and on the tracking issue (Lane UI).

| # | Decision |
|---|---|
| C1 | **One spacing rule.** S = `--inset` = 8 px on every OS (Windows moves from 4 to 8). Every fill (search field, row, shelf, icon-button hover, footer action, menu) keeps S from the window edge and S from any divider on each side that touches one. So the header is its tallest control + 2S, the footer is its 24 px actions + 2S, and the list starts S below the header divider and ends S above the shelf. |
| C2 | **Header.** Heights: macOS 52 (36 px pin button), Windows 48 (32 px field and buttons), GNOME 50 (34 px field and buttons). A 1 px divider under the header on every OS, always. Order: search glyph, query, clear ×, format pop-up (filled, no divider), pin. The trailing button keeps S to the edge, top and bottom. |
| C3 | **Two columns.** Icons, the search glyph, the status dot and type tiles are centred on the 32 px tile column (`inset + row-pad + 16`); text starts on the title column (`inset + row-pad + tile + row-gap` = 64 px): row titles, the search query, the status text and the shelf citation. The search glyph sits 6 px inside the tile column on each side. |
| C4 | **Rows.** 58 px, 2 px apart everywhere (results, Recently added, choices). The citation key is hidden in rows; the year is trailing and centred vertically. Selection: accent fill on macOS and GNOME; subtle fill plus a 3 × 16 px accent pill on Windows. |
| C5 | **Pointer.** Click selects (the shelf shows its citation); double-click or the footer Copy does what Enter does. ⌘-click or Ctrl-click, or the + on a hovered tile, adds to the citation. Right-click opens More for that row. Only copying closes the window. |
| C6 | **Pin.** A neutral icon button at the header's end, always visible. On: filled glyph on the pressed fill, never the accent. ⌘⇧P / Ctrl+Shift+P; also in More as "Keep window open" / "Stop keeping open". Escape never unpins. |
| C7 | **Shelf.** One 40 px line with the beak quote mark hanging before the citation (mark at the tile column's end, text on the title column). Shown while there are results; hidden on empty, error and choice screens. With several references the citation itself holds them as segments (no chips line): each segment reserves 1 px on each side; under the pointer it becomes a soft pill (radius 6) with a 16 px round remove button inside its end over a 6 px fade of the text, so nothing moves and neighbours are never covered (this holds for zero-width separators such as BibLaTeX's). Pointing at the × darkens its circle. From the keyboard ← → select a segment as an accent-filled token and Delete removes it. On copy the shelf shows a closing beak after the citation and "Copied" at the line end. |
| C8 | **Footer.** Status dot and text, then "Copy N as <format> ↵", "Shortcuts ⌘/", "More ⌘K". Actions are 24 px fills. Key caps only in the footer and lists, never inside buttons; buttons use quiet trailing text ("Cite this page ⌘↩"). The macOS Return cap is the return-arrow glyph. |
| C9 | **Menus.** Items padded 10 px on both sides; a leading check column only when an item in that menu can be checked (the format menu), never in More. Separators inset 10 px with 4 px above and below. The format menu's filter text aligns with the item labels, and the menu opens 4 px below the pop-up, aligned to its trailing edge. macOS shortcut text uses native modifier order (⇧⌘C). |
| C10 | **Size follows content.** Every state is as tall as its content: search and footer alone (nothing to suggest), one hint line, Recently added, or a list of choices. Results grow it in 220 ms with the top edge fixed, to the remembered height; it never shrinks while open and starts compact next time. Reduced motion: no animation. |
| C11 | **Compact start.** Recently added (3 compact rows) when the library has dates; otherwise one rotating tip ("1 of 6"); otherwise nothing, just search and footer. The placeholder carries the hint: "Search by title, author, year or key". |
| C12 | **Choices screens.** Empty library, missing file and first run use one pattern: a title and one line on the tile column's edge, then result-style rows (tile, title, one line of help, optional trailing tag, chevron). Empty library: Zotero ("Found on this Mac/PC/computer") and BibTeX or BibLaTeX file; a CSL JSON or YAML row joins them only when those sources ship (0.5, ROADMAP); "You can drop a file here too." Missing file: "Find example.bib" (accent tile) and "Use another library". First run is two steps of the same pattern: where your references live, then what you write in (Markdown (Pandoc), LaTeX, Typst); the open shortcut and start-at-login live in Settings and the footer tip. No mark in first run. |
| C13 | **Depth per OS.** macOS: one glass sheet for the whole panel (Liquid Glass, vibrancy fallback); the shelf is a tint on it, menus on glass. Windows: Desktop Acrylic; the shelf is a card (tint, 1 px stroke); the footer is a band with a top divider. GNOME: opaque, no blur; footer band with a divider; shelf full-bleed between dividers. Reduce transparency and contrast modes make every layer solid. |
| C14 | **Accent.** The system accent with fallbacks: macOS #007AFF / #0A84FF, Windows #005FB8 / #60CDFF, GNOME #3584E4 / #78AEED (light / dark). Scarlet only for the citation mark, the added tile and the brand. |
| C15 | **Radii.** macOS window 16, row 8, tile 6, control 8, menu 12 (item 6, padding 6). Windows window 8, row/tile/control/item 4, menu 8 (padding 4). GNOME window 15, row 7, tile/control/item 6, menu 12 (padding 6). Citation segment pill 6. |
| C16 | **Mark.** One continuous silhouette with soft depth: a top-to-bottom tone, a soft light along the top rim, a soft shade along the bottom rim and a short shadow, computed from the whole shape (one SVG filter), never per piece. The beak quote matches; below 24 px it drops the shadow. App icon glyph about 73 % of the tile. |
| C17 | **Preview (wide window).** At 960 px or wider the list keeps 400 px and the pane takes the rest; below that the preview replaces the list. Pane toolbar and bottom row are 48 px with S padding; the page area keeps S from the divider and the edge; the content area ends S above the shelf. Page field shows the printed label with "PDF n of m"; "Cite this page ⌘↩"; Shift extends to a range. |
| C18 | **Settings.** Its own window with a searchable sidebar list of pages (General, Shortcuts, Libraries, Citations, Preview, Advanced, About), each a stack of titled groups of the same rows; 720 × 560, down to 560 wide where the sidebar collapses. |

## Decisions D1–D17

Token names are the target names. The first migration (#761) exposes aliases of
the current values and moves only the old Search focus consumers to
`--focus-ring-*`. Other consumers deliberately retain their existing tokens;
the alias inventory alone does not establish the final per-OS appearance.
The platform tests audit the live root, menu, notice and shelf scopes, optional-value
fallbacks and the unscaled 16 px/rem spacing grid. Native menu metrics, the
notice glyph/pill insets and border/optical formulas have exact exceptions.
Synthetic local-scope fixtures cover pointer modes and inherited overrides.
These are bounded dependency checks, not a cascade/specificity interpreter;
browser comparisons verify the winning values and rendered appearance.

| # | Decision | Target token(s) |
|---|---|---|
| D1 | **Components read role tokens only**, never raw colours, `GrayText`, literal px radii or literal shadows. | all below |
| D2 | **One radius chain per OS.** Set only `--r-win`, `--inset`, `--r-min`, `--r-menu`, `--menu-pad`; everything else derives. New: `--r-control` (buttons, fields, pop-ups), `--r-key` (key caps, chips' inner parts), `--r-card` (= `--r-win`), `--r-dialog`, `--r-full: 999px`. | §3 |
| D3 | **Per-OS radius values:** macOS 15: win 12, control 6, menu 8 (verify). macOS 26: win 28, control capsule in the header, 8 in content, menu 12. Windows 11: win/overlay/card 8, control/row/tile 4. GNOME (libadwaita 1.8+ look): win/card 15, derived row 7, control/tile 6, popover 12. | `--r-*` |
| D4 | **Surfaces are four layers:** `--surface-window` (base) < `--surface` (content sheet) < `--surface-raised` (band, cards, shelf) < `--surface-overlay` (menus, popovers, dialogs). Values from soft depth B. | §2 |
| D5 | **Text has four levels:** `--text`, `--muted` (secondary, always 4.5:1), `--text-tertiary` (icons at rest, placeholder hints, 3:1 minimum, never for sentences), `--text-disabled`. | |
| D6 | **Accent is always the system's.** `--accent` (fill), `--accent-text` (accent as text or glyph, 4.5:1), `--text-on-accent`. Ibis never offers an accent override. Native accent projection starts with macOS; Windows and GNOME providers follow (#763). | |
| D7 | **Selection is a fill, never a ring.** macOS and GNOME: full accent fill with `--text-on-accent`. Windows: subtle fill plus a 3 × 16 px accent pill. `--row-inactive-background` for a kept-open picker that lost focus. Menu highlight: accent fill on macOS, subtle fill on Windows and GNOME. | `--row-active-*`, `--menu-highlight*` |
| D8 | **Focus ring only on keyboard focus** (`:focus-visible` / `data-focus-visible`). macOS: 2 px accent, 1 px gap. Windows: 2 px text-colour outer, 1 px surface inner. GNOME: 2 px half-accent mixed to 3:1, drawn **inside** (offset −2 px) so scroll containers never clip it. Contrast modes: 2 px `CanvasText`. | `--focus-ring-*` |
| D9 | **Scarlet marks the citation only:** the shelf's decorative beak quote mark (the supplied two-head SVG from #888, 20 px tall, scaled with type), the added tile and the logo. The mark sits at the tile column's end on the citation line, mirrors in RTL, dims to 45 % while pending and uses `CanvasText` in contrast modes. The citation's text is in the text colour ([preview.md](preview.md)). Never on a control, link, toggle, progress, badge, selection, focus or error. On an accent-filled selected row the added tile becomes a `--text-on-accent` tile with a scarlet check, so a red or orange accent never meets scarlet fill-on-fill. | `--citation`, `--citation-tile` |
| D10 | **Status is Done / Look / Stop / Info**, Ibis-owned hues (Stop stays rose, apart from scarlet and Fluent's critical red), always with words or an icon. `--status-info` = `--accent-text`. | `--status-*` |
| D11 | **Elevation is five steps** with per-OS shadows: `--shadow-card`, `--shadow-raised`, `--shadow-overlay` (one value for every menu), `--shadow-dialog`, `--shadow-toast`. None in contrast modes. | |
| D12 | **Space** is the 4 px grid: `--space-1..10` = 4, 8, 12, 16, 20, 24, 32, 40. **Targets:** 32 px minimum, 44 px under `pointer: coarse`. Visual height may be smaller (28 px macOS toolbar controls) when a `::before` keeps the 32 px target. | |
| D13 | **Type:** the platform's own UI font (SF Pro; Segoe UI Variable; the GNOME interface font, Adwaita Sans since GNOME 48). One scale: 22/28 search (macOS), 16/22 search elsewhere, 15/20 title, 14/20 body, 13/18 meta, 12/16 label; 20/28 heading for dialogs; 28/36 large title for Windows Settings pages. **Monospace for citation keys and key formats only**; formatted citations use the UI font; key caps use the UI font. | `--type-*` |
| D14 | **Motion:** 120 feedback, 200 panel, 280 layout (hard cap). Easing per OS: macOS `cubic-bezier(.2,.8,.2,1)`, Windows `cubic-bezier(0,0,0,1)`, GNOME `cubic-bezier(.33,1,.68,1)`; exits use `--ease-in`. Reduced motion: fades of at most 100 ms, nothing moves. | `--ease-out`, `--ease-in` |
| D15 | **Overlays:** one z-order (content 0, sticky chrome 1, popover/menu 10, tooltip 20, modal 30, in-window toast 40). Escape closes the top layer only; focus returns to the invoker; overlays keep 8 px from the window edge and never leave the webview (ADR 0021). | `--z-*`, `--overlay-margin` |
| D16 | **Never confirm, offer undo.** Destructive actions apply at once with an 8 s Undo toast. Ibis never deletes user files. | |
| D17 | **Settings vocabulary:** a row is label + optional description on the start side, one control on the end side, 48 px minimum (56 with description), in groups on `--surface-raised` cards (Windows: one `SettingsCard` per row at radius 4 = `ControlCornerRadius`; GNOME: boxed list at 12) or grouped form rows (macOS). Every setting applies at once. Window: macOS toolbar of icon tabs, 680 wide, height per pane, page title in the title bar; Windows `NavigationView`, page title 28/36; GNOME `AdwPreferencesDialog`. Full spec and recorder anatomy: [settings.md](settings.md). | `--type-title-large` |

## 1. Principles

They extend the eight rules in [design.md](../design.md) and never contradict
them.

1. **Native first, Ibis voice second.** Follow each platform's convention for
   material, radius, selection, menu metrics, font and shortcut labels. Ibis
   keeps its anatomy, icon drawings, wording and citation scarlet.
2. **One anatomy, every size.** Search, list, detail, shelf, footer, in that
   order, from a 360 px window to a wide one. Layouts fold rather than reorder.
3. **Quiet by default.** At rest only content has full contrast. Chrome is
   tertiary until needed, never invisible: anything you can click is visible
   without hovering.
4. **Every input is equal.** Keyboard first: every action has a key, every key
   is discoverable from the screen. Mouse, touch and voice can do everything the
   keyboard can (§12). Focus never leaves the search box unless the user moves
   it.
5. **Selection is a fill; focus is a ring; neither glows.**
6. **Progressive disclosure, never modes.** New users see one path (type,
   Enter, paste); power features are one key away, never behind an "advanced"
   switch.
7. **Undo over asking.** Nothing asks "are you sure?".

## 2. Colour

### 2.1 Layers (ADR 0021)

1. **Base:** raw values, never read by components: `--scarlet-600 #c23a1c`,
   `--scarlet-500 #d2401f` (brand), `--scarlet-700 #982d16`, `--scarlet-300
   #ff7a5c`, `--scarlet-200 #ff9d87`, status ramps, libadwaita and Fluent
   neutrals.
2. **Role:** the tokens below. Defaults use CSS system colours so an unknown
   webview still follows dark mode, the accent and high contrast.
3. **Look:** `:root[data-platform=…]` overrides, then increased contrast, then
   forced colours, then the gallery contrast preview. Contrast blocks come last.

### 2.2 Roles and per-OS values

Light / dark. † marks a value kept from today's `tokens.css`.

**Surfaces**

| Role | Token | macOS | Windows 11 | GNOME (libadwaita 1.6+) |
|---|---|---|---|---|
| Window base | `--surface-window` | material under the 55 % veil†; opaque fallback `#f2f2f4` / `#1c1c1e` | Desktop Acrylic (picker), Mica (Settings); fallback `#f3f3f3` / `#202020` | `window_bg_color` `#fafafb` / `#222226` |
| Content sheet | `--surface` | `#fcfcfd` / `#242426`† | transparent on the material; the list is a layer card | `window_bg_color`; entries on `view_bg_color` `#ffffff` / `#1d1d20` |
| Raised (search band, cards, shelf) | `--surface-raised` | `#ffffff` / `#2c2c2f` | `LayerFillColorDefault` white 50 % / `#3a3a3a` at 30 % | `card_bg_color` `#ffffff` / white 8 % |
| Overlay | `--surface-overlay` | `#f6f6f8` / `#28282c` at 94 % with blur; opaque without | fallback `#f9f9f9` / `#2c2c2c`† | `popover_bg_color` `#ffffff` / `#36363a` (verify) |
| Field | `--surface-field` | black 5 % / white 8 %† | `ControlFillColorDefault` white 70 % / white 6 %; focused `#ffffff` / `#1e1e1e` | `view_bg_color` |
| Scrim | `--scrim` | black 20 % / black 40 % | black 30 % both | black 25 % / black 45 % |

**Fills**

| Role | Token | macOS | Windows 11 | GNOME |
|---|---|---|---|---|
| Control rest | `--fill-control` | black 5 % / white 8 %† | white 70 % / white 6 % + elevation stroke | `currentColor` 10 % |
| Control hover | `--fill-control-hover` | black 9 % / white 13 %† | `#f9f9f9` 50 % / white 8 % | `currentColor` 15 % |
| Control pressed | `--fill-control-pressed` | black 13 % / white 18 % | `#f9f9f9` 30 % / white 3 % (lighter in Fluent) | `currentColor` 30 % |
| Quiet hover | `--fill-subtle-hover` | black 4.5 % / white 6 %† | black 3.7 % / white 6 % | `currentColor` 7 % |
| Quiet pressed | `--fill-subtle-pressed` | black 8 % / white 10 % | black 2.4 % / white 4 % | `currentColor` 16 % |
| Disabled control | `--fill-disabled` | rest fill at 50 % | `#f9f9f9` 30 % / white 4 % | rest fill, control at opacity .5 |

**Text**

| Role | Token | macOS | Windows 11 | GNOME | Target |
|---|---|---|---|---|---|
| Primary | `--text` | `#16181d` / `#f5f5f7` over the veil† | black 89 % / `#ffffff` | `rgb(0 0 6 / 80%)` / `#ffffff` | ≥ 7:1 |
| Secondary | `--muted` | `#4c5260` / `#c0c3cc`† (`secondaryLabelColor` is too faint) | black 62 % / white 79 % | `#5e5e66` / `#a6a6af` (`.dim-label` is too faint) | **≥ 4.5:1** |
| Tertiary | `--text-tertiary` | black 45 % / white 45 % | black 45 % / white 54 % | `currentColor` 45 % | ≥ 3:1, glyphs and hints only |
| Disabled | `--text-disabled` | black 25 % / white 25 % | black 36 % / white 36 % | control at opacity .5 | aim ≥ 2:1 |
| On accent | `--text-on-accent` | `-apple-system-alternate-selected-text`† | `#ffffff` / `#000000` | `accent_fg_color` `#ffffff` | ≥ 4.5:1 on `--accent` |

**Accent, selection, focus**

| Role | Token | macOS | Windows 11 | GNOME |
|---|---|---|---|---|
| Accent fill | `--accent` | `controlAccentColor` (system) | `SystemAccentColor`: `AccentDark1` light, `AccentLight2` dark; injected from Rust (WebView2 has no `AccentColor`) | `accent_bg_color` from the portal `accent-color` (GNOME 47+); injected from Rust (WebKitGTK 4.1 is GTK 3) |
| Accent as text | `--accent-text` | darkened in light until 4.5:1 | `AccentDark2` light / `AccentLight3` dark | `oklab(from accent l→min(l,.5))` light, `max(l,.85)` dark |
| Selected row | `--row-active-*` | `selectedContentBackgroundColor` + white† | subtle fill 6 %, text unchanged, accent pill 3 × 16 px, radius 1.5 px† | full accent with `--text-on-accent`† |
| Selected row, window inactive | `--row-inactive-background` | `#dcdcdc` / `#464646`, text unchanged | same subtle fill, pill `--text-tertiary` | `currentColor` 10 % |
| Menu highlight | `--menu-highlight*` | accent fill + white | subtle fill, text unchanged† | `currentColor` 7 %† |
| Focus ring | `--focus-ring-*` | accent, 2 / +1 px† | text 2 px + surface 1 px† | `color-mix(accent 50 %, text)` 2 px, offset −2 px |
| Text selection | `::selection` | accent 30 % | accent + white | accent 30 % |

**Separators and strokes**

| Role | Token | macOS | Windows 11 | GNOME |
|---|---|---|---|---|
| Separator | `--separator` | black 10 % / white 10 %† | black 6 % / white 8 %† | `currentColor` 15 %† |
| Control edge | `--stroke-control` | none† | black 6 % / white 7 %, bottom black 16 % / white 9 % | none |
| Field edge | `--stroke-field` | none | bottom black 45 % / white 54 % (3:1); focused 2 px `--accent` | none |
| Overlay edge | `--stroke-overlay` | black 10 % / white 10 % hairline† | black 6 % / black 20 % | black 14 % / black 50 % |
| Strong (3:1, contrast modes) | `--line` | `CanvasText` | `CanvasText` | `CanvasText` |

**Status and brand**

| Role | Token | Light | Dark | Over the macOS veil | Contrast modes |
|---|---|---|---|---|---|
| Citation mark | `--citation` | `#c23a1c` | `#ff7a5c` | `#982d16` / `#ff9d87` | `CanvasText` |
| Added tile | `--citation-tile` | `#c23a1c` | `#d2401f` | same | `CanvasText` |
| Done | `--status-done` | `#1a7f37` | `#5bc26f` | `#135f29` / `#6fca81` | `CanvasText` |
| Look | `--status-look` | `#8a5a00` | `#e3b341` | `#734b00` / `#f0c25a` | `CanvasText` |
| Stop | `--status-stop` | `#c4314b` | `#ff8a8a` | `#9b273b` / `#ff9c9c` | `CanvasText` |
| Info | `--status-info` | `--accent-text` | `--accent-text` | same | `CanvasText` |

Native Stop colours are not adopted: Fluent's `#c42b1c` and libadwaita's
`#c01c28` sit almost on Ibis scarlet, which would make "failed" and "this is
your citation" one colour. Done and Look stay close to the platform hues.

### 2.3 Appearance and contrast modes

The native bridge reuses `native_appearance` (still returning a material string)
and its watcher. `accent:changed` carries an optional finite sRGB triple in
[0,1], replayed on every initial query independently of the material cache.
Only private CSS inputs are injected. Normal-contrast roles preserve the fill,
choose readable paired text for rest, hover and pressed primary fills, and
derive near-black/white accent text and focus cues. Invalid/unavailable input clears the projection to the existing system
colour fallback. Contrast modes retain their system pairs. Gallery theme
simulations never adopt the host's accent; native acceptance uses the picker.

- **Light and dark** come from `color-scheme: light dark` and `light-dark()`;
  components never read `prefers-color-scheme`. The Appearance setting sets
  `color-scheme` on `:root` and the native window appearance together.
- **Increased contrast:** every role collapses to `Canvas` / `CanvasText` /
  `Highlight` pairs, materials go opaque, separators and control edges become
  `CanvasText`, every control gets a 1 px edge, the active row a 3 px edge, no
  shadows, a 2 px `CanvasText` focus ring everywhere. Scarlet becomes
  `CanvasText`: the check, not colour, says "added".
- **Forced colours:** the same, with `ButtonText`/`ButtonFace` on controls,
  `Highlight`/`HighlightText` on selection, `GrayText` on disabled.
  `forced-color-adjust: none` only where a system pair is already used.
- **Reduced transparency:** materials become opaque `--surface-window`;
  overlays lose blur.

### 2.4 Contrast rules

| What | Minimum |
|---|---|
| Body and secondary text, placeholder, footer, key-cap labels | 4.5:1 against rendered pixels on every look and theme |
| Search query at 22 px | 4.5:1 (the normal-text bar, even for large text) |
| Control boundaries that are the only cue (Windows field bottom edge, GNOME selected row, Windows pill) | 3:1 against the adjacent colour |
| Focus ring | 3:1 against both the surface and the control |
| Meaningful icons (status, added check, the citation mark) | 3:1 |
| Tertiary glyphs at rest | 3:1 |
| Disabled | none; aim ≥ 2:1 |
| White on the system accent | the platform's own pair is accepted below 4.5:1 because the user chose the accent (white on macOS dark `#0a84ff` = 3.65:1, on GNOME `#3584e4` = 3.77:1). macOS rows and menus use `selectedContentBackgroundColor` (white 6.35:1 in dark); GNOME row titles stay semibold |
| Over the macOS material | 4.5:1 over a white, a black and a saturated wallpaper, measured natively (#774) |

Measured on browser specimens: secondary text 7.2–7.6:1 on macOS and
5.0–6.5:1 on Windows and GNOME; tertiary glyphs 3.3–5.7:1; scarlet 4.8–6.4:1;
Done is the tightest text role (4.58:1 on Windows light).

### 2.5 Where scarlet is allowed

| Allowed | Never |
|---|---|
| The citation mark in the shelf, the Copied notice and the format preview | Buttons, links, toggles, checkboxes, sliders |
| The added tile behind a white check | Selection, menu highlight, focus ring |
| The logo, app icon, About and first-run brand mark | Errors, Stop status, destructive buttons, badges, counters |
| | Progress, spinners, Settings controls, the shortcut recorder, citation prose |

### 2.6 Colour customization

Follow the system accent, appearance and contrast. Offer Appearance (System,
Light, Dark). Do not offer an accent override (an override would also need a
scarlet clash rule for every hue). Never offer per-element colours, the brand
colour, custom CSS or colour themes.

## 3. Shape

### 3.1 The concentric rule

An inner radius is the outer radius minus the inset between them, never larger,
and never below a per-OS floor:

```
--r-row     = max(--r-min, --r-win  − --inset)       rows, search field, shelf card
--r-tile    = max(--r-min, --r-row  − --row-pad)     type tile, footer buttons
--r-mi      = max(--r-min, --r-menu − --menu-pad)    menu items
--r-control = per OS                                  buttons, pop-ups, fields outside a chain
--r-key     = max(--r-min − 2px, 4px)                 key caps, chip remove buttons
--r-card    = --r-win                                  Settings cards and groups
--r-dialog  = per OS                                  dialogs and sheets
--r-full    = 999px                                    switches, toasts, capsules
```

Liquid Glass's three shape kinds map to `--r-control` (fixed), `--r-full`
(capsule) and the chain (concentric).

### 3.2 Per-OS values (px)

| Token | macOS 15 and earlier | macOS 26 | Windows 11 | GNOME |
|---|---|---|---|---|
| `--r-win` | 12 | 28 (verify against Spotlight) | 8 | 15 |
| `--inset` | 8 | 8 | 8 | 8 |
| `--r-min` | 6 | 6 | 4 | 6 |
| `--r-row` | 6 | 20 | 4 | 7 |
| `--r-tile` | 6 | 8 | 4 | 6 |
| `--r-control` | 6 | 8 in content, capsule in the header | 4 | 6 |
| `--r-menu` / `--menu-pad` / `--r-mi` | 8 / 5 / 4 (verify) | 12 / 6 / 6 | 8 / 4 / 4 | 12 / 6 / 6 |
| `--r-dialog` | 12 | 20 (verify) | 8 | 12 (18 for alert dialogs) |
| `--r-key` | 4 | 5 | 4 | 4 |
| Tooltip | 6 | 8 | 4 | 6 |
| Toast / notice | capsule | capsule | 8 | capsule |
| Settings card | grouped form rows, 12 (verify) | verify | 4 per `SettingsCard`; the picker's list layer card stays 8 | 12 (boxed list) |
| Switch, segmented track | capsule | capsule | capsule | capsule (switch), 6 (linked buttons) |

GNOME follows [libadwaita 1.8's 15 px window](https://github.com/GNOME/libadwaita/blob/1.8.0/doc/css-variables.md#window-radius);
1.6/1.7 used 12 px. The 8 px inset derives 7 px rows; tiles keep the 6 px
floor, and popovers retain their separate 12 px radius. Windows tiles are 4
because Fluent has no 2 px list element.

### 3.3 Strokes

| Token | Value | Use |
|---|---|---|
| `--hairline` | 1 px; 0.5 px on macOS at ≥ 2 dppx | separators, overlay edges, window edge over the material |
| `--border-width` | 1 px | control and field edges |
| `--stroke-focus` | 2 px | focus ring, Windows field focus underline |
| `--contrast-border-width` | 3 px | active row in contrast modes |
| Selection pill (Windows) | 3 × 16 px, radius 1.5 px | |

Never a coloured border on a control. On macOS never a hairline, a shadow and a
fill on the same element: pick a hairline or a shadow.

## 4. Type

### 4.1 Fonts

| | macOS | Windows 11 | GNOME |
|---|---|---|---|
| UI | `-apple-system` | `"Segoe UI Variable Text", "Segoe UI Variable", "Segoe UI"` (Display at ≥ 20 px) | `system-ui` (Adwaita Sans since GNOME 48, Cantarell before) |
| Mono (keys and key formats) | `ui-monospace` (SF Mono) | `"Cascadia Mono", Consolas` | `monospace` (Adwaita Mono since 48) |
| Arabic, Hebrew, CJK | system cascade | `"Segoe UI"`; `"Yu Gothic UI"`, `"Microsoft YaHei UI"` | fontconfig (Noto) |

Each platform gets its own stack; Linux never picks up an installed Segoe UI.

### 4.2 Scale (rem; px at 100 %)

| Role | Token | Size / line | Weight | Notes |
|---|---|---|---|---|
| Search (macOS) | `--type-search` | 22 / 28 | 400, tracking −0.025 em (Latin only) | Spotlight's query |
| Search (Windows, GNOME) | | 16 / 22 | 400 | |
| Large title (Windows) | `--type-title-large` | 28 / 36 | 600 | Windows Settings page titles |
| Heading | `--type-heading` | 20 / 28 | 600 | dialog titles; macOS alerts use 15/20 |
| Title | `--type-title` | 15 / 20 | 600, −0.01 em | row titles, Settings group headings |
| Body | `--type-body` | 14 / 20 | 400 | |
| Meta | `--type-meta` | 13 / 18 | 400 | menus, descriptions, footer, the shelf |
| Label | `--type-label` | 12 / 16 | 600 (headings) or 400 (hints) | nothing smaller |
| Citation, key formats | `--type-citation` | 13 mono | 400 (500 in the Copied notice) | formatted styles use 13/18 in the UI font |

Weights 400, 500 (key caps and chip labels) and 600. No 700, no italics except
"why it matched", no `text-transform: uppercase`.

### 4.3 Numerals and keys

- `tabular-nums` on years, counts, page numbers, times and the recorder.
- Key caps use the UI font: monospace fonts draw ⌘ ⌥ ⇧ ⌃ ↵ ⌫ poorly.
- Citation keys (`@garcia2021`) and key-format output use monospace.

### 4.4 Text size

- Type and vertical sizes are rem; radii and hairlines stay px.
- Ibis's Text size multiplies the root size (today Larger = 1.15). Planned:
  Default 1, Large 1.15, Larger 1.3, which changes `larger_text` in ADR 0020
  (#775).
- System scaling is followed and multiplied with the setting, capped at 2.0:
  Windows `UISettings.TextScaleFactor` (WebView2 does not apply it), GNOME
  `text-scaling-factor` (WebKitGTK already applies it; do not double it).
- With larger text the window grows by the same factor, up to the work area.

## 5. Space and size

### 5.1 Grid

| Token | px | Use |
|---|---|---|
| `--space-1` | 4 | icon to label |
| `--space-2` | 8 | inset, chip padding, menu offset |
| `--space-3` | 12 | row padding, row gap |
| `--space-4` | 16 | dialog padding, Settings card padding |
| `--space-5` | 20 | Settings gutter at compact width |
| `--space-6` | 24 | between Settings groups, empty-state blocks |
| `--space-8` | 32 | Settings page side padding |
| `--space-10` | 40 | empty-state top padding |

2 px only for optical nudges, written `calc(var(--space-1) / 2)`.

### 5.2 Density

| | Compact | Comfortable (default) | Spacious |
|---|---|---|---|
| Result row | 40 | 58 | 76 |
| Settings row | 40 | 48 (56 with description) | 56 (64) |
| Menu item | 28 mac / 32 Windows, GNOME | same | same |

Density changes rows only; controls, targets, type and the header never change.

### 5.3 Hit targets

| | Visual height | Target |
|---|---|---|
| macOS | 28 in the header, 32 in content | 32 (`::before` expands) |
| Windows 11 | 32 | 32 |
| GNOME | 32 | 32 |
| Any, `pointer: coarse` | +8 | 44 |

Adjacent targets never overlap; the 4 px gap between header controls belongs to
the 32 px target.

The header's nominal control bands are 52 / 48 / 50 px (macOS / Windows /
GNOME): 36 / 32 / 34 px controls plus two 8 px gaps. The always-visible 1 px
divider is outside that band, making the total 53 / 49 / 51 px. The footer has
24 px fills and two 8 px gaps (40 px); Windows and GNOME add an outside 1 px
top divider. Coarse 44 px targets and wrapped controls grow these bands.
The list's clipping viewport keeps 8 px from adjacent bands even while scrolled.

### 5.4 Icons

| Size | Use | Stroke (Ibis set) |
|---|---|---|
| 12 | chevrons inside controls | 1.5 |
| 16 | menu items, chips, inline status, clear glyph, citation mark | 1.5 mac and GNOME, 1 Windows |
| 20 | header icons, type tiles | mac 1.6, Windows 1, GNOME 1.5 |
| 32, 48 | empty states, first run | 1.5 at 32, 2 at 48 |

Ibis draws its own set; platform icons only where the platform owns the meaning.

## 6. Elevation and materials

### 6.1 Window materials

| Window | macOS | Windows 11 | GNOME |
|---|---|---|---|
| Picker | `NSVisualEffectView` popover material + 55 % veil; macOS 26: Liquid Glass on the search capsule only | Desktop Acrylic | flat `window_bg_color` |
| Settings | standard window background, toolbar of icon tabs, grouped form rows | Mica, cards on `LayerFillColorDefault` | flat, boxed-list cards |
| Copied notice | HUD-style pill | notification card | dark toast |
| Menus, popovers | in-window, `--surface-overlay` with blur | in-window, opaque | opaque `popover_bg_color` |

Never glass on glass, never CSS blur pretending to be the desktop. All materials
turn opaque under reduced transparency, increased contrast, forced colours and
battery saver.

### 6.2 Layers

| Level | What | macOS | Windows 11 | GNOME |
|---|---|---|---|---|
| 0 Window | `--surface-window` | material | Acrylic / Mica | `window_bg` |
| 1 Content | `--surface` | sheet | list as a layer card: 1 px `--stroke-control`, `--shadow-card` | flat |
| 2 Raised | `--surface-raised` + `--shadow-raised` | band with 1 px inner top highlight | flat on Mica | flat with separator |
| 3 Overlay | `--surface-overlay` + `--shadow-overlay` + `--stroke-overlay` | | | |
| 4 Modal | dialog + `--scrim` + `--shadow-dialog` | | | |
| 5 Toast | own window (Copied) or `--shadow-toast` | | | |

### 6.3 Shadows

| Token | macOS | Windows 11 | GNOME |
|---|---|---|---|
| `--shadow-card` | `0 0 0 .5px rgb(0 0 0/10%), 0 1px 2px rgb(0 0 0/8%), 0 8px 20px -8px rgb(0 0 0/18%)` | `0 0 2px rgb(0 0 0/12%), 0 2px 4px rgb(0 0 0/14%)` | `0 0 0 1px rgb(0 0 6/3%), 0 1px 3px 1px rgb(0 0 6/7%), 0 2px 6px 2px rgb(0 0 6/3%)` |
| `--shadow-raised` | `0 1px 0 var(--separator), 0 8px 16px -12px rgb(0 0 0/22%)` | none | none |
| `--shadow-overlay` | `0 12px 32px rgb(0 0 0/16%)` + hairline | `0 0 2px rgb(0 0 0/12%), 0 8px 16px rgb(0 0 0/14%)` | `0 1px 5px 1px rgb(0 0 0/9%), 0 2px 14px 3px rgb(0 0 0/5%)` |
| `--shadow-dialog` | `0 24px 48px rgb(0 0 0/22%)` | `0 0 8px rgb(0 0 0/12%), 0 32px 64px rgb(0 0 0/14%)` | `0 4px 24px rgb(0 0 0/18%)` |
| `--shadow-toast` | `0 8px 16px rgb(0 0 0/18%)` | `0 4px 16px rgb(0 0 0/22%)` | none (dark toast) |
| `--shadow-row-lift` | selected row only: `inset 0 1px 0 rgb(255 255 255/22%), 0 1px 2px rgb(0 0 0/14%), 0 4px 10px -4px rgb(0 0 0/24%)` | none | none |

Windows dark values use Ibis's 1.7 × alpha adaptation. This is distinct from
[Fluent UI's upstream 2 × neutral shadow ramp](https://github.com/microsoft/fluentui/blob/master/packages/tokens/src/alias/darkColor.ts);
the toast keeps its compact footprint because its fixed 420 × 112 native window
reserves 16 px around the card. A generic shadow28 produces a visible clipped
edge in that viewport. No shadow on rows at rest, chips, key
caps on Windows and GNOME, anything in contrast modes, anything on Linux except
cards, overlays and dialogs, or anything inside an overlay. In dark mode, fills
and hairlines separate layers first.

## 7. Motion

| Token | Value | Use |
|---|---|---|
| `--motion-feedback` | 120 ms | hover and press fills, switch state, tooltip fade |
| `--motion-panel` | 200 ms | menus, popovers, dialogs, toasts in |
| `--motion-layout` | 280 ms (cap) | pane open, window grow, shelf appear |
| `--motion-exit` | 100 ms | everything out |
| `--ease-out` | mac `cubic-bezier(.2,.8,.2,1)`; Windows `cubic-bezier(0,0,0,1)`; GNOME `cubic-bezier(.33,1,.68,1)` | entrances, state changes |
| `--ease-in` | mac and GNOME `cubic-bezier(.4,0,1,1)`; Windows `cubic-bezier(1,0,1,1)` | exits |
| `--tooltip-delay` | 600 ms, then 0 ms for 300 ms after one closes | |
| `--busy-delay` | 400 ms before any spinner shows | |

| | macOS | Windows 11 | GNOME |
|---|---|---|---|
| Menu open | at once; fade out 100 ms after a pick | slide 8 px + fade, 200 ms | at once |
| Dialog | fade + 0.98 → 1, 200 ms | fade + 1.05 → 1, 200 ms | fade 200 ms |
| Selection | instant | the pill slides between rows, 167 ms | instant |
| Toast | slide 8 px + fade | slide up + fade | slide up + fade |
| Chevron | never rotates | never rotates | never rotates |

Reduced motion: no translate, scale or slide; fades of at most 100 ms; spinners
turn at half speed. **Never animates:** list content while typing, row height,
the caret, layout on hover, the citation text, ticking counts, focus rings, the
window on first show (only a 120 ms fade), anything over 280 ms, anything that
loops except a busy indicator.

## 8. States

| State | How it shows | Notes |
|---|---|---|
| Rest | role fill or none | |
| Hover | `--fill-*-hover`, only under `@media (hover: hover)` | never moves or resizes; never on touch |
| Pressed | `--fill-*-pressed`; darker on macOS and GNOME, lighter on Windows | no scale on Windows and GNOME |
| Selected | fill (D7); checked items get a leading check | never an outline |
| Focused (keyboard) | ring (D8) | never after a click |
| Disabled | `--text-disabled`, `--fill-disabled`, no hover, `aria-disabled` | menu items stay focusable (ADR 0021) |
| Busy | the leading glyph becomes a 16 px spinner after `--busy-delay`; `aria-busy` | never a full-window blocker |
| Invalid (fields) | `--status-stop` 2 px bottom edge (Windows) or text below (all), icon + words | never a red ring |
| Active row, window inactive | `--row-inactive-background` | kept-open picker without focus |

| Primitive | Rest | Hover | Pressed | Selected / on | Focus-visible | Disabled |
|---|---|---|---|---|---|---|
| Button, primary | `--accent` + `--text-on-accent` | accent 92 % mixed to text | accent 84 % | | ring | `--fill-disabled`, `--text-disabled` |
| Button, secondary | `--fill-control` (+ Windows stroke) | `-hover` | `-pressed` | `aria-pressed`: `--fill-control-pressed` | ring | as above |
| Button, quiet | none, `--muted` | `--fill-subtle-hover`, `--text` | `--fill-subtle-pressed` | accent glyph | ring | `--text-disabled` |
| Icon button | as quiet, 32 × 32 | | | filled glyph on `--fill-control-pressed`, neutral `--text` | ring | `--text-disabled`, `--fill-disabled` |
| Pop-up | quiet in the header, secondary in Settings | | open: pressed fill | | ring | |
| Menu item | none | `--menu-highlight` (pointer and arrows alike) | same | leading ✓ | the highlight is the focus | `--text-disabled`, reachable |
| Segmented control | track `--fill-control` | segment `--fill-subtle-hover` | | thumb `--surface-raised` + `--shadow-card` (mac, Windows), `--fill-control-pressed` (GNOME) | ring on the group | |
| Switch | track `--fill-control-pressed`, white knob | track darker | knob stretches 2 px (Windows) | track `--accent` | ring | 50 % |
| Checkbox | 16 px box, `--stroke-field` | edge darker | | `--accent` box, white check; mixed: dash | ring | |
| Text field | `--surface-field` | edge darker (Windows) | | | Windows accent underline; ring elsewhere; search field none | |
| List row | none | `--fill-subtle-hover` | | `--row-active-*` | the fill is the focus | `--text-disabled` |
| Chip | `--fill-control` | remove button hover | | | ring on the remove button | |
| Key cap | `--fill-control` | | flashes pressed while held (recorder only) | | | |

## 9. Primitives

Each `src/ui/` component wraps one React Aria component (ADR 0021), owns one CSS
file that reads role tokens only, and has a test per platform look and a gallery
state.

| Primitive | Anatomy | Sizes | Keyboard | macOS | Windows 11 | GNOME | React Aria |
|---|---|---|---|---|---|---|---|
| Button | [icon 16] label; padding 12; one primary per view | 32 (28 mac header), 24 inside chips | Space, Enter | push, `.borderedProminent` | `Button`, `AccentButton` | `button`, `.suggested-action`, `.flat` | `Button` |
| Icon button | glyph 20 in a 32 square; tooltip names action + key | 32 | Space, Enter | toolbar item | subtle button | `.flat` image button | `Button` + `Tooltip` |
| Toggle button | icon button with `aria-pressed`, filled glyph when on | 32 | Space | toolbar toggle | `ToggleButton` | toggle `.flat` | `ToggleButton` |
| Pop-up | value + 12 px chevron; menu with checks | 32 (28 mac header) | Space, Enter, ↑↓; type-select | `NSPopUpButton` | `ComboBox` / `DropDownButton` | `GtkDropDown` | `Select` |
| Menu | sections with headings; [✓ 18] label … [key] | items 28 mac, 32 Windows and GNOME; width 220–320 | ↑↓, Home/End, type-select, Enter, Esc, → submenu | `NSMenu` | `MenuFlyout` | `GtkPopoverMenu` | `Menu`, `SubmenuTrigger` |
| Segmented control | track + 2–5 equal segments | 28 mac, 32 Windows, 34 GNOME | ←→ within, Tab out | `NSSegmentedControl` | segmented | `AdwToggleGroup` | `ToggleButtonGroup` |
| Switch | track 38 × 22 mac, 40 × 20 Windows, 46 × 26 GNOME | | Space | `NSSwitch` | `ToggleSwitch` | `GtkSwitch` | `Switch` |
| Checkbox | 16 box + label, mixed state | 16 (20 GNOME) | Space | checkbox | `CheckBox` | `GtkCheckButton` | `Checkbox` |
| Text / search field | [icon] text [clear]; label above in Settings | 32 (search 40) | standard editing; Esc clears search | `NSTextField`, `NSSearchField` | `TextBox` | `GtkEntry`, `GtkSearchEntry` | `TextField`, `SearchField` |
| List row | [tile 32] title / meta (monospace key and library shorten first) [year only, vertically centred] | 40 / 58 / 76 | via the listbox | table row | `ListView` item | `GtkListBox` row | result list stays a combobox (#485) |
| Chip | [label] [× 16 in 24]; fill, no outline | 24 | ←→ moves; Backspace, Delete remove | token | `TokenizingTextBox` token | pill tag | `TagGroup` |
| Key cap | UI-font label, 20 high, min 20 wide, 4 inline pad | 20 (16 in menus) | | menu key text | PowerToys caps | `GtkShortcutLabel` | plain `kbd` |
| Tooltip | one line, ≤ 40 ch, label + key cap | 24–28 | shows on focus; Esc hides | native tooltip | `ToolTip` | `GtkTooltip` | `Tooltip` |
| Toast / banner | toast: [icon] text [Undo]; banner: full width above the list | toast 44 capsule; banner ≥ 40 | Ctrl/⌘ Z undoes; F6 reaches it | HUD | `InfoBar`, toast | `AdwToast`, `AdwBanner` | `role="status"` until RA Toast is stable |
| Inline message | [status icon 16] sentence under its control | | | inline text | inline `InfoBar` | `.error` label | |
| Progress | bar 4 high capsule; spinner 16 / 32 | | | `NSProgressIndicator` | `ProgressBar`, `ProgressRing` | `GtkProgressBar`, `AdwSpinner` | `ProgressBar` |
| Empty state | icon 48 tertiary, title 15/20, one line 13/18, up to 2 actions | max 40 ch | first action Enter-able | `ContentUnavailableView` | empty-state pattern | `AdwStatusPage` | |
| Dialog | title 20/28 (mac 15/20), body, end-aligned buttons (mac primary right-most, Windows primary left of the pair, GNOME stacked when narrow) | 320–480; full window below 420 px | Esc cancels, Enter default, focus trapped | sheet / `NSAlert` | `ContentDialog` | `AdwAlertDialog` | `Dialog`, `Modal` |
| Popover | arrowless panel, 8 px offset | max window − 16 | Esc; focus moves in | `NSPopover` | `Flyout` | `GtkPopover` | `Popover` |
| Settings row / group | see D17 and [settings.md](settings.md) | 48 / 56 | Tab through controls | grouped Form rows | `SettingsCard`, `SettingsExpander` | `AdwActionRow`, `AdwPreferencesGroup` | composition |
| Shortcut recorder | see [settings.md](settings.md) §2.1 | 32 | Enter or click records; Esc cancels; Backspace clears | inline field | PowerToys dialog | "Set Shortcut" dialog | composition on `Button` |
| Tabs | text tabs: underline (Windows), pill (mac 26), plain (GNOME) | 32 | ←→ | segmented / tab view | `TabView`, `SelectorBar` | `AdwViewSwitcher` | `Tabs` |

## 10. Overlays and layering

| Layer | Token | Examples | Dismissal | Focus |
|---|---|---|---|---|
| Content | `--z-content: 0` | list, shelf | | |
| Sticky chrome | `--z-chrome: 1` | header, footer, the macOS glass edge | | |
| Popover / menu | `--z-popover: 10` | format, scope, More, filter suggestions, the "+N" list | Esc, outside click (consumed), Tab, picking | moves in; returns to the trigger or the search box |
| Tooltip | `--z-tooltip: 20` | | pointer out, Esc, focus out, any key | never takes focus |
| Modal | `--z-modal: 30` | keys dialog | Esc, Close; scrim click closes non-destructive dialogs only | trapped, returns to the invoker |
| In-window toast | `--z-toast: 40` | "Removed · Undo" | 8 s (paused on hover and focus), Esc when focused | never steals focus; F6 reaches it |
| Separate windows | OS | Copied notice, Settings | | the Copied window never takes focus |

- One overlay of each kind at a time; a menu closes a tooltip; a modal closes
  any menu.
- **Escape ladder:** close the top overlay, then the pane, then clear query and
  filters, then hide. Escape never turns off Keep Window Open and never closes
  Settings.
- **Positioning:** React Aria placement with `containerPadding` 8, flip then
  shift, offset 6, max height window − 16. Header menus prefer `bottom end`,
  footer menus `top end`.
- **Small windows** (< 420 × 360): popovers take the window width − 16; dialogs
  become full-window sheets with a back button; tooltips truncate to one line.
- Overlays close when the picker hides and do not reopen.
- Native menus remain for the menu bar, tray and right-click (ADR 0021).

## 11. Windows and adaptivity

### 11.1 Picker sizes

| | Value |
|---|---|
| Default | 720 × 480 |
| Minimum | 360 × 320 |
| Maximum | min(1280, 90 % work area) × min(900, 90 %) |
| With the pane | the list keeps its width; the window grows toward the side with room, up to 1200, then the pane replaces the list |
| Larger text | default size × the text factor |

### 11.2 Width classes (container queries on `main`, in `em`)

| Class | Width at 100 % text | Layout |
|---|---|---|
| XS | < 28.75 em (460 px) | one scroller; header controls wrap under the field; footer hints show keys only |
| S | 28.75–40 em | list only; the pane replaces the list with a back affordance |
| M | 40–60 em | list + pane side by side when opened (list ≥ 22 em) |
| L | ≥ 60 em (960 px) | list max 45 em; the pane takes the rest (≤ 72 ch); the shelf's content moves into the pane ([preview.md](preview.md)) |

Below 22 em (352 px) high, the shelf and pane cap at 6 rem and the list joins
one scroller.

### 11.3 Placement and displays

- Opens on the display with the focused window, at the last position on that
  display; otherwise centred one fifth from the top.
- Sizes are logical; moving between displays keeps the logical size.
- Geometry in whole logical px; at Windows 125 % and 150 %, draw strokes as
  `box-shadow: inset`; icons are SVG at integer sizes.

### 11.4 Platform window behaviour

| | macOS | Windows 11 | GNOME (Wayland / X11) |
|---|---|---|---|
| Picker | non-activating floating panel, all Spaces, over full-screen apps | tool window, no taskbar or Alt+Tab, light dismiss | Wayland: compositor-centred, kept open means stays open; X11: positioned, on top when kept open |
| Snap / tiling | none | not snappable; Win+arrows ignored | not tiled |
| Settings window | normal window, Dock while open, ⌘, opens it, not full-screenable | normal window, taskbar while open, Snap layouts | normal window, adaptive down to 360 px |
| Settings size | 680 wide, toolbar of icon tabs, height per pane, title in the title bar | 1000 × 700, min 640 × 500; `NavigationView` collapses below 641 px | 640 × 600, min 360 × 294 |
| Tray / menu bar | `NSStatusItem` + native menu | notification-area icon + native menu | StatusNotifierItem needs an extension on stock GNOME, so the app launcher must always reach the picker and Settings |

### 11.5 Long strings

- Budget +35 % for German and Greek, +50 % for labels of 10 characters or fewer.
- Labels on actions and settings wrap to two lines rather than truncate. Data
  (titles, names, paths) truncates with an ellipsis and a tooltip.
- Buttons size to content, min width 64 (Windows dialog buttons 120).
- Settings rows reflow below 40 em: the control moves under the label.

### 11.6 Right-to-left and scripts

- Logical properties only; the pill, tile, year and chevron mirror. Search,
  clock and media glyphs do not; back and forward arrows do.
- Key caps keep platform order (⌘⇧K) in RTL.
- `:lang(ar, he, fa, ur, zh, ja, ko)` gets `letter-spacing: 0`.
- Mixed-direction strings are isolated (`unicode-bidi: isolate`). The citation
  output is `dir="auto"`, so a right-to-left formatted citation reads correctly;
  citation keys stay LTR.
- No uppercase transforms; Hebrew and Arabic get +2 px line height at 13 px.

## 12. Interaction patterns

**Keyboard (picker).** Focus stays in the search box. ↑ ↓ move the active row
(`aria-activedescendant`), Page Up/Down by a page, Ctrl/⌘ ↑ ↓ to first and last.
Enter copies, Shift+Enter adds to the citation, → opens the pane, Ctrl/⌘ K opens
More, Ctrl/⌘ / the keys list, Ctrl/⌘ , Settings, Ctrl/⌘ I toggles what will be
pasted, Ctrl/⌘ Shift P toggles Keep Window Open, Esc steps back. Labels come
from `keys.ts` only.

**Mouse, touch and voice parity** (#788). Every task works with a mouse or
trackpad, a touchscreen or pen, or voice control and switch access alone:

- Opening without a shortcut works: the menu bar or tray icon and the app icon
  open the picker, and first run says so.
- Every keyboard action has a visible control: clicking a row adds it, with a
  clear add/remove target; footer hints ("Copy as … ↵", "Shortcuts", "More")
  are real buttons; chips have × buttons; right-clicking a row opens a context
  menu (Copy, Add, Open PDF, Show in Zotero).
- Hover is never the only way to see something. Tooltips repeat visible or
  accessible text; kept-open and selected states are visible without hover.
- Targets follow D12 (32 px, 44 px under `pointer: coarse`) without changing
  the keyboard layout.
- An outside click must not cost a mouse user their selection: either keep it
  until the next open (with Undo) or give a clear path to Keep Window Open. The
  choice is made in #788's plan.
- Dragging a row or the shelf into another app drops the citation text.
- Voice Control and Voice Access can name every control: visible labels match
  accessible names.
- Gallery states and a test cover pointer-only and coarse-pointer layouts.

**Type-ahead.** Typing anywhere in the picker types into the search box. Menus
select by first letters; menus with more than 7 items get a filter field. In
Settings, ⌘F / Ctrl+F focuses settings search.

**Multi-select.** The citation is the multi-select: Shift+Enter or ⌘/Ctrl-click
adds, in order; no Shift-click ranges. Among chips, ← → move, Backspace and
Delete remove, Alt+← → reorders.

**Drag and drop.** Dragging a row out drops the citation text (`text/plain`).
Dropping a `.bib` file on the picker or first run adds it as a library. Chips
reorder by drag. The list never starts a window drag.

**Undo.** Ctrl/⌘ Z undoes the last citation change and the last library
removal; the toast offers the same. Undo history lives in memory.

**Confirmations.** None in the picker. In Settings, Reset applies at once with
Undo; Clear history applies with an 8 s Undo before data is deleted.

**Errors.** Say what happened and what to do, in one sentence, next to the
cause; never a modal. Library errors live in the footer (Look / Stop). Copy is
the fallback whenever insertion fails, and the notice says why.

**Loading.** Keep the last results while reloading; the footer says
"reloading…". Spinners only after 400 ms. First load shows "Opening library…"
with a spinner, never skeleton rows.

**Offline.** "Offline" means Zotero is unreachable: keep the last copy
searchable, say "not updated · checked 2 h ago" (Stop dot), retry quietly.

## 13. New and experienced users

| Layer | What they see | How it retires |
|---|---|---|
| First minute | one placeholder ("Search title, author, year…"), the Enter hint, the shelf showing what Enter pastes | the Enter hint stays while a row is active |
| First week | one footer tip at a time, tied to what they just did (after 3 copies: "Shift+Enter adds several"; after an outside-click hide: "Keep Window Open (⌘⇧P) keeps Ibis open") | retires after the action is done twice or the tip is closed; a local "seen" flag (needs an ADR first) |
| Power | More lists every action with its key; `?` lists every key; filters suggested when typed; density; custom shortcut; ⌘I hides the output line | always reachable, never shown unasked |

Tooltips name the action and its key ("More · ⌘K"). No tour, coach marks,
badges or "what's new" pop-up.

## 14. Customizability

| A setting (Settings window) | Automatic | Never offered |
|---|---|---|
| Global shortcut (recorder) | Light / dark when Appearance = System | Accent colour, brand colour |
| Appearance: System, Light, Dark | Accent, contrast, reduced motion and transparency | Fonts, font size in px, radius, spacing |
| Density: Compact, Comfortable, Spacious | System text scaling | Custom CSS, themes |
| Text size: Default, Large, Larger (ADR change) | Window material per OS | Animation speed |
| Citation format and style; show what will be pasted | Display and position per display | Toolbar layout editing |
| Start at login | Language | Per-element colours |
| Keep last search for (1–3 min) | Copy versus insert per app capability | "Advanced mode" |
| Libraries; footer tips; clear history | | |

## Not decided here

Reserved for the maintainer, each with a recommendation: an accent override (no);
using Fluent or Adwaita icon sets on their platforms, a licensing question (keep
drawing our own); a soft shadow under the macOS selected row, against "selection
is a fill" (allow on macOS only); text size in three steps (yes, with an ADR,
#775).

Values marked "verify" have not been checked on a live system; #774 measures
them.
