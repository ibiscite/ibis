# 0047 Opening references, where the picker appears, and going back

Decided by the maintainer on 2026-11-12, after research into launchers and
command palettes on macOS, Windows and Linux (Spotlight, Raycast, Alfred,
PowerToys, Flow Launcher, KRunner, Ulauncher, Rofi; Linear, Notion, VS Code,
Zed) and into how research tools open references (Zotero's own links, Better
BibTeX, Bookends, Paperpile, JabRef, editors that open a file at a line).

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

Keys: macOS ⌘O opens, ⌥⌘O is the alternate, and holding ⌥ shows alternates.
Windows and Linux use Ctrl+O and Ctrl+Shift+O, and holding Shift shows
alternates, because Ctrl+Alt with a letter types characters (AltGr) on many
keyboard layouts. Every alternate is also listed in More.

| Library | ⌘O (default) | Alternate | Also in More |
| --- | --- | --- | --- |
| Zotero, a reference selected | Show in Zotero: selects the item (`zotero://select`), starting Zotero if needed | Open its PDF in Zotero's reader | Show PDF in Finder, Explorer or Files when a local attachment exists |
| Zotero, nothing selected | Open Zotero | None | None (there is no library-level Show in Finder for Zotero) |
| `.bib`, CSL-JSON, RIS, CSV | Open the file: the chosen editor at the entry's line where it supports lines (VS Code, Zed, Sublime Text, BBEdit, TextMate), otherwise the system's default app | Show the file in Finder, Explorer or Files | Open the attached PDF, where the format records a file (BibTeX `file`, RIS `L1`) |

- **Configurable** in Settings › Libraries › Opening references: Zotero's ⌘O
  is Show in Zotero, Open PDF in Zotero, or Open PDF in the default app; file
  libraries' ⌘O is Open the file or Open the attached PDF. Whatever is not
  chosen becomes the alternate. Details… beside it holds the editor and the
  PDF viewer. Settings gets a search that finds every option, opens its
  details and highlights it; a changed option shows Use Default.
- **Several PDFs:** the default opens the first PDF attachment; More lists
  each by name.
- **No PDF:** Ibis says so in one line ("Vale 2024 has no PDF") and offers
  Show in Zotero or Open the file. It never silently does something else.
- **Missing or moved file:** the existing fix action ("Find thesis.bib…").
- **Pages:** a PDF opens at the cited page only when Ibis knows how printed
  page numbers map to the PDF's pages; otherwise it opens at the start.
  A printed "417" is never assumed to be PDF page 417.
- **Read-only and local:** Ibis opens files and Zotero links through the
  system and changes nothing. A web-only attachment opens in the browser by the
  person's action; Ibis itself fetches nothing.
