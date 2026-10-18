# Nightly BibTeX fuzzing

This cargo-fuzz workspace member instruments the private lexer (`split_parse`),
the complete resolver/converter (`load_str`) and loaded-reference CSL-JSON
serialization (`csl_export`). All accept arbitrary bytes through UTF-8 replacement,
as file loading does. It shares the root Cargo.lock and is excluded from default
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
every time. Cache eviction starts from these fixtures again. Cache keys are unique
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
The crate-specific NCSA policy exception is recorded in ADR 0013; ADR 0014
amends only its workspace layout.
