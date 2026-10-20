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

The full check includes the default workspace and every desktop crate test.
CI also tests core/CLI with the Rust 1.90 minimum, the core in release mode, and
builds the web interface once for all three native apps. Docs-only PRs report
success for Desktop jobs without building the app; docs-only merges start no
main run. Newer PR/main pushes supersede older CI runs, and only main saves
Rust build caches. Pandoc comparisons skip
when Pandoc is absent; `IBIS_REQUIRE_PANDOC=1` requires them (in PowerShell,
`$env:IBIS_REQUIRE_PANDOC = '1'`). Missing optional Pandoc does not make the full
check incomplete. Review changed snapshots like code.

## Focused harnesses

**Core scale.** Run `cargo test -p ibis-core --release --locked --lib owned_index_100000_reference_measurement -- --ignored --nocapture` for 100,000 synthetic multilingual records, full source/key identities, common/late/absent queries and limits 20/200. This measures core indexing/search, not parsing, IPC, serialization, UI latency or a portable pass threshold. Historical calibration is in [#9](https://github.com/ibiscite/ibis/issues/9#issuecomment-6282578839).

**Metamorphic.** Seven transformations run 64 cases in ordinary CI and 2,048 weekly: entry/field order, whitespace, delimiters, name case, macro expansion and comments. They compare identities, normalized fields and warning kind/key/message, ignoring raw text, order and line numbers. Generators exclude ambiguous duplicates and conflicting macros; failed weekly runs retain shrunk examples/seeds. Turn regressions into fixtures before fixing them.

**Interface accessibility.** Vitest runs axe-core on first-run and loaded states, with a broken-combobox control. JSDOM cannot establish rendered contrast or reflow; contrast has a numeric test, and native focus and screen-reader speech need separate evidence (#8, #79).

**Real Zotero.** `scripts/real-zotero.sh` runs `crates/ibis-zotero-local/tests/real_zotero.rs` against a downloaded Zotero in Linux CI, using a dedicated synthetic profile. It checks actual response shapes, versions and change detection; it never opens a personal library. Transport mocks remain useful for controlled failures.

## Native picker smoke

Build the embedded executable with `--features tauri/custom-protocol`, then run
only in a disposable interactive session. The harness owns the clipboard, opens
a temporary editor and sends real global input. It refuses existing Ibis settings
on macOS/Windows; macOS also refuses existing TextEdit and requires existing
Accessibility/Automation/HID grants. Never bypass permissions or change protected
preference/TCC databases. Windows uses its actual roaming folder, so changing
`APPDATA` does not isolate it; Linux uses a temporary `XDG_CONFIG_HOME`.

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

The native smoke uses UI Automation/SendInput (Windows), System Events/CoreGraphics (macOS) or AT-SPI/xdotool (Linux). It checks window identity, accessibility roles/counts, shortcut/search focus, that the Linux picker asks to stay out of the taskbar and window switcher (`_NET_WM_STATE_SKIP_TASKBAR` and `_NET_WM_STATE_SKIP_PAGER`, read with `xprop`; Windows and macOS have no comparable deterministic check), a second launch, exact synthetic clipboard output from a fresh sentinel (on macOS, Enter is pressed only once the picker is the key window, since the search field's accessible focus does not prove keyboard ownership), dismissal/editor focus return, replacement typing, watcher/reload recovery, damaged libraries and settings preservation. It reads no personal libraries and only cleans up owned files/processes. `smoke.json` and startup logs retain completed assertions and fixed diagnostics, not document text. Exit 1 is failure; exit 2 is unavailable, never a pass. `--self-test` tests the harness only. Linux smoke blocks PRs; macOS/Windows UI smoke warns on PRs and blocks on `main`, where failures open/update an issue. A warning or earlier passed assertion does not make a failed native run pass.

**First-run preferences.** Add `--first-run-preferences` instead of `--budgets` to run only the focused lifecycle check (#486). It starts without a library/gallery override, chooses the native first-run Typst button, and requires saved format-2 preferences before copying. Settled More/Quit must exit zero within one ten-second budget covering menu input and process wait. It then restarts with the same settings and an owned synthetic `IBIS_LIBRARY` solely to supply references; without choosing a format again, native copy must produce exact Typst output from a fresh clipboard sentinel. Before its single Return, one accessibility observation must show the exact query, expected title and one result together. It reads raw clipboard text and rejects a matching value returned after the twenty-second copy-read deadline. Reopening for a second normal Quit requires the foreground picker and named search; AT-SPI may report the active result as focused. The second Quit must pass the same bound. Existing settings and dangling settings links are refused; failures remain failures after owned-resource cleanup. Reports retain only fixed boolean facts from the last observed picker state. Helper subprocess timeouts are capped to the remaining Quit budget where possible; synchronous native calls can return later, which fails the ten-second acceptance oracle rather than establishing a hard checker runtime bound.

This check does not establish native chooser selection, remembered-library relaunch without overrides, pending-save/load Quit, or macOS default-menu Cmd-Q. Those remain open in #238. Its reports/logs use `screenshots/preferences-<OS>` in CI and are uploaded with the existing artifacts; actual success on each native system is required separately from contract tests.

The native click drivers support first-run writing buttons' toggle semantics: macOS `AXCheckBox` with `AXToggle`, and Windows button `TogglePattern`. Exact labels, bounded enumeration and unique actionable controls are required; uncertain activations fail without another input. For macOS More/Quit, the single guarded native Quit activation also establishes control availability; a redundant full-tree read would consume the same ten-second budget before Quit. Fixed role/pattern booleans identify the native path without recording accessible text.

Modeled platform API types must use a private test namespace: shadowing `System.*` names can bind real Windows assemblies. Fresh actual Windows execution remains required.

**Performance.** The native suite measures three fresh processes with 100,000 synthetic references, from launch to accessible count, native query to late result, and current summed process-tree memory. Every sample must fit [smoke-budgets.json](../scripts/smoke-budgets.json); maxima enforce the gate. These include probing/polling overhead, warm OS caches and potentially double-counted shared pages; they exclude reparented helpers, lifetime peak memory and cold-disk claims. No `--budgets` means `measured-without-budget`; record exact source, runner and architecture. Calibration history and qualification limits are in [#9](https://github.com/ibiscite/ibis/issues/9#issuecomment-6282578839).

**Offline probes.** Before launching on Linux/macOS, run `python3 scripts/check-offline.py --record-baseline screenshots/process-baseline.json`, then `python3 scripts/check-offline.py --pid PID --baseline screenshots/process-baseline.json --seconds 60`; Windows uses `scripts/check-offline.ps1`. The probes reject external TCP/UDP endpoints and fail on missing tools, permissions or malformed output. Loopback/unspecified bindings are allowed, private/link-local networks are external. macOS observes newly appearing WebKit helpers but cannot prove ownership of a reparented helper; its traffic fails the session observation. Sampling cannot exclude connections shorter than the polling interval or attribute ownerless closed sockets. A UI pass does not establish offline operation.

**macOS full-screen panel.** Run `python3 scripts/check-panel-macos.py --self-test`, then `python3 scripts/check-panel-macos.py --app target/debug/ibis-desktop --output screenshots/panel-macOS` in the same disposable session. It requires actual AXFullScreen geometry, verifies search, independently primed clipboard, Enter/Escape, editor focus and a real chooser's focus/cancellation. It does not select a file or prove IME, speech, Stage Manager or second-display behaviour. Exit 2 is unavailable. The reproducer, implementation details and earlier failed/partial evidence are preserved in [#198](https://github.com/ibiscite/ibis/issues/198#issuecomment-6282578002).

**macOS appearance.** Run `python3 -B scripts/test_native_appearance_macos.py`, then `python3 scripts/check-native-appearance-macos.py --app target/debug/ibis-desktop --output screenshots/appearance-macOS`. The probe refuses existing app/settings ownership, toggles ordinary System Settings controls, restores original values and records restoration/cleanup. Inspect captures for visible material and legibility; an API-success mode alone does not prove appearance. Unavailable capabilities are not passes. The full procedure, fallback logs and partial evidence remain in [#232](https://github.com/ibiscite/ibis/pull/232#issuecomment-6282577029).

## Picker screenshots

CI launches embedded native debug apps with the synthetic `real-world.bib` and captures the fifteen native gallery states listed in `scripts/capture-gallery.py`; no development server is needed. Download `picker-combined-<commit SHA>-attempt-<run attempt>` from the run's artifacts (retained 14 days), inspect all three platform PNGs and startup logs, and check the manifest/source SHA. Before combination, or if it fails, individual `picker-<commit SHA>-<runner OS>` artifacts may remain; successful combination deletes those shards. Failed jobs may leave partial evidence; screenshots have no automated visual verdict and do not prove typing, focus, copying, speech or unsupported platforms. Gallery capture uses `scripts/capture-gallery.py`; Linux's Xvfb images do not establish a real desktop compositor's appearance.

**Browser contact sheet for visual pull requests.** One machine can show every look: run `node scripts/gallery-contact-sheet.mjs`. It builds the evidence gallery (`vite build --mode evidence` into `apps/desktop/dist`), serves it on 127.0.0.1 only and renders every state × macOS, Windows and Linux looks × light and dark × normal and Larger text in Playwright's Chromium at the picker's 720×480. It writes one PNG per frame, `contact-sheet-normal.png` and `contact-sheet-large.png` (labelled with the commit and browser) and `manifest.json` to `target/gallery-contact-sheet` (`--output` changes it). Narrow it with `--state`, `--platform`, `--theme` or `--text` (each repeatable); `--scale 2` gives sharper images; `--no-build --dist DIR` reuses a build. It needs Playwright with Chromium already installed and never downloads a browser. Exit 0 means every frame acknowledged its render, 1 that a frame did not (a `failure-<state>.png` is kept), 2 that Playwright or Chromium is unavailable. These are browser renders: Chromium's fonts, no vibrancy or Mica and no native window frame, so they show layout and colour roles, not native appearance. Label them "browser render" and keep CI's native captures for the rest.

The same switches work by hand at `/gallery.html` in `pnpm -C apps/desktop dev`: `?platform=macos|windows|linux`, `?theme=light|dark`, `?text=normal|large` (the picker's Larger text setting) and `?density=compact|comfortable|spacious`, with or without `?state=<id>`; the catalog page links each. A state's own setting wins (Dark mode stays dark). In the bundled evidence build opened by hand, the key labels (⌘ or Ctrl) follow the browser's own system, because the picker reads them before the switch runs; the development server and the script both show the labels of the chosen platform. The native evidence app ignores the switches. The browser-only states (`pinned`, `menu-open`, `tray`, `keys`, `compact`, `spacious`, `larger-text`) are reached by the clicks and keys a person would use and are acknowledged only once they are on screen.

## Manual-only checks

The final checklist and coverage gaps live in [#79](https://github.com/ibiscite/ibis/issues/79#issuecomment-6282579792) and the [coverage ledger](https://github.com/ibiscite/ibis/issues/330#issuecomment-6282580538). They cover a person's Zotero, screen-reader speech, keyboard/IME, real-editor paste/full-screen, sleep/wake, displays, synced folders, permission recovery and feel. No hand test is requested until the app is otherwise ready. Hosted Windows Server/macOS/Ubuntu checks do not certify other versions, architectures or Wayland, and a synthetic local-file test does not establish cloud/network-drive recovery.

## Nightly fuzzing

Install `cargo-fuzz` with `cargo install --locked cargo-fuzz --version 0.13.1`
and nightly with `rustup toolchain install nightly --profile minimal`, then run:

```sh
cargo +nightly fuzz run load_str -- -max_total_time=60 -timeout=5 -rss_limit_mb=1024 -max_len=16384
```

The nightly runs `split_parse`, `load_str`, `csl_export` and `cite_render`, giving each 300 seconds with a persistent corpus seeded from synthetic fixtures, minimizing failures and opening deduplicated issues. `zotero_payload` is an additional local target; it is not in the nightly matrix. Fixed inputs become regression fixtures. Fuzz targets share the root lockfile outside default test members; only cargo-fuzz uses nightly instrumentation. See [fuzz/README.md](../fuzz/README.md) for reproduction, reporting and limits.
