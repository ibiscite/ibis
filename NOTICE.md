# Notices

Ibis is free software under the GNU Affero General Public License, version 3 or
later (`LICENSE`). Its source is at https://github.com/ibiscite/ibis.

The `ibis` program contains these unmodified third-party files:

- **citeproc-js** by Frank Bennett, Zotero's build from
  https://github.com/zotero/zotero, dual-licensed CPAL 1.0 or AGPL 3.0; Ibis
  uses it under the AGPL.
- **Citation Style Language styles and the en-US locale** from
  https://citationstyles.org/ (https://github.com/citation-style-language/styles
  and https://github.com/citation-style-language/locales), licensed under
  Creative Commons Attribution-ShareAlike 3.0
  (https://creativecommons.org/licenses/by-sa/3.0/). Each style file names its
  authors; the files are in `crates/ibis-csl/vendor/styles` in the source.
- **Zotero's item schema** from https://github.com/zotero/zotero-schema, part
  of the Zotero project.

Exact upstream revisions and hashes are in `crates/ibis-csl/vendor/manifest.json`
and `crates/ibis-sources/vendor/zotero/manifest.json` in the source.
