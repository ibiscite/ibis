# Testing strategy

Citation software fails at the edges: unusual names, damaged files, rare item types, other scripts, odd editors. Ibis relies on automated tests, rather than manual review, to find those edges before users do. The table separates available checks from planned coverage; a configured check is not evidence that a particular run passed.

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
| Real Zotero | Wrong assumptions about what Zotero's local API actually sends (answer shapes, versions, change checks), and a closed-Zotero read that differs from it | `crates/ibis-zotero-local/tests/real_zotero.rs` and `real_zotero_closed.rs`, run by `scripts/real-zotero.sh` against a freshly downloaded Zotero in CI (Linux), which stops Zotero and compares a read of its database with the API's answer |
| Native picker smoke | Real shortcuts, accessibility, clipboard, focus, reloads and 100k measurements | `scripts/smoke-native.py`, platform adapters; requires an interactive runner |

## Rules

- **A bug fix starts with a failing test or fixture.** The smallest input that reproduces the problem is added to `fixtures/` or a test, so it can never return unnoticed.
- **Compare with an established implementation where one exists.** Pandoc for BibTeX reading and CSL rendering today; citeproc-js and the CSL test suite for formatting (0.3). Intentional differences are documented next to the normalization that hides them.
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

`node scripts/check.mjs` runs the default workspace and every desktop crate test. CI also tests the core and command line with the Rust 1.90 minimum and the core in release mode. Pandoc comparisons are skipped when Pandoc is missing; `IBIS_REQUIRE_PANDOC=1` makes them required. Review changed snapshots like code.

Script tests (`*.test.mjs`, `*.test.cjs`, `test_*.py`, `*.test.py`) run with `node scripts/check.mjs --script-tests` once UI dependencies are installed; native CI uses `--platform-script-tests`, which skips only the UI import-boundary suite. `node scripts/style-lint.mjs` fails on a raw colour or px size in desktop CSS outside `tokens.css` (existing ones are allowlisted for #1033). `pnpm -C apps/desktop lint` (in the full check and Web CI) runs Biome with only the React hook rules: complete effect dependencies and hooks at the top level; a deliberate trigger-only dependency list carries a `biome-ignore` comment that says why.

## Focused harnesses

- **Settings migration:** `cargo test -p ibis-desktop identity::tests` covers symlinks, interrupted staging, concurrent processes and look-alike names. Cleanup only removes directories it can prove it owns.
- **Installed updates:** `python3 scripts/update-e2e.py --self-test` checks the update harness without installing. Its native run requires the stored library and shortcut to survive replacement and restart. Windows and macOS runs need a disposable account. It does not prove an identity move, shortcut activation or login startup.
- **Core scale:** `cargo test -p ibis-core --release --locked --lib owned_index_100000_reference_measurement -- --ignored --nocapture` measures indexing and search over 100,000 synthetic multilingual records (core only; calibration in [#9](https://github.com/ibiscite/ibis/issues/9#issuecomment-6282578839)).
- **Metamorphic:** seven transformations (entry and field order, whitespace, delimiters, name case, macro expansion, comments) must not change identities, fields or warnings. CI runs 64 cases and the weekly run 2,048. Turn a failure into a fixture before fixing it.
- **Interface:** `pnpm -C apps/desktop test` runs Vitest over the picker's states, keys, copy timing, freshness messages and large selections, with axe-core on first-run and loaded states. JSDOM cannot judge contrast, native focus or speech (#8, #79).
- **Real Zotero:** `scripts/real-zotero.sh` runs `real_zotero.rs` against a freshly downloaded Zotero in Linux CI with a synthetic profile.

## Native picker smoke

`scripts/smoke-native.py` drives the real app with real input (UI Automation on Windows, System Events on macOS, AT-SPI and xdotool on Linux). It checks the shortcut, search focus, accessibility roles, exact clipboard output, focus returning to the editor, a second launch, library reload, damaged libraries and settings preservation, and on Linux that the picker stays out of the taskbar. It reads no personal library and cleans up only what it created. Probe details live in the script.

Build with `--features tauri/custom-protocol` and run it only in a disposable interactive session: it takes over the clipboard, opens an editor and sends global input. It refuses to run over existing Ibis settings on macOS and Windows. Never bypass permissions to make it run.

```sh
# Linux (test tools: xvfb xauth xdotool x11-utils xclip xterm openbox python3-pyatspi)
xvfb-run -a -s '-screen 0 1280x800x24' dbus-run-session -- \
  /usr/bin/python3 scripts/smoke-native.py \
  --app target/debug/ibis-desktop --output screenshots/smoke-Linux \
  --budgets scripts/smoke-budgets.json
# macOS and Windows: the same arguments in the disposable runner session
# (Windows: python and target/debug/ibis-desktop.exe)
```

Exit 0 is a pass, 1 a failure and 2 unavailable, never a pass; `--self-test` tests the harness alone. Linux smoke runs in the required `Desktop (ubuntu-24.04)` check. macOS and Windows smoke run in the `Native UI (…)` jobs on pull requests and `main` (the merge queue skips them), which are not required: a failure turns the job red without blocking a merge, and on `main` opens or updates an issue.

- **First run:** `--first-run-preferences` (instead of `--budgets`) chooses a writing format, quits, restarts and copies; the copy must use the saved format (#486, gaps in #238).
- **Performance:** four fresh launches with 100,000 synthetic references (the first a warm-up) measure launch to accessible count, query to late result and process-tree memory. The gate in [smoke-budgets.json](../scripts/smoke-budgets.json) uses the median of three launches for timings and the maximum for memory; any launch over 1.5 times a budget fails (#464). The numbers include polling overhead and warm caches and exclude cold-disk and lifetime-peak claims; calibration is in [#9](https://github.com/ibiscite/ibis/issues/9#issuecomment-6282578839). `--skip-performance` skips only these launches, for Nix and installed-package smoke.
- **Offline:** `scripts/check-offline.py` (Linux, macOS) and `scripts/check-offline.ps1` (Windows) fail if the running app connects outside the computer. Sampling cannot see connections shorter than its polling interval.
- **macOS:** `scripts/check-panel-macos.py` checks the picker over a full-screen app (#198); `scripts/check-native-appearance-macos.py` captures the picker under each system appearance, and the captures must be looked at (#232).

## Picker screenshots

- **Native captures:** CI captures the gallery states in `scripts/capture-gallery.py` from the real app on each platform with the synthetic `real-world.bib`. Look at all three in the `picker-combined-<SHA>-attempt-<n>` artifact (14 days). There is no automatic verdict, and Linux images come from Xvfb.
- **Browser contact sheet:** `node scripts/gallery-contact-sheet.mjs` renders every gallery state in each look, theme and text size in Playwright's Chromium at 720×480, into `target/gallery-contact-sheet` (narrow with `--state`, `--platform`, `--theme`, `--text`). It never downloads a browser; exit 2 means Chromium is unavailable. Label these "browser render": they show layout and colour, not native appearance. The same switches work at `/gallery.html` under `pnpm -C apps/desktop dev`. The required Web interface job runs the full sheet on interface changes, main and the merge queue, and a failed frame fails "CI result".
- **Geometry check:** `node scripts/gallery-geometry.mjs` compares the rendered picker with `tokens.css` and [`design-geometry.json`](design-geometry.json), and checks that regions do not overlap, chips are not cut and the list scrolls on its own. It reports only until #576 lands (`--enforce` fails on a mismatch); `scripts/gallery-geometry.test.mjs` keeps it in step with `docs/design.md`.

Before and after images for a visual pull request belong in that pull request or on the `evidence` branch, not in `docs/`.

## Manual-only checks

The hand checklist lives in [#79](https://github.com/ibiscite/ibis/issues/79#issuecomment-6282579792): a person's Zotero, screen-reader speech, keyboard and input methods, paste into real editors, full screen, sleep and wake, several displays, synced folders and permission recovery. No hand test is requested until the app is otherwise ready. CI runners do not certify other OS versions, architectures or Wayland.

## Nightly fuzzing

Install `cargo-fuzz` with `cargo install --locked cargo-fuzz --version 0.13.1`
and nightly with `rustup toolchain install nightly --profile minimal`, then run:

```sh
cargo +nightly fuzz run csl_export -- -max_total_time=60 -timeout=5 -rss_limit_mb=1024 -max_len=16384
```

The nightly run gives `split_parse`, `csl_export` and `cite_render` 300 seconds each with a corpus seeded from synthetic fixtures, and opens an issue for each new failure. Fixed inputs become regression fixtures. See [fuzz/README.md](../fuzz/README.md).
