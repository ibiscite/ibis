# First run and choice screens

[C12] First run, no library, an empty library and a library that could not be opened share one pattern: one calm group centred in the steady frame, a title and one line, then result-style rows (tile, title, one line of help) in a column, and any note below.

- One row per goal: label the outcome, start with a verb, never two rows to the same place. At most one suggested row gets an accent tile.
- Rows are a list box: ↓ from search enters, arrows move, Enter or a click chooses. Choice screens hide the result heading, shelf and Copy.
- A superseded source search or file request reports nothing.
- **Welcome**: the whole window, with no search header. The mark, a title, the line "Ibis finds a reference and pastes its citation where you're typing. It only reads your library, never changes it.", then four starts as a 2 × 2 grid of tiles (Zotero, a .bib file, Find Them on This Computer, Try Sample References). Every tile is the same height and puts its glyph in the same place, level with its title; a fact such as "Found on this Mac" replaces the help line rather than adding one. The note names the registered shortcut ("After this, open Ibis from any app with ⇧⌘Y."), or says **No shortcut is set** with how to open Ibis instead. Esc dismisses a menu, then hides.
- **Opening a library**: nothing for 400 ms, then the mark, "Opening example.bib…", "Reading your library. Zotero can stay closed." and a thin progress bar. Rows kept from before stay instead.
- **No library**: "Where do your references live?" with the welcome grid; nothing is searched until Find is chosen.
- **Zotero found**: Ibis still asks. When Zotero is running or its library is on this computer, the welcome screen keeps every start and highlights Zotero, so Enter takes it. Only that look happens first; no library is read until it is chosen.
- **Empty library** (a successful load with zero references): "No references in thesis.bib", "Choose a library with references.", then **Use Zotero**, **Choose a Library File…**, and for a file **Open <file> ⌘O**; below, "Or drag a BibTeX, BibLaTeX, CSL-JSON, RIS or CSV file into this window." A file Ibis cannot read is never called empty: it is a failed open (below).
- **Could not open, nothing kept**: the title names the library and the fix, and the line under it says why in the backend’s words, never the title again; **Use Another Library** shows the no-library screen. Every row has a help line, the source row too ("Ibis carries on by itself once it’s open", "See the file in its own app"). A failure with references kept stays a notice above them.

| Problem | Title | Rows |
| --- | --- | --- |
| `missing` | Can’t find thesis.bib | Find thesis.bib, Use Another Library |
| `unreadable`, `other` | thesis.bib could not be opened / Can’t open thesis.bib | Try Again, Use Another Library |
| `zotero_closed` | Open Zotero to read your library ("Ibis usually reads it with Zotero closed, but not this time.") | Open Zotero (accented), Use Another Library |
| `zotero_missing` | Ibis can’t find Zotero’s library | Use Another Library, Try Again |
| `zotero_access_off` | Zotero isn’t sharing your library | Open Zotero, Use Another Library |
| `zotero_profiles` | Which Zotero profile? | One row per profile, Open Zotero, Use Another Library |

- Setup finishes itself: Ibis never asks "What do you write in?"; with Zotero closed Ibis reads the library anyway. A screen waiting on Zotero says where the fix is and asks Zotero every two seconds while it is shown, never while the picker is hidden, so it carries on by itself as soon as Zotero answers. With several profiles the closed path asks once, by the profile's name, and only a closed read that used it remembers the answer; a saved profile that is gone or unreadable asks again and never reads another. With references on screen the choice stays out of the way: the footer offers **Choose Zotero Profile…**, and the library menu lists the profiles whenever there are several and Zotero is not running, so a mistaken choice can be changed. A running Zotero answers for the profile it opened, so it offers no choice.
- **Try Sample References** loads three invented references (lantern gardens, clockwork clouds, the invented sea) for this run only, marked by the footer status "Sample references"; every desktop sample citation ends with ` [Sample]`. The website demo and help screenshots use the same set without the suffix. The practice task is identical on every surface: Search for “lantern”, choose the lantern gardens reference, then press Enter.
