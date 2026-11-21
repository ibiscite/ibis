# EndNote

Invented records for the EndNote source adapter (`crates/ibis-source-endnote`).

- `invented.xml`: four records the first slice maps (a journal article with
  Unicode and organisation authors, a second article with the same title, a
  book and a book section) and one it skips (a conference paper). The first
  record also has a Label and a private `file://` link, which are not read.
  Import it into an empty EndNote library to run the trial in
  [the source adapter guide](../../docs/source-adapters.md#endnote).
- `expected.json`: the CSL each kept record becomes, in order, without its
  `id`. `ibis sources check --expect` compares a library with it.

The roles follow EndNote 21.3's own field labels for each reference type: a
book's secondary author is its Series Editor and its `pages` its Number of
Pages; a book section's secondary title is its Book Title.
