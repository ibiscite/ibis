Stage A measures real engine paths on deterministic invented data. It changes
no repository production source. `instrument.py` copies the current crates into
`target/stage-a-source`, adds exclusive nested timing spans, and exposes one
explicit-directory shim calling the production closed acquisition and mapping.
Native process/locality checks, private-copy durability, quick_check, parsing,
normalization, fingerprints, indexing, source readiness and citation gates run
unchanged. JSON records the replacements and source/executable hashes.

Run from the worktree, with no other build or benchmark started by this task:

```sh
export CARGO_TARGET_DIR="$PWD/target"
export CARGO_NET_OFFLINE=true
cargo test -p ibis-engine --release --offline --locked --test storage_spike
python3 crates/ibis-engine/tests/storage_spike/protocol_tests.py
python3 crates/ibis-engine/tests/storage_spike.py
```

The runner refuses to overwrite evidence. For an outside-sandbox measurement
using the already generated, hash-checked fixtures:

```sh
CARGO_TARGET_DIR="$PWD/target" CARGO_NET_OFFLINE=true \
  python3 crates/ibis-engine/tests/storage_spike.py \
  --reuse-fixtures --output spike-results/stage-a-outside-sandbox.json
```

Five cyclic Latin-square rounds put every source in every order position once
at each size; the 50k order is mirrored and sizes alternate. First queries by
round are common/CJK/typo/CJK/common. Every process also checks all three queries.
Each size's flat-cache baseline must satisfy max/min <= 1.25 for first results,
verified-ready, peak and settled RSS. All attempts remain in the evidence.
Invalid or environmentally refused runs have no accepted latency distribution.

First-results has both an engine timer and a parent-observed process-start timer.
Verified-ready includes explicit fixture validation and a successful checked key
citation, conservatively adding that validation time. Stage values are exclusive
and do not double count mapping inside parsing or snapshot work inside hydration.
`normalize` means source-to-CSL mapping; search text normalization belongs to
`index`. The residual includes native checks, source-copy validation, schema
bounds, cleanup, publication, source ownership and probe overhead. Cache-hit
`hydrate` includes per-object canonical validation and fingerprint checks; a
second fresh parse precedes citation readiness. Verification stages are retained
separately from first-results stages.

The lean physical schema derives from the closed-reader fixtures. Added indexes
prevent correlated queries from becoming accidental quadratic scans. Full-text
tables are an explicit approximation with a 100k invented-word dictionary,
item-word postings, a composite primary-key index and an item index. Padding
adds actual rows until the DB is at least 600 MB decimal. Notes and attachment
children add real rows, but no attachment files, trash, groups, WAL or BBT are
modeled. Bibliographic fields come from the same generator for BibTeX, closed
SQLite and raw live JSON. Keywords use BibTeX fields versus Zotero tag rows;
Zotero's existing live client omits tags and child payloads from its top-item
read. These assumptions do not establish equivalence to every real library.

The live stub preloads inventory and API response batches outside timing. HTTP
uses the real loopback transport and `live.rs`, with no artificial latency; the
external stub's memory and Zotero's own work are excluded. A refused listener or
closed-reader process check stays a failed run, never an in-memory substitute.

App launch, WebView, cold OS file cache, real Zotero response latency and other
machines are excluded. Fixture generation and hashing warm OS pages. The engine
result cannot alone justify deleting the cache under the slowest-release-machine
rule. No SQLite cache store, fault matrix or production behavior is implemented.
