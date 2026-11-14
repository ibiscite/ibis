# 0042 Formats, apps and writings

The engine resolves formats by app kind and optional writing. Shared palettes
and sheets compose `ui/ViewFrame`; layout tokens and `ui/` own spacing.

**One concept people see: the format** (APA, MLA, Pandoc keys, `\cite`…). Nobody has to configure more than one format. Ibis decides which format applies, in this order:

1. **A writing, if you chose one for this app.** A writing is optional and opt-in: a name, a format and optionally a `.bib` to check against ("Thesis · Chicago notes"). You create one from the format palette. Ibis never guesses it from a document, a window title or a URL.
2. **This app's format, if you changed the format while in it.** Ibis remembers that without asking. The header says "MLA · for Word", and the palette offers to go back to your usual format.
3. **Your format:** the one you chose in Settings for this kind of app. It applies everywhere else.
4. **A suggestion:** used until you choose a format. First run asks nothing.

Your format applies to apps of its own kind: a style in word processors and apps Ibis does not know, a key syntax in Markdown, LaTeX and Typst apps. So it is two values, shown in Settings as "In documents: APA" and "In Markdown and LaTeX: Pandoc keys", the second only once it differs from the suggestion or once you have cited in such an app. An app of the other kind gets its kind's suggestion until you change the format there, which sets that app's format.

The header always shows the format in use and, when it isn't your usual one, why ("for Word", "Thesis"). Someone who never changes anything never sees app formats or writings, except the "New writing…" action in the format palette. First run asks nothing: a new install suggests a sensible first format for the kind of app (a word processor, Markdown, LaTeX), and the suggestion becomes the app's format only when the person pastes with it. The first paste in each kind of app says so once ("Pasted in APA · Change"), and Change opens the format chooser.

**The format chooser is a palette**: one searchable view shared with More.
It offers the way back to the usual format, recent formats, the app's delivery
choice, New writing and the output catalogue. Key formats preview the chosen
references; citation styles show a labelled example. Writings are managed in
their own sheet. Esc leaves the format unchanged; Enter picks and returns to
Search. Real style previews and inline writings are [planned](../../ROADMAP.md#04-citation-and-integration-work).

**Inside Zotero, Ibis finds instead of citing.** Enter opens the PDF, or shows the item when there's no PDF. Other keys show the item or copy a citation. The footer says which.

**Customisation lives in two places that stay in step.** Settings shows the common choices. `settings.json` holds everything, including shortcuts, writings and formats for each app. An edit to the file applies while Ibis runs, and a file Ibis can't read is reported and never overwritten (0016). The CLI reads the same file.

Writings have a unique `name`, a `format` (listed output ID, bundled CSL style
ID or output specification) and an optional `bibliography`. `writing_apps`
selects them by name. A proven key for a different work refuses copying;
uncertainty is a warning.

An editor or CLI may explicitly supply a document path or `bibliography` and
`csl`. Scalar Markdown/Quarto front matter in that supplied document can select
a unique matching saved writing; relative bibliography paths resolve beside
it. Ambiguous or unreadable context refuses. Document context accepts bundled
CSL style IDs, including their `.csl` spelling. No title or folder search guesses
a writing.

Settings ownership and outside edits follow [0040](0040-one-settings-owner.md).

After successful paste, a suggestion is remembered only while its captured
engine answer still applies. Its explanation is durable and once per kind.
Zotero's default action asks its existing local API for a PDF attachment and
otherwise shows the item; showing the item explicitly skips that request.
