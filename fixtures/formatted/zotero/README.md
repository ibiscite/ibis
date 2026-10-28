# Zotero formatting goldens

All data is invented. `expected.json` holds four Zotero items (the
`formatted-*` cases of `fixtures/zotero/mapping/cases.json`: the records of
`../source-files` as Zotero stores them) and their expected text, HTML and RTF
for the five bundled styles, citation and bibliography.

The expectations never pass through Ibis's Rust code. `generate.mjs` takes
Zotero's own CSL for each item (pinned `itemToCSLJSON`, from the mapping
corpus) and formats it with the pinned citeproc-js through
`crates/ibis-csl/tests/node-oracle.mjs`, under the same settings as
`../source-files` (locale `en-US`, items in order, no affixes, IEEE as its
reference entry). Regenerate or verify from the repository root:

```sh
node fixtures/zotero/mapping/generate.mjs
node fixtures/formatted/zotero/generate.mjs
node fixtures/formatted/zotero/generate.mjs --check
```

Unlike the `.bib` record, Zotero keeps `de Vries` as one family name, so
citeproc reads `de` as a non-dropping particle: `de Vries, A.` in APA.

`crates/ibis-cli/tests/zotero_formatted_goldens.rs` maps the items with Ibis's
Zotero mapper and replays all ten cases through the engine, the human
`cite --style` command and framed `serve --stdio`; every result must equal
these bytes. It replays them again on an engine that reads the same items from
a fake Zotero local API with `open-zotero`. A test in
`crates/ibis-sources/src/zotero_source.rs` writes the items into a synthetic
Zotero 10 database and checks that the closed reader publishes exactly the
records the local API gives; it runs only in disposable macOS CI, like the
other actual closed-reader test.
