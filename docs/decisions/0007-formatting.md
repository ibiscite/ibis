# 0007 Formatting

`ibis-csl` runs Zotero's maintained citeproc-js in embedded QuickJS with ICU4X
collation. The processor is pinned by commit and hash. Official CSL styles and
locales ship in a compressed bundle, pinned to upstream releases, byte for byte
under their licences and with their authors. Requests unpack only the named
1 MiB blocks; only the last style remains in memory. A dependent style uses its
parent's rules in its own default language.

Key output comes from `ibis-core`, escaped for each syntax, with typed losses
for details the syntax cannot express. Outside a document, numeric styles give
the bibliography entry without a number, note styles the full first note, and
generated year suffixes require a document-level view. Source-supplied literal
dates and year letters are data; numeric date ordering remains independent of
the rendered letters.

Stored title capitalization is retained. Styles own presentation; APA reports
that titles need sentence case. Conversion warnings use fixed codes and item
indices, carry no field text or paths, and participate in citation fingerprints.
Ordinary direction marks retain their meaning; embedding, override and isolate
controls are neutralized before output.

A style's declared features determine its behavior, never its ID. The output
language is the requested language, otherwise the style's default, independent
of interface and system language. Unknown languages refuse; a bare language
uses the main dialect in upstream `locales.json`.

The citation line uses engine-owned reference spans and display markup. Form
examples use the prepared citation's snapshot without replacing its copy
identity. Late replies cannot publish into a newer edit or picker opening.
Full reference text remains selectable in its menu and tooltip.
