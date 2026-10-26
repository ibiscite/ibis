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

The 293-case corpus covers all 37 bibliographic item types in schema version 45. Every
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
Malformed timestamp tails retain Zotero's empty-literal result. Creator controls
also exercise all four JavaScript line terminators in family and given names.
Review regressions preserve greedy blank Extra values, literal unsupported era
ranges, unspaced era markers and the pinned JavaScript behavior of uppercase
particles and quoted dates across Unicode line separators.
Attachments, notes and annotations are deliberately excluded.

The harness stands in for Zotero's field/type/creator registries using the
pinned schema. Native Unicode regular expressions implement the one XRegExp
pattern used by the date parser; padding/whitespace helpers mirror Zotero's
utility helpers. Parsing is fixed to en-US and UTC, independent of the host
locale and time zone. No two-digit-year case depends on the current century.
The client ordinal suffix stub is the exact
[`date.daySuffixes = st, nd, rd, th`](https://github.com/zotero/zotero/blob/db41b99fddcc52d38897c59c161b6adaca036afb/chrome/locale/en-US/zotero/zotero.properties#L800)
resource from the same Zotero revision. Its source path, value and file digest
are recorded in the manifest's `oracleProfile`; that profile also selects the
locale, time zone and client mode used by the harness.

Nonzero unpadded two-digit years are clock-dependent in Zotero and are omitted
from the differential corpus. Ibis deliberately keeps those ambiguous dates
literal; this is an explicit profile difference, rather than fixture parity.
The `5/3/00` control is stable because Zotero does not apply century inference to
zero. Schemeless URL controls record source fidelity independently of location
privacy protections.
This corpus checks normalization, rather than formatted citation style output
or Zotero transport and database behavior.
