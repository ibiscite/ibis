# Source-file formatting goldens

All data is invented. `synthetic.bib` and `synthetic.json` were authored by hand
as matching bibliographic inputs. Neither was exported from Ibis. The four
records cover an italic title, a name particle, Japanese names and text, a
corporate author with no date, and an article page range. BibTeX's default
`useprefix=false` is represented by CSL's `dropping-particle` field.

`expected.json` was generated once from the hand-authored CSL-JSON input using
`crates/ibis-csl/tests/node-oracle.mjs` under Node 24.19.0. The oracle returns the
output of a direct, unobserved `CSL.Engine` over Zotero's pinned citeproc-js; it
separately checks that Ibis's JavaScript observer does not change those bytes.
It does not call the Rust source readers or formatter, and no Ibis output or
Ibis-normalized record was used to generate these expectations.

Pins are recorded in `crates/ibis-csl/vendor/manifest.json`:

- Zotero: `db41b99fddcc52d38897c59c161b6adaca036afb`.
- CSL styles: `0e5503bd9a75668c402567a93e20b7327784e4fa`.
- CSL locales: `fa0cf08906a6e714040ef7a2f7999b297b10259a`.

Generation used the five bundled style IDs in their manifest order, each with
normal and bibliography-only requests. Locale was explicitly `en-US` and
`keep_order` was false. Items retained the CSL-JSON fields, with only their
processor IDs replaced by `ibis-0` through `ibis-3`; cites used those IDs in the
file's order with empty affixes and no locator or author suppression. Each
oracle result's text, HTML and RTF was copied unchanged into its matching case.

The oracle's documented standalone configuration disables automatic year
suffixes. IEEE's normal request deliberately uses the reference-entry fallback
because Ibis cannot know a document's citation numbers: the bibliography's
citation-number label and flush alignment are removed. Both IEEE cases thus
have the same reference-entry output, rather than an invented in-text `[1]`.
RTF is wrapped with the same canonical Times New Roman document header used by
the existing pinned oracle. No Zotero process or real profile was used.

Generation checksums:

- Hand-authored CSL-JSON: `dab8df9d79b2a807847fad13f1db613caa9418c2a517306f26f0ee8afbf8566e`.
- Raw oracle response: `f8785d89dbeebce0a7ecb166799200eb5e20d64d331a71c121e5fe6cfe4343a3`.

The CLI integration test starts with each source file, then replays all ten
cases through the engine, human `cite --style` arguments and framed
`serve --stdio`. Every representation must match these independent expected
strings, and the complete canonical result must agree across front ends.
The app replays the five cases the picker can ask for (four styles in their
normal form, IEEE as a reference list) from both files, through search rows,
preview and copy (`engine_file/parity_tests.rs`).
