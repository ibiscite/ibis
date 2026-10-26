# Neutral Zotero mapping differential fixtures

Every item, creator, key and field in `cases.json` is invented. Generation needs
only Node 22+ and the pinned public upstream code in
`crates/ibis-sources/vendor/zotero`; it never launches Zotero or accesses a user
library. Regenerate or verify from the repository root:

```sh
node fixtures/zotero/mapping/generate.mjs
node fixtures/zotero/mapping/generate.mjs --check
```

Each case contains a name, a neutral flat `item` and its `expected` CSL object.
Expected data is the result of the pinned `Zotero.Cite.System.retrieveItem`,
which calls the pinned `Utilities.Item.itemToCSLJSON`. Only `id` is omitted:
Ibis addresses records with `ItemRef`, independent of Zotero database IDs or
export URIs. The normalizer keeps CSL field hacks in the converted Extra/note
field, as Zotero does, for citeproc to consume during formatting.

The 239-case corpus covers all 37 bibliographic item types in schema version 45. Every
valid field is populated, including type-specific base fields; every valid
creator role has personal and institutional examples. Further cases cover
Extra field spelling and date overrides, interruption rules, invalid dates,
quoted dates, EDTF qualifiers and ranges, eras, English and Chinese dates,
name particles and suffixes, quoted family names, single-field creators,
multiple ISBNs, and printed-article URL/access-date suppression with controls.
Adversarial controls include blank Extra values, carriage returns and Unicode
line separators, embedded citeproc fields, type/base-field collisions, particle
punctuation and spacing, ordinal dates, ISBN Unicode boundaries and timestamp
offsets that cross UTC midnight or a year boundary.
Attachments, notes and annotations are deliberately excluded.

The harness stands in for Zotero's field/type/creator registries using the
pinned schema. Native Unicode regular expressions implement the one XRegExp
pattern used by the date parser; padding/whitespace helpers mirror Zotero's
utility helpers. Parsing is fixed to en-US and UTC, independent of the host
locale and time zone. No two-digit-year case depends on the current century.
This corpus checks normalization, rather than formatted citation style output
or Zotero transport and database behavior.
