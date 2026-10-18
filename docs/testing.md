# Testing strategy

Citation software fails at the edges: unusual names, damaged files, rare item types, other scripts, odd editors. Ibis relies on automated tests, rather than manual review, to find those edges before users do. Layers marked "planned" are tracked in the linked issues and not yet running; all others run in CI on every change.

## Layers

| Layer | What it catches | Where |
| --- | --- | --- |
| Unit tests | Logic errors in one module | `#[cfg(test)]` modules in each crate |
| Specification by example | Exact expected output for each citation syntax and source quirk | `crates/*/tests/*.rs` |
| Fixture corpus with snapshots | Any change in how real-world quirks are read | `fixtures/`, `crates/ibis-core/tests/snapshots/` |
| Property tests | Panics, hangs and broken invariants on generated input | `proptest` in `crates/ibis-core/tests/` |
| Metamorphic tests | Different but equivalent inputs giving different results | planned (#67) |
| Subprocess tests | Process aborts that in-process guards cannot catch (stack overflow) | `crates/ibis-cli/tests/cli.rs` |
| Differential tests | Loss of meaning, measured against an independent implementation | `crates/ibis-cli/tests/fidelity.rs` (Pandoc); planned nightly on larger corpora (#69) |
| Fuzzing | Crashes and runaway work on arbitrary bytes | planned nightly (#66) |
| Open-data corpus | Real names, scripts and records synthetic data misses | planned nightly (#68) |
| Platform harness | Insertion, shortcuts and accessibility on each OS | planned for 0.3 |

## Rules

- **A bug fix starts with a failing test or fixture.** The smallest input that reproduces the problem is added to `fixtures/` or a test, so it can never return unnoticed.
- **Compare with an established implementation where one exists.** Pandoc for BibTeX reading and CSL rendering today; citeproc-js and the CSL test suite for formatting (0.2). Intentional differences are documented next to the normalization that hides them.
- **Never report success for a step that did not run.** `node scripts/check.mjs` exits with status 2 (incomplete) when a required tool is missing.
- **Bounded work on hostile input.** Any loop or recursion driven by input has a limit, and a test proves the limit holds.
- **Synthetic or openly licensed data only.** No personal libraries. Open-data files record their source and licence.

## Running tests

```sh
node scripts/check.mjs                         # everything CI runs on every change
cargo test -p ibis-core                        # fast core tests
IBIS_REQUIRE_PANDOC=1 cargo test -p ibis --test fidelity   # differential tests (needs Pandoc)
cargo insta review                             # review changed snapshots
```
