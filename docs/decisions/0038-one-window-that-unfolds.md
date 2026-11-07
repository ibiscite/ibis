# 0038 One window that unfolds

2026-11-04 (agent design pass, asked for by the maintainer: "think out of the
box… form will follow function"). Three directions were drawn on the design
canvas and compared against today's jobs (search, cite several references with
pages, choose the output, recover) and the planned ones (read a PDF, pick a page
or passage, quote, browse collections, check a manuscript):

| | A. One window that unfolds (chosen) | B. Tokens in the search field | C. Library workspace |
| --- | --- | --- | --- |
| Model | Spotlight-sized picker; details, then pages, open beside the list in the same window | Mail's address field: each reference is a token in the search bar with a popover for page and form | Mail or Xcode: a sidebar of collections, a list, an inspector; the quick picker becomes a second, compact mode |
| Quick cite | Unchanged: type, Enter | Unchanged | A second product to learn |
| Several references and pages | The shelf, one segment each, details beside | Tokens crowd the field the person types into, and `#tag` chips already live there | Fine, but behind the workspace |
| PDF and page picking | In the same window: list, pages, details | A separate reader window: Esc, focus and the paste target split across two windows | Natural |
| Collections, manuscript check | A search scope and a sheet | No home | Natural |
| Keyboard | One focus path, one Esc ladder | Popovers inside a text field fight the caret | Many panes to Tab through |

**A** keeps what already works (search first, one steady window, the shelf as
the one place the citation is built, the footer's one action) and gives every
planned job a place without a redesign: details and pages unfold toward the
trailing side, and nothing the person was looking at moves. B is beautiful for
one reference and breaks at three with pages; C is a second app. Mac precedents:
Spotlight and Raycast for the launcher, Preview and Skim for a sidebar of page
thumbnails beside the pages, Finder and Pages for an inspector beside content,
System Settings for its own window.

- **Reading has its own shape** (approved by the maintainer, 2026-11-04). Drawing the boards showed a portrait page has
  no room in the steady 480-point window and that list, thumbnails, pages and
  column side by side are too many regions. While a PDF is open the window grows
  once, taller and wider, and the list folds away; search, shelf, footer and the
  column stay.
- **Unfolds, never pops.** Details and the reader are regions of the picker, not
  overlays or windows, so the shelf, footer and Enter stay live and Esc steps back
  one region at a time. Only menus, the keys sheet and dialogs float.
- **Pages are chosen, never followed.** Choosing a page is an explicit act: a
  thumbnail click (⇧-click a range, ⌘-click another page, as Preview selects
  pages) or **Cite This Page** (⌘↵). Scrolling to read never changes the Page
  field. This replaces the earlier plan that Page follows the page in view.
- **Printed page first, physical page beside it.** Every page shows its printed
  label; one without a citable label shows "PDF 8" and asks once for its printed
  number, which then offsets its neighbours that also lack labels (decision 0036).
- **The reader's shell ships before the PDF engine.** The layout, thumbnails and
  page choice are built against a page source interface with an invented source
  for the canvas and tests. Show PDF stays out of the running app until #1252
  closes: PDF.js lands behind the same interface with decision 0036's boundary.
- **Kept, by name:** the anatomy (search, list, shelf, footer), one steady window
  except the reader's widening, the shelf, the inspector column, the footer's one
  action, the format pop-up and the choice screens. **Replaced:** the inspector's
  repeated title and its separate document block, menu example columns (now a
  second line under the label), the format menu's two-line note (now one
  heading), the separate Check for Updates row, and Page following scroll.
- **Chrome only where it acts** (same day, from the canvas pictures): a resting
  result row has no tile plate, the footer's main action is quiet text with the
  footer's one accent on its ↵ cap (as Raycast's), the search bar's format pop-up
  is quiet like its neighbours, Settings cards separate by fill alone, and More
  no longer repeats the footer's library line. Each removes an element that
  repeated what something nearby already said.
