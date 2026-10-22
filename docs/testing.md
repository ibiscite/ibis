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
| Real Zotero | Wrong assumptions about what Zotero's local API actually sends (answer shapes, versions, change checks) | `crates/ibis-zotero-local/tests/real_zotero.rs`, run by `scripts/real-zotero.sh` against a freshly downloaded Zotero in CI (Linux) |
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

## Focused harnesses

**Core scale.** `cargo test -p ibis-core --release --locked --lib owned_index_100000_reference_measurement -- --ignored --nocapture` measures indexing and search over 100,000 synthetic multilingual records. It measures the core only, not parsing, the interface or a pass threshold. Calibration is in [#9](https://github.com/ibiscite/ibis/issues/9#issuecomment-6282578839).

**Metamorphic.** Seven transformations (entry and field order, whitespace, delimiters, name case, macro expansion, comments) must not change identities, normalized fields or warnings. CI runs 64 cases; the weekly run 2,048 and keeps shrunk failing examples. Turn a failure into a fixture before fixing it.

**Interface.** `pnpm -C apps/desktop test` runs Vitest. It covers the picker's states, keyboard behaviour, copy timing (late replies never overwrite newer choices), library freshness messages and large selections, and runs axe-core accessibility checks on first-run and loaded states. JSDOM cannot judge rendered contrast, native focus or screen-reader speech; those need native checks (#8, #79).

**Real Zotero.** `scripts/real-zotero.sh` runs `crates/ibis-zotero-local/tests/real_zotero.rs` against a freshly downloaded Zotero in Linux CI, with a synthetic profile. It checks the answers Zotero actually sends and never opens a personal library.

## Native picker smoke

`scripts/smoke-native.py` drives the real app with real input: UI Automation and SendInput on Windows, System Events and CoreGraphics on macOS, AT-SPI and xdotool on Linux. It checks the shortcut, search focus, accessibility roles, exact clipboard output, focus returning to the editor, a second launch, library reload after a change, damaged libraries and settings preservation. On Linux it also checks that the picker stays out of the taskbar and window switcher. It reads no personal library and cleans up only what it created.

After a Linux check fails, one child of the existing harness has four seconds to
record a structural census before fixture/app teardown. It flushes safe process
and X-window facts before querying AT-SPI, retaining validated partial facts on
timeout. Output contains only bounded counts, PIDs, booleans and fixed map-state
enums; unavailable tools and failed/empty queries stay distinct. The original
readiness deadline, failure and teardown are preserved.

Build with `--features tauri/custom-protocol` and run only in a disposable interactive session: the harness takes over the clipboard, opens an editor and sends global input. It refuses to run over existing Ibis settings on macOS and Windows. Never bypass permissions to make it run.

```sh
# Ubuntu/Debian test tools, separate from app dependencies:
sudo apt-get install xvfb xauth xdotool x11-utils xclip xterm openbox python3-pyatspi
xvfb-run -a -s '-screen 0 1280x800x24' dbus-run-session -- \
  /usr/bin/python3 scripts/smoke-native.py \
  --app target/debug/ibis-desktop --output screenshots/smoke-Linux \
  --budgets scripts/smoke-budgets.json

# macOS, in the logged-in disposable runner session:
python3 scripts/smoke-native.py --app target/debug/ibis-desktop \
  --output screenshots/smoke-macOS --budgets scripts/smoke-budgets.json

# Windows, in the disposable runner session (PowerShell):
python scripts/smoke-native.py --app target/debug/ibis-desktop.exe --output screenshots/smoke-Windows --budgets scripts/smoke-budgets.json
```

Exit 0 is a pass, 1 a failure and 2 unavailable, never a pass. `--self-test` tests the harness alone. Linux smoke runs in the required `Desktop (ubuntu-24.04)` check and blocks pull requests. macOS and Windows smoke run in the separate `Native UI (…)` jobs on the app the Desktop job built: they are not required checks, so a failure turns that job red without blocking a merge, and on `main` it also opens or updates an issue.

On Windows, control activation reuses the picker window handle verified by a snapshot and checks its process ownership, existence and visibility again. A modal menu hides Search from accessibility, so opening it must not force the next action to rediscover the window through Search.

The menu focus check proves Escape ownership with Space reopening More. After Tab closes it, a native key-delivered probe must read back exactly in Search, then the original query and results must return before copying. AT-SPI/UIA may report a combobox's active result as focused while the input owns keyboard focus, so a focus flag alone never proves post-Tab typing. The probe never clicks or programmatically focuses Search; it checks the owned app, visible foreground picker and absent menu before delivery and readback. Only fixed Boolean outcomes enter the report.

**First-run preferences.** `--first-run-preferences` (instead of `--budgets`) runs the first-run check (#486): choose a writing format, quit, restart and copy, and the copy must use the saved format without choosing it again. Quit must finish within ten seconds. What it does not cover is listed in #238.

**Performance.** The native suite launches four fresh processes with 100,000 synthetic references and measures launch to accessible count, native query to late result, and current summed process-tree memory. The first launch is a warm-up. The gate in [smoke-budgets.json](../scripts/smoke-budgets.json) uses the median of the three measured launches for both timings and the maximum of all four for memory. Any single launch, the warm-up included, fails if a timing exceeds 1.5 times its budget, so hangs and stalls still fail ([#464](https://github.com/ibiscite/ibis/issues/464)). These include probing/polling overhead, warm OS caches and potentially double-counted shared pages; they exclude reparented helpers, lifetime peak memory and cold-disk claims. No `--budgets` means `measured-without-budget`; record exact source, runner and architecture. Calibration history and qualification limits are in [#9](https://github.com/ibiscite/ibis/issues/9#issuecomment-6282578839).

On Windows, memory sampling pins the root creation identity from the original launch handle, follows only descendants created no earlier than their current parent, and rechecks creation identities while reading working sets and before returning the sum. Missing identities or process churn fail the measurement. Each sample retains at most 64 numeric components (local index, parent index, depth, root-relative age and bytes), plus an older-orphan count; observed PIDs, absolute times, names and paths are omitted. This repairs attribution, without establishing the cause of earlier aggregate-only spikes (#620).

**Offline probes.** `scripts/check-offline.py` (Linux, macOS) and `scripts/check-offline.ps1` (Windows) watch the running app and fail if it opens any connection outside the computer, or if a tool or permission is missing. On Linux and macOS, run `python3 scripts/check-offline.py --record-baseline screenshots/process-baseline.json` before launch, then `python3 scripts/check-offline.py --pid PID --baseline screenshots/process-baseline.json --seconds 60`. Sampling cannot see connections shorter than its polling interval.

**macOS full-screen panel.** `python3 scripts/check-panel-macos.py --app target/debug/ibis-desktop --output screenshots/panel-macOS` (after `--self-test`) checks that the picker opens over a full-screen app, copies, closes and returns focus. Background is in [#198](https://github.com/ibiscite/ibis/issues/198#issuecomment-6282578002).

**macOS appearance.** `python3 scripts/check-native-appearance-macos.py --app target/debug/ibis-desktop --output screenshots/appearance-macOS` (after `python3 -B scripts/test_native_appearance_macos.py`) switches system appearance settings, captures the picker and restores the settings. Look at the captures: the API reporting success does not prove the material is visible. Background is in [#232](https://github.com/ibiscite/ibis/pull/232#issuecomment-6282577029).

## Picker screenshots

**Native captures in CI.** CI launches the real app on each platform with the synthetic `real-world.bib` and captures the gallery states listed in `scripts/capture-gallery.py`. Each state is captured as soon as the app acknowledges rendering it, after a 1 s settle for the native window to paint; a state that is not acknowledged within 23 s fails the capture. Download the `picker-combined-<commit SHA>-attempt-<n>` artifact (kept 14 days) and look at all three platforms. Screenshots have no automatic verdict, and Linux images come from Xvfb, not a real desktop.

**Browser contact sheet.** `node scripts/gallery-contact-sheet.mjs` renders every gallery state in each platform look, light and dark, normal and Larger text, in Playwright's Chromium at the picker's 720×480. It writes PNGs and a `manifest.json` to `target/gallery-contact-sheet`. Narrow it with `--state`, `--platform`, `--theme` or `--text`. Frames are taken with the pointer outside the window, so hover styles show only in the `pin-hover` frame. `high-contrast` and `forced-colors` use Chromium's own contrast and forced-colours emulation; the native evidence app cannot emulate them, so it keeps the `data-gallery-theme="contrast"` CSS. It needs Playwright with Chromium installed and never downloads a browser. Exit 0 means every frame rendered, 1 that one did not, 2 that Chromium is unavailable. These show layout and colour, not native appearance, so label them "browser render". The same switches work by hand at `/gallery.html` under `pnpm -C apps/desktop dev` (`?platform=`, `?theme=`, `?text=`, `?density=`, `?state=`). The `toolbar-menu` states preview the shared menu component (#585) in the browser only.

**Geometry check.** `node scripts/gallery-geometry.mjs` renders the same gallery for each look at normal and Larger text and prints a table of the picker's geometry against [`docs/design-geometry.json`](design-geometry.json): row height per density, footer, search band, the search glyph over the tile column, typed text at the row title's x, and the corner chain. For five states with and without chosen references it also checks that no region of the picker (search band, notices, tray, preview, list, footer) overlaps another, that no chip is cut by the tray's edge, and that the list scrolls in its own area. It takes about half a minute and needs Playwright's Chromium, so it is not part of `scripts/check.mjs`; `scripts/gallery-geometry.test.mjs` (which is) keeps the JSON in step with `docs/design.md`. It reports only (exit 0) until the geometry work in #576 lands; `--enforce` exits 1 on a mismatch. `--no-build --dist DIR` reuses a build. Exit 2 means Playwright or Chromium is unavailable.

Before and after images for a visual pull request belong in that pull request or on the `evidence` branch, not in `docs/`.

## Manual-only checks

The hand checklist lives in [#79](https://github.com/ibiscite/ibis/issues/79#issuecomment-6282579792): a person's Zotero, screen-reader speech, keyboard and input methods, paste into real editors, full screen, sleep and wake, several displays, synced folders and permission recovery. No hand test is requested until the app is otherwise ready. CI runners do not certify other OS versions, architectures or Wayland.

## Nightly fuzzing

Install `cargo-fuzz` with `cargo install --locked cargo-fuzz --version 0.13.1`
and nightly with `rustup toolchain install nightly --profile minimal`, then run:

```sh
cargo +nightly fuzz run load_str -- -max_total_time=60 -timeout=5 -rss_limit_mb=1024 -max_len=16384
```

The nightly run gives `split_parse`, `load_str`, `csl_export` and `cite_render` 300 seconds each with a corpus seeded from synthetic fixtures, and opens an issue for each new failure. Fixed inputs become regression fixtures. See [fuzz/README.md](../fuzz/README.md).
