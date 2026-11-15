# 0045 The citation line is a token field

The citation line is the actual output Enter delivers. Each reference is a
token containing its output words, with a soft fill and an always-visible
remove button with Undo. Output brackets and separators remain plain text.
Search remains a separate text field.

A token opens an upward popover for this citation's form, page/type, prefix and
suffix. Reference actions live in More and shortcuts. Reader and warnings use
the side panel. This keeps editing on the output without widening the window.

Numeric tokens add a quiet author/year handle. Long tokens end in an ellipsis
after a third of the line; details and tooltips expose the full text. Moving a
token enables Keep My Order and reports it in the footer.

⌘↓ from Search focuses the last token. Left/Right move focus, Shift+Left/Right
reorder, Delete removes, Down or Space opens details, and Up or Esc returns to
Search. More lists the same actions. Enter always delivers.

`ui/CitationToken` owns token presentation, `ui/Popover` the anchored surface,
and `ui/Inspector` the Reader and warnings. Shared motion tokens govern entry,
moves, removal and Undo; Reduce Motion fades or uses the final state. Motion
never delays Enter.
