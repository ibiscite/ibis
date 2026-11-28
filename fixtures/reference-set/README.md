# Reference set

`references.json` is an invented reference set written by the rules in
[docs/reference-set.md](../../docs/reference-set.md): four records from
`../formatted/source-files/synthetic.json` (a name particle, non-Latin names, a
corporate author and page ranges), as
`ibis cite KEY… --file synthetic.json --format csl-json` writes them.

`crates/ibis-cli/tests/reference_set.rs` requires that the file is in the
layout the spec describes, and that Ibis, reading the set as a library, writes
it again byte for byte.
