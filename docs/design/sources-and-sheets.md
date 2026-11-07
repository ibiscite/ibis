# Source checks, passages and cited references

## Source checks and passage reuse (planned)

Approved directions from ROADMAP.md; planned behaviour. No new notebook, source writes or cloud access; agents get notes and annotations only once the person turns that on for a library (decision 0033), and never private links.

**Check the bibliography.** Does the exact key occur in one chosen read-only `.bib`? Name that file, never "the whole document". Kept separate from loaded search libraries; an unknown result is never "missing", and Ibis never silently swaps a key.

With a writing project selected, choosing or removing this bibliography changes that project. With none selected it changes the standalone check. A project with no bibliography never borrows the standalone check or a previous project's file.

| Checked-file state | What the person sees |
| --- | --- |
| One exact occurrence | In thesis.bib; inspect the project entry |
| No occurrence after a complete scan | Missing from thesis.bib; copy an available entry or re-export |
| Unreadable, incomplete or uncertain | Couldn't check thesis.bib; retry or find the file |
| Several occurrences | Key appears more than once; inspect its locations |
| Conflicting fields | Key may name another reference; compare the entries |
| Key is known to identify a different work | Refuse this key citation until the conflict is resolved |

Built for key outputs: **More › Check Against a Project Bibliography…** chooses the file, and each result row then says its state in words, naming the file ("In thesis.bib"), with the lines to look at for a repeated or doubtful key. The file is read when chosen, at launch and when the picker opens; nothing polls. More names the file, what is wrong with it if anything, and offers **Stop Checking thesis.bib**. A key known to identify a different work is refused: Enter neither copies nor pastes that key, keeps the selection and explains the conflict. This rule replaces warn-and-copy for that case (maintainer, 2026-11-04, decision 0028). Other states outside "In" join the citation's warnings, in the preview and in the copy's own result, so Enter copies, says why and stays open, an unreadable file included. Copy Entry says the entry comes as the file has it, `file` and `note` fields included. Formatted citations show no marks: a style has no key to check.

**Unreadable project bibliography** (maintainer, 2026-11-05; decision 0028): a check is advice, not a gate. When thesis.bib can't be read, the citation stays available and says "Couldn't check thesis.bib" with Retry; only a key known to name a different work is refused. A key written more than once is a warning too, since no entry can be chosen between them. Last-good copies of the bibliography and one-off unverified actions are withdrawn.

- **Copy bibliography entry** (More, on a missing row) keeps the key, never edits a source, and supplies a self-contained entry or explains how to export one.
- **Find a passage**: search Zotero highlights, notes and comments, labelled Highlight, Comment or Note; a note is never treated as a quotation from a guessed reference; image-only annotations show "No text quotation".
- **Open passage** opens a verified Zotero annotation, or says it is page-only. Physical and printed pages show apart ("PDF page 8 · cite p. 417"); an unknown printed page shows "Citation page unknown".
- **Copy quote with reference**: the stored text, printed page, reference and link as plain text or escaped Markdown; comments stay separate; nothing summarised or truncated.
- **Export evidence brief**: chosen passages in order, an optional question, one new Markdown file; local Zotero links labelled apart; no private paths.
- **Changed copied key**: "You previously copied oldKey; this item's current key is newKey", only for sources whose identity survives a rename (Zotero, CSL-JSON with a stable `id`).

## References cited since Ibis opened

**More › Cited Since Opening…** opens a sheet, named for what Ibis knows: what it copied since it opened or since a reset, never the document's bibliography. Its lead says it covers **every project since Ibis opened**; choosing a project neither partitions nor clears the list. It lists each reference once, in the order first cited, with Remove; **Undo** names its action ("Undo Remove Okafor & Lima, 2018"), and undoing keeps anything copied since. **Reset List** is undoable too. Under the list is the bibliography in the output's style (APA when the output is keys), exactly what **Copy Bibliography** writes as rich and plain text together. **Save…** writes it as rich text, a web page or plain text. A reference from a library that isn't open is left out and counted, never guessed. The list holds record identities, in memory only, and leaves it only through Save.
