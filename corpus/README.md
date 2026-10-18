# Open-data reader corpus

This engineering-only corpus complements the synthetic `fixtures/` suite. Its
small curated sample covers English, Spanish, Latin, Japanese and Arabic;
literature, drama, history, biology, physics and information theory; books and a
journal article. It is a coverage seed, not a representative statistical sample.
Public metadata is downloaded outside the checkout for nightly/manual testing.
No personal libraries, abstracts, full text, image files or Wikipedia prose are
included in generated test files.

`manifest.json` records the source URL, exact Wikidata entity revision, retrieval
date, canonical-JSON SHA-256, language, type and discipline of each sampled work.
Author and journal labels also have pinned entities in the manifest. All sources
are Wikidata structured entity data, released under
[CC0](https://www.wikidata.org/wiki/Wikidata:Licensing). The
[linked-data API](https://www.wikidata.org/wiki/Wikidata:Data_access#Linked_Data_interface)
supports immutable `revision` URLs without an account or API key. These requests
are test tooling; the application does not gain any network capability.

Normal CI checks provenance and runs synthetic, offline conversion tests. The
nightly/manual workflow verifies every cached digest before reuse, downloads
missing pinned revisions, generates `corpus.bib` (BibLaTeX) and
`expected.csl.json`, and compares the CLI export with that independent projection.
The raw cache lives in the runner's temporary directory and is keyed by the
manifest hash. A failed download, altered revision/digest, missing required field
or changed reader output fails visibly; no live fallback silently changes the
sample. Failure artifacts retain the public corpus and its provenance.

```sh
node --test scripts/corpus.test.mjs
node scripts/corpus.mjs check
node scripts/corpus.mjs fetch /tmp/ibis-corpus
cargo run -p ibis --locked -- csl --all --bib /tmp/ibis-corpus/corpus.bib > /tmp/ibis-corpus/ibis.csl.json
node scripts/corpus.mjs verify /tmp/ibis-corpus
```

The projection includes title in the selected source language, ordered literal
author names, publication-date precision, DOI, journal, volume, issue, pages and
the Wikidata identity URL when available. Labels are selected in the work's
language, then multilingual/default, then English. Types are curated in the
manifest; a dramatic work uses CSL's `book` type. Publication dates absent in the
source remain absent and are listed in `provenance.json`; no date is invented.
This is deliberately a projection of bibliography fields, not a complete
Wikidata-to-CSL converter. Unknown source formats fail rather than being guessed.
Literal backslashes in source fields are currently rejected with a clear error
because [reader issue #90](https://github.com/ibis-cite/ibis/issues/90) would drop
them; the pinned sample contains none. Other TeX metacharacters are escaped.

To refresh or expand the sample, review the structured-data licence again,
retrieve each chosen entity, record its `lastrevid`, and pin that revision URL.
Compute `sha256` with the exported `digest(entity)` function (recursively sorted
object keys, compact JSON; array order retained). Record the actual retrieval
date and any additional author/journal entities, then run a fresh-cache download
and export verification before committing the manifest. Do not switch to live
random sampling: it prevents reproducing old failures. Reproduce reader bugs
with minimal synthetic fixtures in the normal suite before fixing them.
