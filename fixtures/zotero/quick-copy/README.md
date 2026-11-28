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
- **The same characters.** `&amp;` and `&#38;`, `&nbsp;` and its character,
  and a line break inside a field, which Ibis writes as a space.

A bibliography's layout, the inline styles that give a pasted bibliography its
line height, hanging indent, spacing and margins, is compared on its own as
`bibliography layout`; for a numeric style, only the block around the entry.
One difference is named there: Word reads no indent from the enclosing block,
so where Zotero puts a hanging indent on the body (`margin-left: 2em`), Ibis
puts it on each entry as the RTF's half inch.

Each style's `differences/STYLE.json` lists every remaining case where Ibis's
output differs, with both texts, and the records Ibis refuses because they have
neither title nor author (`missing_details`), where Zotero prints what little
there is, often only "(n.d.)". The test fails when a list changes; review the
change like a snapshot, then run
`IBIS_BLESS=1 cargo test -p ibis --test zotero_quick_copy`. The whole corpus runs
on Linux, where it takes about a minute.

## What it shows (28 November 2026)

Of 4,394 comparisons, 3,505 match exactly and 668 are refusals of records with
no title or author. The other 221 are:

- **Ibis is right and Zotero is not.** citeproc-js prints the first year range a
  style renders with the date's affixes twice: `(2021–(2023))`. Zotero shows it
  in Nature and GB/T; Ibis fixes it in its bridge. Zotero also fails on a date
  range whose two ends have different precision, where Ibis prints the range.
  Zotero links a URL without a scheme to doi.org; Ibis leaves it as text.
- **Deliberate.** Ibis reads Extra-field dates with Unicode line separators as
  the dates they are; it refuses records a style cannot print
  (`[CSL STYLE ERROR: reference with no printed form.]` in Zotero).
- **To fix.** Rendering text, HTML and RTF from one registration trips a
  citeproc state bug when an author and a translator share a family name and
  initial, so Harvard and GB/T author-date refuse those items.

The first runs also caught the CSL-JSON reader skipping `part-number` and
`license`, quoted family names such as `"de Synthetic"` parsed into a particle
again, URLs and DOIs printed without links, and bibliographies copied without
Zotero's layout. Ibis now sets up citeproc as Zotero does (links on, names
never parsed again) and lays out a copied bibliography as Zotero does, in HTML
and in RTF, where Zotero's own paragraph settings are used.
