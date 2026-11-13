# 0042 Formats, apps and writings

2026-11-10 (maintainer). This replaces the parts of 0031 about formats and writing projects; 0031's limits on what is stored for each app still apply. The implementation follows consolidation step 7's picker session and the 0.4 citation work.

The design passes establish the shared frame now: the format palette, the actions palette and every sheet compose `ui/ViewFrame`, with layout tokens and `ui/` parts owning all spacing ([layout rule](../design.md#how-the-design-changes)). The engine resolves formats by app kind and optional writing. The palette previews chosen references for key formats and a labelled example for citation styles.

**One concept people see: the format** (APA, MLA, Pandoc keys, `\cite`…). Nobody has to configure more than one format. Ibis decides which format applies, in this order:

1. **A writing, if you chose one for this app.** A writing is optional and opt-in: a name, a format and optionally a `.bib` to check against ("Thesis · Chicago notes"). You create one from the format palette. Ibis never guesses it from a document, a window title or a URL.
2. **This app's format, if you changed the format while in it.** Ibis remembers that without asking. The header says "MLA · for Word", and the palette offers to go back to your usual format.
3. **Your format:** the one you chose in Settings for this kind of app. It applies everywhere else.
4. **A suggestion:** used until you choose a format. First run asks nothing.

Your format applies to apps of its own kind (maintainer, 2026-11-10): a style in word processors and apps Ibis does not know, a key syntax in Markdown, LaTeX and Typst apps. So it is two values, shown in Settings as "In documents: APA" and "In Markdown and LaTeX: Pandoc keys", the second only once it differs from the suggestion or once you have cited in such an app. An app of the other kind gets its kind's suggestion until you change the format there, which sets that app's format.

The header always shows the format in use and, when it isn't your usual one, why ("for Word", "Thesis"). Someone who never changes anything never sees app formats or writings, except one "New writing…" item at the end of the format palette. First run asks nothing (maintainer, 2026-11-10): a new install suggests a sensible first format for the kind of app (a word processor, Markdown, LaTeX), and the suggestion becomes the app's format only when the person pastes with it. The first paste in each kind of app says so once ("Pasted in APA · Change"), and Change opens the format chooser.

**The format chooser is a palette** (maintainer, 2026-11-10, option B): the header's format opens one searchable view of the window, shared with More. It lists the way back first when the format isn't yours ("↩ Use APA in Word again", "↩ Stop using Thesis in Word"), then the recent formats, the app's own choices (paste into it from now on, or only copy), writings once one exists and "New writing…", then every format, found by typing; beside the list, the highlighted format's real citation for the chosen references. Esc leaves it unchanged; Enter picks and returns to Search.

**Inside Zotero, Ibis finds instead of citing.** Enter opens the PDF, or shows the item when there's no PDF. Other keys show the item or copy a citation. The footer says which.

**Customisation lives in two places that stay in step.** Settings shows the common choices. `settings.json` holds everything, including shortcuts, writings and formats for each app. An edit to the file applies while Ibis runs, and a file Ibis can't read is reported and never overwritten (0016). The CLI reads the same file.

Not chosen: splitting delivery syntax from style as two visible choices, and making projects mandatory. Both add a concept that most people never need.

2026-11-11: Writings are directly editable objects with a unique `name`, a
`format` (a listed output ID, bundled CSL style ID or output specification) and
an optional `bibliography`.
`writing_apps` selects them by name. Old project IDs, revisions, standalone
project bibliographies and strict-project policies are removed. A proven key
for a different work refuses copying; uncertainty is a warning.

An editor or CLI may explicitly supply a document path or `bibliography` and
`csl`. Scalar Markdown/Quarto front matter in that supplied document can select
a unique matching saved writing; relative bibliography paths resolve beside
it. Ambiguous or unreadable context refuses. The first version accepts bundled
CSL style IDs, including their `.csl` spelling. No title or folder search guesses
a writing.

Settings persistence lives in the engine. Its queue orders outside reports and
saves, reads bounded regular files, tracks only bytes actually written, and
watches both a symlink and its target. Invalid existing files refuse ordinary
writes even with a good backup; restore is explicit. Valid edits apply only
changed preference fields, including shortcut replacement and recording recents.
Watch hints already queued are coalesced without waiting for a quiet period, so
reverting a shortcut does not leave the previous key active during a debounce.
Completed editor writes trigger a read too; Ibis's own reads do not.

After successful paste, a suggestion is remembered only while its captured
engine answer still applies. Its explanation is durable and once per kind.
Zotero's default action asks its existing local API for a PDF attachment and
otherwise shows the item; showing the item explicitly skips that request.
