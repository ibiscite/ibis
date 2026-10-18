# Testing strategy

Citation software fails at the edges: unusual names, damaged files, rare item types, other scripts, odd editors. Ibis relies on automated tests, rather than manual review, to find those edges before users do. Layers marked "planned" are tracked in the linked issues and not yet running; all others run in CI on every change.

## Layers

| Layer | What it catches | Where |
| --- | --- | --- |
| Unit tests | Logic errors in one module | `#[cfg(test)]` modules in each crate |
| Specification by example | Exact expected output for each citation syntax and source quirk | `crates/*/tests/*.rs` |
| Fixture corpus with snapshots | Any change in how real-world quirks are read | `fixtures/`, `crates/ibis-core/tests/snapshots/` |
| File watcher integration | Real platform notifications, interrupted writes, rename replacement, deletion, valid empty files, coalesced change bursts and symlink targets | `crates/ibis-core/tests/source_watch.rs` |
| Property tests | Panics, hangs and broken invariants on generated input | `proptest` in `crates/ibis-core/tests/` |
| Metamorphic tests | Different but equivalent inputs giving different results | `crates/ibis-core/tests/metamorphic.rs`; larger run in `.github/workflows/metamorphic.yml` |
| Subprocess tests | Process aborts that in-process guards cannot catch (stack overflow) | `crates/ibis-cli/tests/cli.rs` |
| Differential tests | Loss of meaning, measured against an independent implementation | `crates/ibis-cli/tests/fidelity.rs` (Pandoc); planned nightly on larger corpora (#69) |
| Fuzzing | Crashes and runaway work on arbitrary bytes | `fuzz/fuzz_targets/`, nightly `.github/workflows/fuzz.yml` |
| Open-data corpus | Real names, scripts and records synthetic data misses | `corpus/`, weekly/manual `.github/workflows/corpus.yml`; offline checks on every change |
| Platform harness | Insertion, shortcuts and accessibility on each OS | planned for 0.3 |

## Rules

- **A bug fix starts with a failing test or fixture.** The smallest input that reproduces the problem is added to `fixtures/` or a test, so it can never return unnoticed.
- **Compare with an established implementation where one exists.** Pandoc for BibTeX reading and CSL rendering today; citeproc-js and the CSL test suite for formatting (0.2). Intentional differences are documented next to the normalization that hides them.
- **Never report success for a step that did not run.** `node scripts/check.mjs` exits with status 2 (incomplete) when a required tool is missing.
- **Bounded work on hostile input.** Any loop or recursion driven by input has a limit, and a test proves the limit holds.
- **Synthetic or openly licensed data only.** No personal libraries. Open-data files record their source and licence.

## Running tests

```sh
node scripts/check.mjs                         # formatting, lints, tests and dependency policy
cargo test -p ibis-core                        # fast core tests
cargo test -p ibis-core --release --locked     # core regressions with release arithmetic
cargo test -p ibis-desktop                     # desktop app tests (not part of plain cargo test)
IBIS_METAMORPHIC_CASES=2048 cargo test -p ibis-core --test metamorphic --locked
IBIS_REQUIRE_PANDOC=1 cargo test -p ibis --test fidelity   # differential tests (needs Pandoc)
cargo insta review                             # review changed snapshots
```

CI tests the core and CLI with the declared Rust 1.90 minimum on Linux. The web interface is checked
and built once on Linux; all three native desktop builds embed that same artifact.

CI also runs the complete core suite in release mode on Linux, covering malformed
dates, private parser-panic containment and the property tests with release arithmetic.

The full check runs the default workspace's `cargo test --locked`, including the
CLI's Pandoc fidelity tests when `pandoc --version` succeeds. Without Pandoc those
comparisons skip; set `IBIS_REQUIRE_PANDOC=1` to require them. Missing optional
Pandoc does not cause the check script's incomplete (exit 2) result. To enforce
fidelity in PowerShell:

```powershell
$env:IBIS_REQUIRE_PANDOC = '1'
node scripts/check.mjs
```

The metamorphic suite generates small synthetic libraries with multilingual
text, inherited metadata and field warnings. Each of seven transformations
runs 64 cases in normal CI and 2,048 weekly (Monday): entry order, field order,
whitespace/CRLF/indentation, equivalent value and entry delimiters, type/field
name case, macro expansion/concatenation, and comments. It compares complete
record identities and normalized fields, plus warning kind/key/message;
verbatim source text, record/warning order and warning line numbers are the
only differences ignored. The generators exclude duplicate keys, duplicate
fields and conflicting macro definitions because their first/last-definition
precedence is meaningful. Quoted values exclude quote/group syntax that would
make delimiters semantically different. Failed weekly runs preserve the
shrunk example and regression seeds as an artifact. Turn reader regressions
into explicit synthetic fixtures or tests before fixing them.

## Picker screenshots

Every CI run builds and launches the real debug app on Windows 2025, macOS 15
and Ubuntu 24.04, including pull requests that change `apps/desktop/`. To capture
another commit, choose **Actions → CI → Run workflow** and select its branch.
No development server or app startup switch is needed: the build embeds the web
interface with `--features tauri/custom-protocol`, and debug windows stay visible
when they lose focus.

Open the run's **Artifacts** and download `picker-<commit SHA>`. The SHA identifies
the checked-out source (the merge commit for a pull request). It contains
`picker-Windows.png`, `picker-macOS.png`, `picker-Linux.png` and startup logs, kept
for 14 days. The three platform shards are combined after the desktop jobs finish;
failed jobs can leave a partial artifact. A process that exits during the startup
wait fails its job; capture errors produce warnings. Check that all three PNGs
exist and actually show the picker before claiming cross-platform evidence.

The app opens the synthetic `fixtures/bibtex/real-world.bib` through an absolute
`IBIS_LIBRARY` path, without saving a library preference. Screenshots show the
initial empty-query view after an eight-second wait: review layout, font rendering,
scaling, library status and window edges by eye. There is no pixel comparison or
automated visual verdict. Linux uses a 1280×800 Xvfb display without a desktop
window manager; its theme and shadows may differ from a user's desktop. These
images do not establish result-list behavior, typing, focus, shortcuts, clipboard
integration or screen-reader accessibility; those still need separate checks.

## Nightly fuzzing

Install `cargo-fuzz` with `cargo install --locked cargo-fuzz --version 0.13.1` and
nightly with `rustup toolchain install nightly --profile minimal`, then run:

```sh
cargo +nightly fuzz run load_str -- -max_total_time=60 -timeout=5 -rss_limit_mb=1024 -max_len=16384
```

Targets are `split_parse`, `load_str` and `csl_export`. The nightly workflow gives
each 300 seconds with a persistent corpus seeded from synthetic fixtures, and
minimizes and reports crashes, timeouts and OOMs as deduplicated `area:testing`
issues. Fixed inputs become fixtures with regression coverage. Fuzz targets share
the root lockfile and are outside default application/test members. Only
cargo-fuzz enables nightly instrumentation; the app toolchain stays pinned. See `fuzz/README.md` for
reproduction, reporting and resource limits.
