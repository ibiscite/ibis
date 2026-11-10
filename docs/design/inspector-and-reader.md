# Inspector and Reader

The regions that unfold beside the list ([the model](../design.md#the-model), decision 0038).

## Citation details

[C23] One reference's citation details are a popover on its token (decision 0044), as Zotero's bubbles open theirs: they belong to this citation and are forgotten after the paste.

- **Where**: above the token, anchored to it, with its arrow on the token, growing from it in the shared overlay material and staying inside the window, `--inspector-width` wide. Where the list can scroll, the highlighted result moves clear of it. It is not modal: Search, the list, the line and the footer stay live.
- **Only this citation's details**, on one label column, with no actions: Open the source, Show in Zotero and the rest act on the reference, so they live in More and on its result row; Remove is the token's ×; moves are ⇧← ⇧→ and More.
  - **Show**: a segmented control whose segments are the real outputs for this reference ("(Vale, 2021) | Vale (2021) | (2021)"), from the engine, offering only the forms this output can print. Where none can, one quiet line says why.
  - **Page**: the value field, with the locator type as a quiet pop-up at its start ("p. ▾ 417"), drawn as every pop-up is; its menu lists all 29 CSL locator types and narrows as you type. It takes ranges and lists ("417", "xii–xiv"), a typed label names the type ("ch. 3" sets Chapter), and text that is not a locator stays literal. The engine names any output that cannot print it.
  - **+ Text before** and **+ Text after**: quiet links that reveal the prefix and suffix fields; a field with text stays shown. The engine formats and escapes the text.
  - **Keep My Order** at the foot, only when the style sorts.
  - A refusal or warning about the reference shows in words at the foot.
- **Keys**: focus starts in the Page field. Tab moves through it in order, never trapped. Enter in a field does what the footer says. Esc closes an open menu, then the details, and focus returns to the token. Pressing another token moves them; pressing the same one closes them.

## Inspector

[C19] The Reader and a citation's warnings are never an overlay. They open in a panel beside the list, as Preview's sidebar does.

- **Where**: in the middle row, on the trailing edge, `--inspector-width` wide, with the shelf running under both. Its divider runs the whole row it splits, as a Mac split view's does: it meets the search bar's divider at the top and the shelf at the bottom, and the content keeps S inside it (checked by `canvas/geometry.ts`). The window stays its one steady size. Search, list, shelf and footer stay where they are and stay live. Below 36 rem wide the panel takes the list's place instead.
- **Warnings** list what the engine said about the citation, in words.
- **A wider panel**: at 34 rem the column keeps its width on the trailing edge and the document sits beside it, scrolling alone.

## Reader (planned, #1252)

[C20] A read-only view of a reference's PDF that is already in Zotero or a `.bib` `file` field, for looking at a source and choosing its page without opening Zotero (decision 0036). Ibis never imports, downloads, edits or annotates a PDF; **Open in Zotero** stays beside it.

- **Where**: in the list's place, beside the Inspector column. **Show PDF** (⌘D, Ctrl+D elsewhere) is a row in the column under the fields, and reads **Hide PDF** while open. Opening gives the window a reading shape once, from its top edge and within the screen: as wide as thumbnails, one page at Fit Width and the column, and as tall as the screen allows up to 1.6 times the steady height. It is the one planned exception to [C10], with no animated resize. The list folds away and keeps its selection; search, shelf and footer stay where they were. Typing in Search, Esc or Hide PDF returns the list and the steady size; the details stay open.
- **Anatomy** [C20], as Preview: thumbnails in a sidebar at the reader's leading edge with its own full-height divider (⌥⌘2 hides and shows them); the pages on one `--surface-grouped` canvas edge to edge between the dividers, each a sheet with one `--separator` edge and `--inset` between them; and the page in view in a small floating bar centred over the bottom of the pages, in the overlay material, holding the page indicator and **Cite This Page ⌘↵** (and, for a page without a printed number, its one question). No toolbar row takes height from the page; zoom is ⌘+ ⌘− ⌘0 and find is ⌘F, as in Preview. With several PDFs a pop-up names each by its title, never a path, and the reader asks before showing one.
- **Status** (decision 0038): the shell is built against `features/reader/source.ts`'s `PageSource` (printed labels in PDF order, and a cancellable `draw`), with an invented book in the canvas's `reader` scenario and the tests; `pages.ts` is the one model of cited pages for the thumbnails, Page and the shelf. No source exists in the running app, so Show PDF does not appear there. Before it does, #1252 settles: the PDF.js adapter implementing `PageSource` behind the bytes command (decision 0036), with `/PageLabels` read through `citableLabel`; the worker CSP and no-live-worker tests in WKWebView and WebView2; the native resize to the reading shape and back; the reader's keys in `KEY_TABLE` with their handlers (⌘D, ⌘↵, ⇧⌘↵, ⌥⌘2, ⌥↑ ⌥↓) and an "In the reader" group in the keys sheet; keeping a confirmed offset by the file's content hash (today it lasts one opening); and the maintainer's answer on Liberation Sans.
- **Thumbnails** show each page small with its printed label under it ("417", "xii"); a page without a citable label shows "PDF 8" in secondary text, while printed numbers read in the text colour. The cited pages are the selected thumbnails, filled as selected rows are; the page in view has a quiet ring of `--separator`, never the accent.
- **Page indicator**: "p. 417 · PDF page 8 of 312", `tabular-nums`. An unknown label reads "PDF page 8 of 312 · no printed page"; a label printed on two pages adds "also PDF page 3". The tooltip and accessible description say where the printed page came from: the PDF's page labels, a Zotero annotation, or an offset you confirmed. It never shows the physical number as the citation page. A label that is not a citable page (decision 0036) shows in quotes with "no printed page"; one outside the record's pages shows both ("p. 12 · record pp. 413–440").
- **Choosing the page** (decision 0038): explicit, never by scrolling. A click on a thumbnail cites that page; ⇧-click extends to a range; ⌘-click adds or removes one page, as Preview selects pages. In the pages, **Cite This Page** (⌘↵) cites the page in view and **Cite Through This Page** (⇧⌘↵) extends the range to it. The Page field, the thumbnails and the shelf segment always show the same pages, and typing in Page selects the matching thumbnails. A page with no citable label asks once, in the column, "What number is printed on PDF page 8?", with the field and **Use for Nearby Pages**; the answer is an offset you confirmed and applies only to unlabelled pages next to it. A page outside the record's range is cited only after **Cite Anyway**. Choosing a page never copies or pastes; Enter still does what the footer says.
- **Several references**: the reader shows the PDF of the segment whose details are open; ← → on the shelf move to the next reference's PDF, each keeping its place for this opening.
- **Go to a page** (⌥⌘G, Ctrl+G elsewhere): a field in the toolbar. "417" goes to the page printed 417; with none it says "No page printed 417 in this PDF" and offers **Go to PDF Page 417** as a named choice; with several it lists them. It never jumps silently.
- **Zoom**: Fit Width by default, then Actual Size and 50 to 400 %. With focus in the pages ⌘+, ⌘− and ⌘0 (Ctrl elsewhere) zoom as in Preview; elsewhere they keep changing text size. Pages redraw sharp at the screen's scale; zoom lasts the session only and is never a setting.
- **Find in the PDF** (⌘F, Ctrl+F elsewhere, with focus in the reader): the toolbar becomes a search field with "3 of 12" and previous and next; ↵ and ⌘G (F3) go forward, ⇧↵ and ⇧⌘G (Shift+F3) back, Esc closes it. Matches are a quiet fill, the current one `--selected`; the count, not colour, says where you are. It searches the PDF's own text, page by page in the worker, stops when closed and never runs while hidden. A page without text says so ("This page is an image without text"), never "no matches".
- **Keys**: all by key, Tab in order (thumbnails, pages, toolbar, column). In the thumbnails ↑ ↓ move and Space or Enter cites, with ⇧ and ⌘ as for clicks. In the pages: ↑ ↓ scroll, Space and ⇧Space or Page Down and Page Up by a screen, ⌥↑ ⌥↓ by a page, Home and End to the first and last page; typing there never goes to search. On Windows and Linux no reader key uses Ctrl+Alt, which is AltGr. Esc steps back: find, Go to page, the reader (focus returns to Show PDF), then the panel. The keys sheet lists the reader's keys under their own heading.
- **Screen readers**: the pages are one region named by the reference's title, never the PDF's own /Title; each page is a group named by its indicator words, holding its text in reading order; the drawn page is hidden from them. Moving page by key announces the indicator once, politely; scrolling announces only where it settles.
- **Light, dark and contrast**: the paper is drawn as the PDF draws it, white in dark mode as in Preview; only the surround follows the theme. Increase Contrast adds `--stroke-field` to each sheet. Forced colours draw the page in `Canvas` and `CanvasText`, and text selection uses `Highlight`.
- **Motion**: page jumps and zoom are instant; nothing scrolls or zooms by animation, so Reduce Motion changes nothing here.
- **Right to left**: the panel and toolbar mirror; the pages, their text and the numbers in the indicator never do (each number is isolated).
- **Hidden and reopened**: hiding the picker closes the PDF and frees it. Reopening within a minute returns to the same page from memory; the place is never saved to disk.
- **Where it is unavailable**: on Linux until the web view sandbox is on, and where the web view's engine is too old (decision 0036), Show PDF stays, dimmed, with the reason in one quiet line; **Open in Zotero** still works.
- **When it cannot open**: the document area shows one sentence and one fix, centred, never a dialog; the column stays usable, so a page can still be typed. Nothing passes through the notice slot.

| State | Words | Fix |
| --- | --- | --- |
| No PDF | Show PDF is dimmed: "No PDF for this reference" | — |
| Missing file | Can't find this PDF | Show in Zotero, or Open thesis.bib |
| Not downloaded or in the cloud | This PDF isn't on this computer | Open in Zotero |
| Password-protected | This PDF is password-protected | Open in Zotero |
| Damaged | This PDF is damaged | Open in Zotero |
| Over 256 MiB | This PDF is too large to open in Ibis (412 MB) | Open in Zotero |
| Over 15 s | This PDF took too long to open | Try Again, or Open in Zotero |
| One page fails | That page shows "Couldn't draw this page"; the rest stay | — |
| An image over the limits | That page shows "Part of this page is too large to draw", never a blank page as its content | Open in Zotero |
