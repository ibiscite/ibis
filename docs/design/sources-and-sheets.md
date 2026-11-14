# Source checks, passages and cited references

Every sheet uses [`ui/Sheet`](../../apps/desktop/src/ui/README.md), built on the same `ViewFrame` as the palettes. Its content composes the [layout parts](../design.md#how-the-design-changes); the screen supplies no spacing or separate overlay frame.

Back is a 32 px square (44 px with a coarse pointer, scaled with text), with the shared control-size chevron centred on the search bird and the rows' tile column, vertically centred on the search field. It is transparent at rest, with neutral hover and a keyboard-only focus ring, and has an accessible label without a native tooltip. The search placeholder names a palette visually; its title only names the dialog for accessibility. A sheet without search keeps its title on the label column. The footer's Back and Esc keep one baseline without wrapping. Back or Esc restores the parent's query, selection, scroll and focus; reveal the parent before returning focus, without scrolling it.

## Source checks and passage reuse (planned)

Approved directions from ROADMAP.md; planned behaviour. No new notebook, source writes or cloud access; agents get notes and annotations only once the person turns that on for a library (decision 0033), and never private links.

**Check the bibliography.** Does the exact key occur in one chosen read-only `.bib`? Name that file, never "the whole document". Kept separate from loaded search libraries; an unknown result is never "missing", and Ibis never silently swaps a key.

With a writing selected, choosing or removing this bibliography changes that writing. With none selected the check lasts only for this session. A writing with no bibliography never borrows an earlier writing's file.

| Checked-file state | What the person sees |
| --- | --- |
| One exact occurrence | In thesis.bib; inspect the project entry |
| No occurrence after a complete scan | Missing from thesis.bib; copy an available entry or re-export |
| Unreadable, incomplete or uncertain | Couldn't check thesis.bib; retry or find the file |
| Several occurrences | Key appears more than once; inspect its locations |
| Conflicting fields | Key may name another reference; compare the entries |
| Key is known to identify a different work | Refuse this key citation until the conflict is resolved |

Built for key outputs: **More › Check Against a .bib File…** chooses the file, and each result row then says its state in words, naming the file ("In thesis.bib"), with the lines to look at for a repeated or doubtful key. The file is read when chosen, at launch and when the picker opens; nothing polls. More names the file, what is wrong with it if anything, and offers **Stop Checking thesis.bib**. A key known to identify a different work is refused: Enter neither copies nor pastes that key, keeps the selection and explains the conflict. This rule replaces warn-and-copy for that case (maintainer, 2026-11-04, decision 0028). Other states outside "In" join the citation's warnings, in the preview and in the copy's own result, so Enter copies, says why and stays open, an unreadable file included. Copy Entry says the entry comes as the file has it, `file` and `note` fields included. Formatted citations show no marks: a style has no key to check.

**Unreadable writing bibliography** (maintainer, 2026-11-05; decision 0028): a check is advice, not a gate. When thesis.bib can't be read, the citation stays available and says "Couldn't check thesis.bib" with Retry; only a key known to name a different work is refused. A key written more than once is a warning too, since no entry can be chosen between them. Last-good copies of the bibliography and one-off unverified actions are withdrawn.

- **Copy bibliography entry** (More, on a missing row) keeps the key, never edits a source, and supplies a self-contained entry or explains how to export one.

## 0.5 walkthroughs and presentation

[Decision 0055](../decisions/0055-find-the-passage.md) is the canonical specification for passage presentation, named reuse and open actions, page provenance, briefs and copied-key warnings. It records engineering choices for planned behaviour. The invented walkthroughs below are design reasoning and acceptance targets; no participant evidence or new maintainer approval is claimed.

| Walkthrough | Steps and observable result |
| --- | --- |
| Bibliography-only user | Open invented `references.bib`, search `Sen`, select *Water Markers* (2022). References finds it without passage setup; the shelf and named Copy/Paste action show the citation Enter delivers. A source without passage capability offers no empty Passages scope. |
| Zotero user: highlight | Search `channel` in Passages for Vale's *Marsh Routes* (2024). Highlight contains “The east channel closed at dusk”; Enter focuses its detail, leaving clipboard and citation untouched. **Copy Quote with Reference** copies those source words and attribution; its comment is absent. |
| Zotero user: comment | Search `winter` for “Compare the winter survey”. Comment identifies the researcher's words and its associated highlight separately. **Add Note** reuses only the labelled comment and attribution; Copy Quote is unavailable. |
| Zotero user: attached note | Find “Check the tide table”, a Note attached to *Marsh Routes*. Detail says Attached note and names Vale's reference. Add Note reuses authored words; the note inherits neither the highlight's quotation nor its page. With no page evidence it says “Citation page unknown”. |
| Zotero user: standalone note | Find *Questions for the estuary visit*. Detail says “Standalone note · No reference · Citation page unknown”. Add Note retains those facts; no parent, citation or quote-with-reference action is invented. |
| Zotero user: image annotation | Select Vale's image-only annotation in Passages and press Enter. Detail identifies Image annotation and says “No text quotation”; a verified annotation/PDF target can open, but Copy Quote is unavailable and no image text is invented. |
| Independent brief reader | Export Vale's comment followed by its highlight and a passage with unresolved pages. Open the file without Ibis or Zotero. Each section repeats its full reference; the reader can distinguish authored comment from source quote, identify unresolved pages, and recognise the optional question as a question with no generated answer. Local Zotero links are labelled optional aids. |

Choose flat **References/Passages scopes**, as 0055's comparison explains: standalone notes need no invented parent, and passage selection cannot be confused with citation selection. Use the [existing picker anatomy](picker.md): scope controls in the list-heading band, one Search and results list, the Inspector for typed passage detail, the shelf slot and footer. References is the fresh-opening default; Passages selection previews detail, and its footer says **Open Passage Details ↵**. Enter focuses that same detail heading. Delivery uses named **Copy Quote with Reference** / **Add Note** actions, or the named citation action after returning to References. Keep the citation draft out of passage actions; its slot says **Citation kept · Back to References** when needed. No parallel picker or unfolding reference tree.

| Acceptance scenario for 0055 | Observable result |
| --- | --- |
| Keep Sen and a locator on the shelf; enter a Vale passage, then return by keyboard | The original reference query, selection, scroll, focus, citation order, locator and output return. Passage Enter delivers nothing; Enter in References again matches its named citation action. Repeat with an empty shelf: no Vale citation appears. |
| Fail **Open Annotation in Zotero** | The failure names that target and presents available **Open PDF at Page…** and **Show Reference in Zotero** alternatives; neither launches until chosen. A page-only target is labelled as such; opening only at the start is **Open PDF**. |
| PDF page 6 has printed label `xii`; another passage has no label or conflicting labels | Display PDF position separately from citation label. Never send `xii` as index 12; a printed-label open needs a verified mapping. A known PDF position remains openable independently. Show “Citation page unknown” or “Citation page ambiguous · Confirm page”; cancellation leaves it unresolved, with no inferred locator in a quote or brief. |
| Export a question and passage containing headings, brackets, backticks and HTML; race an existing destination, cancel, or fail saving | Supplied fields render literally as escaped Markdown/HTML, with no generated answer or injected structure. Passage order and repeated references survive. Exclusive new-file creation refuses an existing destination even after the chooser; cancellation writes nothing, failure preserves existing files and removes only this attempt's incomplete file. |
| Successfully copy A's `valeOld`, then its proven current key becomes `vale2024` | Preview names the old and current keys and offers Show Reference, Copy Current Citation and Forget Remembered Key. Preview/cancel changes no baseline. A successful receipt says “Copied vale2024” with that copy's warning; `vale2024` becomes the baseline and later previews clear the rename notice. A failed paste still reports that it was copied. No manuscript repair is claimed. |
| Copy fails, or copy succeeds but remembering it fails | Failed copy says it failed, keeps detail and the old baseline, and offers Retry. Successful copy with failed storage says the citation was copied but its key could not be remembered; the previous durable baseline remains. Never acknowledge a failed clipboard write as copied. |
| Retained A/`valeOld`; that key now provably belongs to B | Name both references and say the key now identifies B; offer Inspect References and Forget Remembered Key. A valid new selection of B remains deliverable. An ambiguous current binding is refused under the existing citation rules, never described as a proven rename. |
| Switch Zotero profiles or personal/group libraries; lose identity proof or see a reused identifier | Histories remain separately scoped; switching back restores only that library's history. Ambiguous identity yields no changed-key inference. Deletion/recreation ends continuity; a title, DOI, matching key or bare CSL-JSON `id` cannot transfer it. Temporary read failure preserves history without treating absence as deletion. |
| Forget one remembered key or all history while an earlier copy is pending; rename a plain `.bib` key | Forgotten history stays forgotten across restart and the pending completion; only a later deliberate successful copy establishes a baseline. Forgetting changes no document or clipboard. A `.bib` rename is untrackable and produces no rename warning. |

Remaining participant checks: a Zotero user must distinguish all five passage kinds, choose note versus quote reuse and return with a citation intact; a bibliography-only user must find and cite Sen and explain Enter after a mixed-library round trip; an independent reader must attribute each brief section and explain the literal question, unresolved pages and local links without either app. Record observed actions and misunderstandings on the implementation issue against [0055's human checks](../decisions/0055-find-the-passage.md#human-evidence-still-required-handoff-after-landing); these walkthroughs do not count as passes.

## Cited since opening

Copying captures the list's revision before formatting and checks it again at the clipboard write. An edit, including Reset followed by Undo, retires that copy. The sheet keeps the current list and says it changed; an obsolete copy or save response cannot replace newer feedback.

**More › Cited Since Opening…** opens a sheet, named for what Ibis knows: what it copied since it opened or since a reset, never the document's bibliography. Its lead says it covers **every project since Ibis opened**; choosing a project neither partitions nor clears the list. It lists each reference once, in the order first cited, with Remove; **Undo** names its action ("Undo Remove Okafor & Lima, 2018"), and undoing keeps anything copied since. **Reset List** is undoable too. Under the list is the bibliography in the output's style (APA when the output is keys), exactly what **Copy Bibliography** writes as rich and plain text together. **Save…** writes it as rich text, a web page or plain text. A reference from a library that isn't open is left out and counted, never guessed. The list holds record identities, in memory only, and leaves it only through Save.

Closing the sheet or replacing its list or style cancels unfinished bibliography
formatting, including Copy and Save. A cancelled action that has not yet written
the clipboard or file does not deliver later or delay the next citation.
