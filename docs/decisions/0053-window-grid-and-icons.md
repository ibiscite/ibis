# 0053 One window grid, and icons by meaning

2026-11-13, proposed for the maintainer's review (design/core-spec; pictures in
`.local/design-core/`). Supersedes [0044](0044-lucide-icon-primitive.md).

The maintainer saw a stray clear ×, a held ⇧⌘F standing in for the format
button, a gap before "Best matches", a small bird, a green dot before the
library, a reset button that left empty room at every row's end, and weak
icons. Each came from a rule, not from one screen, so the rules change:

1. **One grid** (`tokens.css`, `ui/Band`): a leading edge (`--row-start`), a
   32 px lead column (`--tile`), a title column (`--label-start`) and a
   trailing edge. A band has a lead column only when something can stand in it;
   headings, notices and status have none and start on the leading edge, where
   a tile's plate starts. Every glyph in the lead column is 20 px. The last ink
   of a band ends on the trailing edge (`--cap-end` for a key cap's fill).
2. **The bird** fills the lead column (`--mark-search`), so its ink matches the
   query's line height.
3. **No clear button.** Esc clears, as in Spotlight and Raycast; every search
   field is the same frameless field.
4. **Held ⌘ marks rows only**, with the digit alone. Controls never swap their
   words or glyph for a key; their keys are already printed beside them.
5. **Status only when it needs you.** A healthy library is its name in the
   footer; a dot appears only with look or stop words.
6. **Use Default follows the setting's name**, so nothing is reserved at a
   row's end and every control shares the card's trailing edge.
7. **Icons by meaning, from Phosphor Regular** (MIT, `@phosphor-icons/core`
   2.1.1), vendored unchanged in `brand/icons/` with `SOURCES.json` naming the
   upstream glyph and `LICENSE-phosphor`. One meaning, one glyph. Lucide's set
   was a 36-glyph subset hand-normalized to one path each, so adding a meaning
   cost a hand drawing and glyphs were borrowed for meanings they do not carry;
   and its 1.5 px stroke at 16 px was heavier than the text beside it.
   Phosphor's 256-unit grid lands its 16-unit stroke on whole pixels at 16 px
   and reads closest to SF Symbols; Tabler and an optically retuned Lucide were
   close seconds, Heroicons lacked a quote, pin, keyboard and contrast glyph and
   has outlines only at 24 px.
