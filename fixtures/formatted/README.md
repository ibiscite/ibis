# Formatted engine cases

These invented records use the shared `GoldenCase` schema. They pin the actual
Ibis-safe bundled processor output, including text, HTML, RTF, display runs,
outcomes, notices and content hashes. Backend tests independently compare the
same processor/style configuration with Node.

- `apa-citation.json`: an ordinary author/date citation.
- `apa-bibliography.json`: a bibliography entry with an italic title.
- `ieee-document-context.json`: a normal request falls back to a reference entry
  because document numbering is unavailable; both notices remain visible.

The CLI formatted integration test replays each case through typed Engine,
human style arguments, canonical `cite --request`, `call cite` and framed stdio.
Canonical strings and hashes must remain byte-identical across those paths.

`source-files/` and `zotero/` start from invented `.bib`, CSL-JSON and Zotero
items instead, with expectations from pinned Zotero code; see their READMEs.
