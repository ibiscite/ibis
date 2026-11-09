# Picker

Search, list, shelf and footer ([design](../design.md#the-model)). The Inspector and Reader are in [inspector-and-reader.md](inspector-and-reader.md); copying, pasting and notices in [delivery.md](delivery.md).

## Search bar

- Order: search glyph, query, clear ×, format pop-up, pin. The header is its tallest control plus 2S, with a divider below [C2]. Its controls take the field's height, so the gap above and beside them is S and their corners nest in the window's (`--r-row`).
- `#tag`, `y:2019-2022` and `a:name` plus a space become removable chips; Backspace in an empty box removes the last.
- **Pin** [C6]: a neutral IconButton at the header's end; on, it shows the pressed fill, never the accent. Esc never unpins.
- The format pop-up names the exact output and, when the format belongs to something, whose it is (`APA · Word`, `APA · Thesis`); the default carries no suffix. One output has one name everywhere, the engine's (decision 0035). Hidden until a library is chosen.

## Results

- **List and row**: type tile, title, meta line, trailing year (or "No date") level with the title's first line in every density. Comfortable's meta line: authors as in a citation, then the container; Compact puts the authors after the title. One rule in both: one or two authors by surname ("García, Okafor"), three or more as "García et al.". The citation key is in the accessible name, not the row.
- **One record summary** (#1675): the row, `ibis search`, `ibis show` and agents read a record's title, authors, year and container from the engine's one summary, so a record shows the same year everywhere. The year is the date's year (also when a source writes it as text, "2016"), else the source's own date text cut to 20 characters ("circa spring 1890s"), else "No date". A year written as text is found by searching for it.
- **Chrome only where it acts**: a resting row shows its type as a glyph alone, with no plate; the plate appears where the tile does something (hover and selection: the add button, + or a check) or says something (a row in the citation: the scarlet tile with a check), without reflow.
- Titles wrap to two lines then end in an ellipsis; the byline stays one line. Opening details beside the list never changes a row's height; in a list narrower than 30 rem (details open, larger text) the byline keeps the authors whole and shows the journal only when it fits whole, never cut to a stub; Peek and the details always show it.
- **Peek** (⌘Y, as Quick Look by keyboard; Space types a space in the search): the highlighted row's full details in a card under it, with the menu's material: title, every author, where it was published, year, type and citation key. It never takes focus, so typing carries on; ⌘Y again, Esc, or a new highlight closes it.
- **Command search**: ">" first in the search lists the picker's commands as one-line rows with their keys, as Raycast does; words narrow them by title or description, ↑ ↓ move, Enter or a click runs one and clears the search, and the footer reads "Run <command> ↵". The library is not searched and nothing would be pasted. Each command's title, key and description come from the menus' words and `KEY_TABLE` (`features/picker/commands.ts`), so a command is written once.
- **Page with the search**: a trailing page typed after the words ("vale 417", "vale pp. 417-419") becomes the "p. 417" token beside the query and the highlighted reference's page; × on the token searches as typed.
- **Drag the citation**: the shelf's beak is the citation's proxy, as a Mac window's title icon is its document's: drag it into any app to drop exactly what Enter pastes (plain text, and rich text for a formatted style). It drags only what Enter would deliver: not while the citation is being made or is out of date, and never a refused one.
- **Hover** [C5] only tints a row under a moving pointer; a click selects, so reaching across rows for Copy never changes what is copied. Double-click copies.
- **List scrolling**: a clicked row stays under the pointer and only that row consumes the click, even if results re-render first. Arrow keys take back ownership.
- **Empty search** [C11]: a new library shows the hint ("Type to find a reference") centred in the middle area, with no keys. With history it shows **Cited here recently** (up to three, for the app Ibis came from) then **Recently added** (three), as ordinary rows navigated as one list.
- **Typing keeps the picker still**: rows, headings, shelf and footer stay until the next reply; nothing blanks. A click on a row from an earlier query does nothing.
- No matches: "No matching references", "Try a surname, year or fewer words.", then Clear Search. When something else is the obstacle the screen names it, with its one fix: a filter (**Remove Filter** keeps the plain words) or "Not in your libraries" for a pasted citation. Left-out Zotero libraries are where else to look, not the obstacle: the title stays "No matching references", the line says some Zotero libraries are left out, and the fix is **Search All Zotero Libraries**. A source failure and an empty library keep their own screens. When nothing matches every word, closest matches show under **Closest matches** and keep a "Closest match" pill on the shelf.

## Search rules

Built in `ibis-core` so every front end shares them (#1204).

- Any word order; case, accents and letter variants folded (`mueller` finds Müller); "et al." and "and" ignored; every word must match.
- Per word: whole word, then word start, then inside a word (three or more characters, or CJK), then one typo in words of five or more letters. Numbers, years, identifiers and tags are never guessed.
- Field hints: `a:` people, `t:` title, `j:` journal, `k:` key, `#` tag, `y:` year or range; quotes keep a phrase. An exact citation key always matches.
- Ranking: exact key, exact title, matches as typed, then typos, then abstracts; field weight; newest year; stable order.
- **Pasted lists** (#1326): keys in any supported citation syntax, or identifiers, in paste order under "Pasted list"; missing ones under "Not in your library: …". Up to 1,000 entries or 64 KiB; otherwise ordinary search.

## Shelf

- **Shelf** [C7]: one line above the footer showing what Enter pastes: the beak at the tile column's end, the citation on the title column. Shown while there are results or a citation is being built; hidden on choice screens. Results never move when a reference is added.
- Several references are segments of one citation, each with its page ("Vale 2021, p. 417"); a pointed segment shows its hover fill and grows at its trailing end to make room for a small ×, `--key-gap` from the edge on every side. Nothing ever covers text. A segment is the reference's own words: the brackets and separators the output adds are shared, quieter and never part of a segment, for one reference too, and a segment's fill hugs its words as a text selection does. ← → select a segment; Delete removes it with Undo; ↑ ↓ return to Search and move through the list, as they do there. The shelf itself is never a focus stop while it has segments, so no ring ever surrounds the whole shelf. Segment text starts exactly on the title column. A focused segment shows the ring every button has; the segment whose details are open keeps the pressed fill, as a toggled-on button does.
- **One line, details on demand**: a segment opens the **Inspector**. The count is a pop-up of every reference with its page; a pick opens that reference there.
- On copy a closing beak mirrors the opening one and a "Copied" capsule draws its check; Reduce Motion shows the end state. Each copy is confirmed once, in one place.
- Pending output dims, then "Formatting…"; Enter waits. A failed format says why and offers another style; refusals stay visible until corrected. Every preview ends in a result or a stated failure: an engine stop is asked again once (after the destination read, when one is running), and a second stop says the citation could not be prepared, so Copy is never left waiting on "Formatting…".
- The shelf keeps the last citation through a running search and when the next search finds nothing.

## Footer

- **Footer** [C8]: status dot and text at the leading edge (dot on the tiles' column), the source action, the one primary action ("Paste into Word · APA ↵" or "Copy N as <format> ↵"), and last, on the rows' trailing edge, "More ⌘K".
- **More never moves**: it has the last column of its own, so nothing about the action moves it: not a longer label, not Alt, not the action being absent. A click aimed at More can never land on Copy. The action sits against it and grows toward the status. The leading slot always keeps room for its fix ("Find thesis.bib…"), so the fix never wraps onto the action; a failure line under them spans the footer.
- Every hint is label first, then keys, on one baseline. When the footer does not fit, shorten the status, then truncate the action's label with "…", its keys and full name kept, and only by the window's width drop hint labels to keys. The action's own width never decides that fold. With ⌥ held, its two actions cannot both truncate on one line, so their labels fold to keys when their full names do not fit; More keeps its label either way.
- **Quiet bands**: nothing in the shelf or footer is a filled button. The main action (Copy or Paste) reads as Raycast's: its words in the text colour at medium weight and the footer's one accent on its ↵ cap, so the eye rests on the results, not on the bottom edge. The library status, the source action, More, the reference count, notes and Details are quiet.
- Never show an action twice on one screen. With ⌥ held: "Add to citation ⇧↵", the Esc step, then More.
- **One place for each status**: library state lives only in the footer status; citation refusals and settings notices use the middle area; anything passing uses the notice slot (see [Notices](delivery.md#notices)).
- **One reading of a library's health**: the dot, the hover text and the library menu all draw from `libraryCondition`. A problem in words is never a green dot: anything the menu reports makes the dot at least Look.

| Case | Footer |
| --- | --- |
| Starting, first run | "Checking library…", "No library chosen" |
| Loaded | "Zotero · 1,204 references" or "thesis.bib · 1,204 references" |
| Changed while open | "… · updated 2 minutes ago" (only when references changed) |
| Reloading | "… · reloading…", count kept; rows and the citation stay until it ends |
| Entries skipped or to check | "damaged.bib · 3 references · 4 entries skipped · 1 to check", look dot; the hover text and the library menu's heading say why and where: "4 entries skipped: 2 use the key good1 (lines 3 and 9), 2 could not be read (lines 21 and 25). Check line 15." |
| Problem, old copy kept | Warning dot, never the stop red, since the old copy is still searched; "Zotero not updated · 2 h", then one fix button with its key ("Open Zotero ⌘O", "Find thesis.bib…") |

- Counts include bibliographic records only. Background work shows a small progress ring before the words.
- **Failed refresh with a last-good copy** (planned, maintainer, 2026-11-04): keep a coherent snapshot of the same established source searchable and citable. The persistent source status clearly says that refreshing failed and gives the last successful read time; it clears after a successful refresh. Enter keeps the action the footer promises, without a repeated confirmation or a forced switch from paste to copy. This does not permit a citation whose source identity is uncertain or whose selected work or key binding is known to have changed (decision 0028).
- The status opens the **library menu** (`top end`, like every footer menu): a heading with the library, its count and age, then a separator; with several Zotero profiles and Zotero not running, a **Zotero Profile** heading with one checked row each; with several Zotero libraries, a **Search in** heading, All Libraries, then one checked row per library; a library that could not load stays dimmed with "Couldn’t load"; then, after a separator, Choose Library… ⇧⌘O, Use Zotero for a file, and Reload Library.
- **Open the source** (⌘O): the words always name the target. While opening Zotero is the fix, ⌘O opens Zotero, so the fix button and More both read "Open Zotero ⌘O". **Show in Zotero** selects the reference in Zotero (starting it if closed); **Open thesis.bib** opens the file; **Open Zotero** only when Zotero is the source and not running. ⌥ turns "Open thesis.bib" into "Show in Finder". If the source cannot open, offer Choose Library… or Find….

## Keys

`KEY_TABLE` in [`apps/desktop/src/keys.ts`](../../apps/desktop/src/keys.ts) is the one source of shortcuts; the keys sheet, footer and More read it. A key is added only with the code that handles it.

| Input | Does |
| --- | --- |
| ⇧⌘Y (Ctrl+Shift+Y elsewhere) | Opens Ibis from any app (the default; follows the active layout's letter) |
| Click / double-click a row | Selects [C5] / pastes or copies what the shelf shows |
| Enter, or the footer action | Pastes or copies what the shelf shows; success hides an unpinned window |
| ⌘-click, the tile’s +, ⇧Enter | Adds to the citation, in order |
| ← → / ⌫ or ⌦ on a shelf segment | Moves between the citation’s references / removes the focused one, with Undo |
| Esc | Steps back one layer, from any control: sheet, menu, inspector, library chooser, then clears search (the shelf stays), then hides. A held Esc takes one step |
| ⌘O / ⇧⌘O | Opens the source / Choose Library… |
| ⌘K | More |
| ⌘/, or `?` outside a field | Keyboard shortcuts |
| ⇧⌘P | Keep window open |
| ⇧⌘F | The format chooser |
| ⌘Y | Peek at the highlighted reference |
| `>` first in the search | Command search |
| ⌘E | Change the last citation for this app |
| ⌘+ / ⌘− | Text size |
| ⌘Z | Undo what the notice slot offers |
| Typing outside a field | Types into search, except in the reader's pages |

## Window behaviour

- **Opening**: on the focused window's display, at its last place there (first time: centred, top in the upper third), with a short fade and focus in search. A search from under a minute ago returns selected, unless Esc or Clear cleared it: an explicit clear sticks. Text typed while opening is kept. The window is current in its first frame.
- **Dropping a file**: one `.bib`, `.biblatex`, CSL-JSON, `.ris` or `.csv` file dropped on the picker (a `.txt` RIS export only through Choose Library) becomes the library, and the footer shows its name and count. Anything else leaves the library alone and says what Ibis accepts until the next good drop or opening.
- **Moving and resizing**: drag the empty header; snap to centre, upper third and corners. Settings › Advanced › **Where Ibis opens** offers Centre and the four corners; **Reset Window Position** restores the default.
- **Other windows**: a command that hands the screen to another window has one effect however it was reached, and Esc then belongs to that window. Settings always closes the picker, even a pinned one, which would otherwise float above it; the pin stays for the next opening. An opened source hides an unpinned picker; pinned, it stays beside. A file chooser belongs to the picker, which stays behind it.
- **Clicking outside** hides Ibis and the click reaches its target, unless pinned or a dialog, drag or file chooser is open.
- Overlays close when the picker hides and do not reopen. Tooltips appear after 600 ms and never take focus.
- **Never** take keys people type with (so not Ctrl+Space or Ctrl+Alt), open by itself, appear in the Dock, taskbar or Alt+Tab (only Settings and a pinned picker do), or animate past 280 ms. On X11 the first press over a full-screen app is ignored and a second within two seconds opens Ibis.
- **Platform limits**: macOS a non-activating floating panel, also over full-screen apps; Windows copies instead of typing into elevated apps; Wayland copies and says "Press Ctrl+V" ([compatibility](../compatibility.md#linux-shortcuts-and-tray)).
- Hidden Ibis waits for events; nothing polls while it is hidden.
