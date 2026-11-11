# 0042 Formats, apps and writings

2026-11-10 (maintainer). This replaces the parts of 0031 about formats and writing projects; 0031's limits on what is stored for each app still apply. The implementation follows consolidation step 7's picker session and the 0.4 citation work.

The design passes establish the shared frame now: the format palette, the actions palette and every sheet compose `ui/ViewFrame`, with layout tokens and `ui/` parts owning all spacing ([layout rule](../design.md#how-the-design-changes)). The format and writing model below remains the target for the engine work; the current build still has one usual format and a Writing Projects sheet reached from the palette. It previews the chosen references for key formats and a labelled example for citation styles.

**One concept people see: the format** (APA, MLA, Pandoc keys, `\cite`…). Nobody has to configure more than one format. Ibis decides which format applies, in this order:

1. **A writing, if you chose one for this app.** A writing is optional and opt-in: a name, a format and optionally a `.bib` to check against ("Thesis · Chicago notes"). You create one from the format palette. Ibis never guesses it from a document, a window title or a URL.
2. **This app's format, if you changed the format while in it.** Ibis remembers that without asking. The header says "MLA · for Word", and the palette offers to go back to your usual format.
3. **Your format:** the one you chose in Settings or the format chooser. It applies everywhere else.

Your format applies to apps of its own kind (maintainer, 2026-11-10): a style in word processors and apps Ibis does not know, a key syntax in Markdown, LaTeX and Typst apps. So it is two values, shown in Settings as "In documents: APA" and "In Markdown and LaTeX: Pandoc keys", the second only once it differs from the suggestion or once you have cited in such an app. An app of the other kind gets its kind's suggestion until you change the format there, which sets that app's format.

The header always shows the format in use and, when it isn't your usual one, why ("for Word", "Thesis"). Someone who never changes anything never sees app formats or writings, except one "New writing…" item at the end of the format palette. First run asks nothing (maintainer, 2026-11-10): a new install suggests a sensible first format for the kind of app (a word processor, Markdown, LaTeX), and the suggestion becomes the app's format only when the person pastes with it. The first paste in each kind of app says so once ("Pasted in APA · Change"), and Change opens the format chooser.

**The format chooser is a palette** (maintainer, 2026-11-10, option B): the header's format opens one searchable view of the window, shared with More. It lists the way back first when the format isn't yours ("↩ Use APA in Word again", "↩ Stop using Thesis in Word"), then the recent formats, the app's own choices (paste into it from now on, or only copy), writings once one exists and "New writing…", then every format, found by typing; beside the list, the highlighted format's real citation for the chosen references. Esc leaves it unchanged; Enter picks and returns to Search.

**Inside Zotero, Ibis finds instead of citing.** Enter opens the PDF, or shows the item when there's no PDF. Other keys show the item or copy a citation. The footer says which.

**Customisation lives in two places that stay in step.** Settings shows the common choices. `settings.json` holds everything, including shortcuts, writings and formats for each app. An edit to the file applies while Ibis runs, and a file Ibis can't read is reported and never overwritten (0016). The CLI reads the same file.

Not chosen: splitting delivery syntax from style as two visible choices, and making projects mandatory. Both add a concept that most people never need.
