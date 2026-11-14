# 0021 Interface foundation

Interactive controls use exactly pinned React Aria Components, imported only
through `apps/desktop/src/ui/`. No second component library or CSS framework.
Hand-written `tokens.css` has base, role and platform layers; components read
role tokens. The menu bar, tray and context menus use native menus.

Icons use [0044's shared primitive](0044-lucide-icon-primitive.md), with artwork
from `brand/`. On macOS the installed Zotero icon is resolved through
NSWorkspace by bundle ID; Ibis ships none of Zotero's artwork. Failed reads,
missing apps and other systems use Ibis's library glyph.
