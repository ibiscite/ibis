# 0047 Opening references, where the picker appears, and going back

## Where the picker appears

- The search row sits a quarter of the way down the usable area of the screen
  that holds the app you are writing in (the focused window), not the screen
  with the pointer. The usable area excludes the menu bar, the notch, the Dock
  and the taskbar wherever it is. It is centred horizontally.
- A position you drag to is remembered per screen, relative to that screen's
  usable area, and clamped when screens change. Reset Window Position restores
  the default.
- The window keeps one size while open: results, palettes, sheets and the
  Inspector change inside it (principle "Nothing jumps").
- On Wayland the desktop places windows; Ibis asks for the same position and
  accepts the desktop's answer.

## Going back

| Where | Esc | Backspace in an empty field | ⌘[ (macOS), Alt+Left (Windows, Linux), mouse Back |
| --- | --- | --- | --- |
| Picker search | Clear the search, then hide | Remove the last filter chip | Nothing |
| Palette (formats, More) | Back one view | Back one view | Back one view |
| Inspector | Close it | Edits only | Close it |
| Sheet or form | Close an open pop-up, else back | Edits only | Back, keeping what was typed |

A held Backspace never navigates: it must be released and pressed again.
Input-method composition and an open pop-up take precedence. Every return
restores the search, selection, scroll and focus it left.

## Opening the reference

⌘O on macOS and Ctrl+O elsewhere open the selected source. File libraries open
in the system's default app; Zotero selects the item, starting Zotero if needed.
With no item, Open Zotero opens the library. A missing file offers Find instead.

For files, ⌥⌘O on macOS or Ctrl+Shift+O elsewhere reveals the source in Finder,
Explorer or Files. Holding the alternate modifier also exposes that action.
Ctrl+Alt with a letter is avoided because it types characters on AltGr layouts.
More lists these actions with the same keys.

Inside Zotero, Find & Open uses Enter to open the item's first available PDF
in Zotero, otherwise the item. An explicit Show in Zotero skips PDF lookup.
An open belongs to the captured item, source revision and picker opening;
changed sources refuse before a delayed launch. IPC accepts native identities,
never paths or arbitrary commands.

Configurable editors/viewers, attachment menus and page-aware opening are
[planned](../../ROADMAP.md#04-citation-and-integration-work). Printed citation
pages must never be assumed to be physical PDF indices.
