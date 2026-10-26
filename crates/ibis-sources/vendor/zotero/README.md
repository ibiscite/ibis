# Pinned Zotero mapping oracle

`manifest.json` records upstream revisions, paths and SHA-256 hashes for the
unmodified files here. The schema and utilities revisions are the submodule
revisions of the existing Ibis Zotero pin,
[`db41b99fddcc52d38897c59c161b6adaca036afb`](https://github.com/zotero/zotero/tree/db41b99fddcc52d38897c59c161b6adaca036afb).
The Rust mapper embeds `schema.json` as data. JavaScript files run only when
regenerating the synthetic differential fixtures; Ibis never executes them or
fetches upstream data at runtime.

- `schema.json` comes from [zotero/zotero-schema](https://github.com/zotero/zotero-schema/tree/b86c79b56479cadac3288e1f122cf34ef04e8809).
- `schema.js`, `date.js`, `dateFormats.json`, `utilities_item.js` and `COPYING`
  come from [zotero/utilities](https://github.com/zotero/utilities/tree/4051881d59c6c12d34417d4c226f37218051be90).
  Copyright and licence notices remain in the source files. Their Zotero code
  is AGPL-3.0-or-later; `COPYING` is the unmodified upstream licence.
  The particle parser also carries Frank Bennett's copyright and dual licence;
  Ibis uses the AGPL option.
- `cite.js` comes from [zotero/zotero](https://github.com/zotero/zotero/blob/db41b99fddcc52d38897c59c161b6adaca036afb/chrome/content/zotero/xpcom/cite.js),
  part of Zotero under AGPL-3.0-or-later. It supplies the citation-system
  conversion, including URL/access-date suppression for printed articles.

`fixtures/zotero/mapping/generate.mjs` verifies every hash before invoking the
pinned conversion code. To change a pin, replace these files from their exact
upstream revisions, record the new hashes, regenerate the fixtures and review
both source and expected-output differences.
