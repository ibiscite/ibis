# 0042 The citation line is a token field

2026-11-10 (#1762). The maintainer chose this from three options
(`.local/design-review/shelf-inspector/options.md`):
- segments plus a side Inspector;
- chips in the search line with a popover;
- the citation line as a token field.

## Decision

- **One line.** The shelf's one line of real output is where a citation is
  built. Each reference in it is a **token**: its own words from the output,
  in a quiet capsule. The brackets and separators between tokens stay plain,
  quieter text. Search stays plain.
- **Details in a popover.** A token's details are a popover that grows upward
  from the token, never over the row being chosen. It holds the locator (type
  and value), prefix, suffix and form, and the actions with their keys. Nothing
  there outlives the paste.
- **[C19] narrowed.** It applies to the Reader and the warnings, which keep the
  side panel. One reference's citation details are no longer a panel.

## Solving the token field's weak points

- **Numeric and note styles.** A token whose words don't name its reference
  (`1` in IEEE) adds a quiet handle, first author and year ("1 Vale 2021"). A
  token longer than a third of the line, such as a note-style sentence, ends in
  "…". The popover names the reference in full.
- **Sorting styles.** Moving a token turns Keep My Order on, so the move shows
  in the output. While it is on in a style that would sort, the line says
  "Your order" at its end, and pressing that turns it off. The line slides
  rather than jumps when the order changes.
- **Distance from typing.**
  - ⌘↓ in Search goes to the last token, and ↓ or Esc goes back.
  - On a token: ← → walk the tokens, ⇧← ⇧→ move one, ⌫ removes one, and
    Space or ↑ opens its popover.
  - Every one of these is also a popover button with its key, and a command
    in More, so nothing exists only as a key or on hover.

## Why

- It keeps one view of the citation: what is edited is what is pasted, which
  the other two options split into chips and output.
- It needs no wider window and no second panel.
- It deletes the reference Inspector, the segment ×, and the count pop-up.

## Consequences

- `ui/Inspector` keeps the Reader and warnings.
- `ui/CitationToken` replaces `ui/ShelfSegment`.
- `ui/Popover` is the shared anchored popover.
- Motion uses the shared motion tokens:
  - a token settles in when added and slides when moved;
  - the popover grows from its token;
  - a removed token folds away, and Undo brings it back the same way.

  Reduce Motion cross-fades instead. No animation delays Enter or a paste.
