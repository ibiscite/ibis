# Citation engine, styles and locale

Unmodified third-party files used for formatted citations (ADR 0007). Each is pinned by upstream commit and SHA-256 in `manifest.json`; `scripts/vendor-citeproc.mjs` downloads them and checks the hashes, and a test checks them on every run. Ibis never downloads these at run time.

- `citeproc.mjs`: Zotero's build of citeproc-js by Frank Bennett, from [zotero/zotero](https://github.com/zotero/zotero), dual-licensed CPAL 1.0 or AGPL 3.0 (Ibis uses it under the AGPL). The only change is the appended `export default CSL;` line that makes it an ES module.
- `styles/*.csl`: styles from the [Citation Style Language](https://citationstyles.org/) project ([citation-style-language/styles](https://github.com/citation-style-language/styles)), licensed CC BY-SA 3.0; authors are listed in each file.
- `locales/en-US.xml`: from [citation-style-language/locales](https://github.com/citation-style-language/locales), licensed CC BY-SA 3.0.

To move a pin, change its commit in `manifest.json` and run `node scripts/vendor-citeproc.mjs --update`, then review the output changes in the tests like snapshots.
