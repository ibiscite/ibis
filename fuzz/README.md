# Nightly BibTeX fuzzing

This cargo-fuzz workspace member instruments the private lexer (`split_parse`),
the complete resolver/converter (`load_str`) and loaded-reference CSL-JSON
serialization (`csl_export`). `cite_render` renders arbitrary keys and notes in
every `cite::render` syntax and checks that each output escapes user text and keeps
keys exact; its synthetic seeds are in `seeds/cite_render/`. All accept arbitrary
bytes through UTF-8 replacement, as file loading does. It shares the root Cargo.lock and is excluded from default
application/test members. Instrumentation is enabled only by cargo-fuzz with
nightly; the app's pinned stable toolchain is unchanged.

```sh
rustup toolchain install nightly --profile minimal
cargo install --locked cargo-fuzz --version 0.13.1
cargo +nightly fuzz run load_str -- -max_total_time=60 -timeout=5 -rss_limit_mb=1024 -max_len=16384
node --test scripts/fuzz.test.mjs
cargo deny --locked check
```

The scheduled workflow runs each target for 300 seconds with a five-second input
timeout, a 1 GiB RSS limit and a 16 KiB mutation limit. It restores each target's
corpus from the previous run and adds the synthetic `fixtures/bibtex/*.bib` seeds
every time, plus any `fuzz/seeds/TARGET/` files. Cache eviction starts from these fixtures again. Cache keys are unique
per run so newly discovered coverage survives subsequent runs, including failures.

`node scripts/fuzz.mjs TARGET` runs this sequence locally too (five minutes).
Crashes, timeouts and OOMs are reproduced with the same resource limits, then
minimized by deleting bytes while retaining the failure signature. Reduction is
bounded to two minutes; the result need not be globally minimal. Original inputs,
reduced inputs and the original log are retained as failure artifacts. Build errors
and unreproducible findings fail the job instead of silently becoming successes.

In Actions, the script opens an `area:testing` issue with lossless base64 input,
failure details, reproduction command and run link. It scans every page of open
issues for a signature marker before creating one. Signatures combine target,
failure class and relevant stack symbols with addresses and build paths removed;
timeouts and OOMs are grouped by target and resource failure class. Sequential
workflow runs avoid duplicate reporting races. No credentials are needed locally;
without `GITHUB_TOKEN` the script saves findings without reporting them.

When fixing a finding, decode its input into `fixtures/bibtex/` and add a regression
test or fixture snapshot before changing the implementation. Only synthetic
fixtures and generated mutations belong in this corpus; never seed personal
libraries. No application runtime networking is introduced.

The harness follows [cargo-fuzz's shared-workspace workflow](https://github.com/rust-fuzz/cargo-fuzz/tree/0.13.1)
and [libFuzzer's corpus and resource controls](https://www.llvm.org/docs/LibFuzzer.html).
The small deletion reducer checks the original signature on every trial, including
resource failures without sanitizer dedup tokens, with one overall time budget.

`zotero_payload` feeds arbitrary JSON pages directly to the public
`zotero::load_items` mapper for user and group libraries. It checks deterministic
mapping, first-duplicate identity, isolation from malformed neighbors, and CSL
serialization, including rejection of colliding citation IDs. It uses no Zotero
process or network. Run its synthetic seeds with
`cargo test -p ibis-core --test fuzz_zotero_payload`, then mutate them with:

```sh
corpus_dir=$(mktemp -d)
cp fuzz/seeds/zotero_payload/* "$corpus_dir/"
cargo +nightly fuzz run zotero_payload "$corpus_dir" -- -max_total_time=180 -timeout=5 -rss_limit_mb=1024 -max_len=16384
```

Use a copy of the seed directory for sustained runs so generated corpus entries
are not accidentally committed. The nightly owner adds new targets separately.
The crate-specific NCSA policy exception is recorded in ADR 0013; ADR 0014
amends only its workspace layout.

`bibtex_inheritance` loads arbitrary BibLaTeX and inherited hostile field values,
then generates chains of 1–32 helpers using crossref or xdata, cycles, case
variants and conflicting parents. It checks source/key identity, hidden xdata
helpers, own-field and last-parent precedence, exact-case lookup, crossref title
mapping, the 16-level cutoff and cycle/depth diagnostics. Repeated loads must
produce identical references and warnings. LibFuzzer's timeout bounds termination;
the target does not use wall-clock assertions.

```sh
cargo test -p ibis-fuzz --test bibtex_inheritance --locked
corpus_dir=$(mktemp -d)
cp fuzz/seeds/bibtex_inheritance/* "$corpus_dir/"
cargo +nightly fuzz run bibtex_inheritance "$corpus_dir" -- -max_total_time=180 -timeout=5 -rss_limit_mb=1024 -max_len=16384
```

The test replays the synthetic seeds and all generated depth/flag combinations.
CI owners should wire this target into nightly separately; it is local only today.
