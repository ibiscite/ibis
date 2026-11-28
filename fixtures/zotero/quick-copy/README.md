# Zotero Quick Copy evidence

What Zotero itself copies for every invented item of the mapping corpus
(`../mapping`, 297 items over all 37 item types) in each of Ibis's 13 curated
styles, and how Ibis's output compares. All data is invented.

`expected.json` is Zotero's output, never Ibis's. `generate.mjs` runs Zotero's
own `quickCopy.js`, `style.js` and `cite.js`, unmodified and pinned in
`crates/ibis-sources/vendor/zotero`, over the citeproc-js, styles and locales
Ibis bundles (`crates/ibis-csl/vendor`). For each item it asks for what Zotero
copies with Shift held (a citation) and without (a bibliography), as text and as
HTML. Each item enters as the CSL that Zotero's own `retrieveItem` gave for it,
recorded in the mapping corpus. Zotero's settings come from its code: uppercase
subtitles for APA, linked URLs and DOIs, no name parsing, a style's own default
locale, one cached engine per style. When Zotero fails, the case records its
error. Regenerate or verify from the repository root:

```sh
pnpm -C apps/desktop install   # jsdom, the browser DOM Zotero's code expects
node fixtures/zotero/quick-copy/generate.mjs
node fixtures/zotero/quick-copy/generate.mjs --check
```

The harness stands in only for Zotero's preferences, file access and installed
style list. Zotero.Style's constructor is replaced by reading the same fields
from the XML, because jsdom's XPath cannot read namespaced XML. COinS spans are
left out of HTML bibliographies: they carry no visible text and the harness has
no OpenURL.

## The comparison

`crates/ibis-cli/tests/zotero_quick_copy.rs` reads every item as a Zotero item,
and every item type as a CSL-JSON file exported from Zotero, through the CLI's
own loader: Ibis with no Zotero at all. It asks Ibis for the same citation and
bibliography entry, and compares text and HTML after these named differences:

- **Numbers outside a document.** Ibis copies a numeric style's reference entry,
  without its number, where Zotero copies `[1]`: Ibis cannot know the number.
- **Layout.** Zotero wraps a bibliography in blocks with line height, hanging
  indent and margin styles. The entry's content is compared here; the layout
  belongs to the Word evidence.
- **The same characters.** `&amp;` and `&#38;`, and a line break inside a field,
  which Ibis writes as a space.
- **Not yet: links.** Zotero links URLs and DOIs in HTML; Ibis does not yet.

Each style's `differences/STYLE.json` lists every remaining case where Ibis's
output differs, with both texts, and the records Ibis refuses because they have
neither title nor author (`missing_details`), where Zotero prints what little
there is, often only "(n.d.)". The test fails when a list changes; review the
change like a snapshot, then run
`IBIS_BLESS=1 cargo test -p ibis --test zotero_quick_copy`. The whole corpus runs
on Linux, where it takes about a minute.

## What it shows (28 November 2026)

Of 4,394 comparisons, 3,482 match exactly and 668 are refusals of records with
no title or author. The other 244 are:

- **Ibis is right and Zotero is not.** citeproc-js prints the first year range a
  style renders with the date's affixes twice: `(2021–(2023))`. Zotero shows it
  in Nature and GB/T; Ibis fixes it in its bridge. Zotero also fails on a date
  range whose two ends have different precision, where Ibis prints the range.
- **Deliberate.** Ibis reads Extra-field dates with Unicode line separators as
  the dates they are; it refuses records a style cannot print
  (`[CSL STYLE ERROR: reference with no printed form.]` in Zotero).
- **To fix.** Quoted family names such as `"de Synthetic"` are parsed into a
  particle again because Ibis lets citeproc parse names, so Chicago prints
  `Synthetic, Example de` where Zotero prints `de Synthetic, Example`. Rendering
  text, HTML and RTF from one registration trips a citeproc state bug when an
  author and a translator share a family name and initial, so Harvard and GB/T
  author-date refuse those items. Links, above.

The first run also caught the CSL-JSON reader skipping `part-number` and
`license`, which it now reads.
