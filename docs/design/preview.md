# Citation preview: the shelf

This page specifies how the picker shows what will be pasted. The maintainer
chose option B, the **citation shelf**, on 2026-10-21 (#787). It replaces the
tray of raw keys and the scarlet preview stacked under the search field. It
uses the tokens in [system.md](system.md) and sits above the footer described
in [design.md](../design.md).

Prototype: branch `design/preview-prototype` (gallery `?preview=b`, gated to
`:root[data-preview]`, so the app is unchanged). It builds on the soft depth
layer from `design/depth-prototype` (variant B). The footer pattern
"Copy as Pandoc ↵" comes from `design/chrome-polish-prototype`.

## Decisions

- **Placement follows meaning.** The top of the window is what you ask for
  (search), the middle what you browse (results), the bottom what you get: the
  shelf, then the footer's Enter action that pastes it. Results never move when
  a reference is added.
- **One quiet card, up to two lines, directly above the footer's "Copy … ↵".**
  - Chips: the chosen references (only after Shift+Enter) as author-year chips,
    "García 2021 ×".
  - Output: the exact text Enter pastes, on one line with an ellipsis; the full
    text is in the tooltip and the wide pane.
- **The citation uses the normal text colour with a small scarlet accent.** One
  16 px scarlet quote mark in the tile column is the single "this is your
  citation" sign. Output text, chips and counts use text colours. Rows keep
  their scarlet added tile.
- **Monospace for key formats only.** Formatted styles use the system font:
  `(García & Okafor, 2021; Lindqvist et al., 2019)`. Key formats use monospace,
  because the syntax is the point: `[@garcia2021; @lindqvist2019]`, with
  punctuation in `--muted` and keys in `--text`.
- **Direction:** the output is a `<bdi dir="auto">`, so an Arabic- or
  Hebrew-first formatted citation reads correctly. Keys stay LTR.
- **Wide windows get a detail pane.** At 960 px (60 em, the L width class) or
  wider, the shelf's content moves into a pane on the right: the focused
  reference (type, title, authors, year, container, key), "In this citation · N"
  with its chips, and "Pastes as APA" with the full output, wrapped. The list
  keeps full-width rows. The attachment preview later opens into the same pane,
  with the output block pinned at its bottom.
- **It can be hidden, and is on by default.** One setting, "Show what will be
  pasted" (Settings › Citations, see [settings.md](settings.md); in More until
  Settings exists), plus ⌘I / Ctrl+I in the picker. Both are saved.
  - On by default: the output is the last check before an action that is hard to
    undo once it lands in a document, and it teaches what each format produces.
    It costs 40 px.
  - When off, the chips stay (they are the person's work in progress) and only
    the output line hides. The footer still says "Copy 3 as APA ↵".
  - When off in a wide window, the pane closes and the list takes the full
    width.

### Chips before the count

- As many chips as fit on one line, at most 6; the rest fold into "+N". In the
  720 px window that is usually 4 to 6. The shelf never grows a second chip
  line.
- If two chips would read the same, add the second author ("García & Okafor
  2021"), then the key.
- "+N" opens a popover with every chosen reference, remove buttons and
  reordering (Alt+↑↓, drag), with the chip keyboard model (← → between chips,
  Backspace and Delete remove).

## Anatomy and states

| Part | Spec |
|---|---|
| Card | `--surface-raised`, inset `--inset`, radius `--r-row`, card shadow. Windows: layer card with stroke. GNOME: no card, a flat band under one separator. The footer drops its top rule under the card. |
| Chip row | 40 px. Chips are a filled `--fill-control` with no outline, label 13/18 at 500, a 16 px × in a 24 px chip with a 32 px hit area. Capsule on macOS and GNOME; 4 px radius with Fluent stroke on Windows. |
| Output row | 40 px. The quote mark is centred over the tile column; text starts at the row-title edge (64 px; 60 on Windows), so search glyph, tiles and quote mark share one column. 13/18. |
| Pending | Under 300 ms the previous text stays, dimmed. After that, "Formatting…" in secondary text with the mark at 45 %. Enter waits. |
| Failed | A Stop icon replaces the mark, then "Couldn't format in APA. Choose another style, or copy keys." The Enter hint disappears. |
| Stale | A Look icon, then "The library changed. Reading it again…". |
| Warnings, provisional (IEEE) | A trailing secondary pill, "1 warning" or "Numbers may change"; detail in its tooltip and in the pane. |
| Copied while kept open | The mark becomes a Done check and the line reads "Copied" for 1.2 s. Otherwise the window hides and the Copied notice shows. |
| Selection | Selectable in the pane, where it wraps. In the shelf it is one read-only line, so selection never fights row clicks. |
| Larger text, RTL | Chips and lines scale; fewer chips fit before "+N". In RTL the mark sits at the start (right) and chips flow from the right. |
| Small window (under 460 px) | The chip row shows only "3 references ›", which opens the list; the output line stays. |
| Increased contrast, forced colours | Card and chips get a 1 px `CanvasText` edge and the mark turns `CanvasText`. Words carry every state. |

When large system text and wrapped header/footer controls consume the available
height, the shelf shrinks into its own scroll area. Keep at least 24 CSS px of
the results scroller reachable; chips, overflow and output remain reachable by
scrolling the shelf.

### Per OS

| | macOS | Windows 11 | GNOME |
|---|---|---|---|
| Shelf | raised card, top highlight, soft shadow | layer card on Mica, 1 px stroke, 8 px radius | flat band, one separator |
| Chips | capsule, like `NSTokenField` tokens | 4 px radius, subtle fill with control stroke (`TokenizingTextBox`) | pill, button fill at 10 % |
| Fonts | SF Pro 13; SF Mono for keys | Segoe UI Variable; Cascadia Mono | Adwaita Sans; Adwaita Mono |
| Pane at ≥ 960 px | on the content sheet | transparent over Mica, the list as a card | flat, separator |

## Variant to try: the beak quote mark

From #787: draw the quote mark as two curved ibis beaks, so it doubles as a
small brand glyph. Try it as an SVG at 12, 16 and 20 px on every platform, light
and dark. Keep it only if it still reads as a quotation mark at 12 px;
otherwise use a plain quote mark and keep the beak glyph for larger brand uses
(app icon details, About).

## Options not chosen

- **Before:** raw keys plus large scarlet preview under the search; pushed
  results down 100–190 px and showed nothing for key formats. Replaced.
- **A, detail pane at every size:** titles wrap and keys truncate at 720 px. Used
  only at 960 px and wider.
- **C, chips in the search field with output in the footer:** crowds the field,
  overloads Backspace and hides library status. Not chosen.
- **Output inside the Enter hint or a tooltip:** the footer wraps; a tooltip is
  invisible by default. Rejected.
- **On demand only:** hides the check from new users. Kept as the setting's off
  mode.

## Engineering notes

- **Chip keyboard foundation:** `ui/ChipGroup` owns navigation and removal,
  with controlled opaque identities/order, locale-matched direction and a
  caller-supplied empty-removal focus destination. The browser-only `chips`
  and `chips-rtl` gallery states exercise it separately; production shelf
  fitting, disclosure and reordering integrate in later leaves.
- **Key formats need a pure preview.** `copyCitation` formats keys in Rust and
  writes the clipboard in one step. Add a side-effect-free command (for example
  `citation_text(ids, format, revision)`) or reuse the Rust formatter without
  copying; check whether ADR 0016 needs a note.
- **Keys:** ⌘I / Ctrl+I needs a `KEY_TABLE` row; the saved toggle is a
  picker-preferences field under ADR 0020.
- **Gallery:** `seeded()` and `selectionContained()` assume today's band order
  and must check "shelf above footer, results not moved"; the prototype's
  `out-*` states (one, many, seven, error, larger, RTL, 50 keys) should join the
  gallery tests.
- **Order of work:** the pure key preview command; `ui/Chip` and the shelf with
  the states above, replacing `SelectionTray` and `FormatPreview`; the setting
  and key; the wide pane with the attachment preview.
- Every colour is an existing token. Text roles on the soft depth surfaces
  already pass; the scarlet mark is a non-text mark that needs 3:1.
