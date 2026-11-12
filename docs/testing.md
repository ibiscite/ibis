# Testing

This page lists checks in this checkout and what they establish. A test
definition does not certify a passing run. Plans live in
[ROADMAP.md](../ROADMAP.md); performance checks and limits are in
[architecture](architecture.md#performance-checks).

## What a test must earn

- Assert a distinct supported outcome or regression a person or client sees.
  The test must fail without the change. Prefer an existing fixture or test
  over a second assertion of the same outcome.
- Protect citation fidelity, parser bounds, public contracts, failed saves,
  cancellation, stale replies, delivery ownership and privacy. Keep native
  tests where the operating system determines the outcome.
- Use invented data, deferred promises and fake clocks. Native waits need
  deadlines. Keep private paths, clipboard text and personal libraries out of
  shared evidence.
- Remove tests for withdrawn behaviour and assertions of source text, CSS
  declarations or incidental call order. Structured architecture/security
  checks and native WebKit colour checks protect separate outcomes.
- Review snapshot changes as code. A consolidation that grows tests needs a
  reason in its pull request.

## Local commands

Setup is in [CONTRIBUTING.md](../CONTRIBUTING.md#setup). Run the focused check
for the changed behaviour; the complete matrix runs in CI.

```sh
node scripts/check.mjs --plan
node scripts/check.mjs
node scripts/check.mjs --all
cargo test -p ibis-engine --locked TEST_FILTER
cargo test -p ibis-desktop --locked TEST_FILTER
pnpm -C apps/desktop exec vitest run FILE --maxWorkers=1
node --test scripts/doc-links.test.mjs
node scripts/gen.mjs --check
```

Plain `cargo test` skips desktop Rust tests. `check.mjs` exits 2 when a tool
is missing. `gen.mjs --check` checks generated schemas, bindings and CLI
options without replacing them. To update a reviewed Rust snapshot, use
`INSTA_UPDATE=always cargo test -p CRATE` and inspect the `.snap` diff.

## What exists

| Layer | What it checks | Limit |
| --- | --- | --- |
| Rust unit, integration and proptest | Parsers, search, citations, settings, source lifecycle and delivery guards. | Simulated decisions do not prove native focus, permissions or insertion. |
| Engine boundary | `engine_boundary.rs` compares Cargo's dependency graph with allowed layers. | A crate boundary check, not a behaviour test. |
| CLI/MCP fixtures | [Canonical cases](../fixtures/cases/README.md), [formatted fixtures](../fixtures/formatted/README.md), schemas and error transcripts. | Covers only the requests each front end can express. |
| Interface tests | Picker, Settings, keyboard, composition, delayed replies and accessibility structure with shared IPC/delivery fakes; the canvas checks portalled citation details and Reader. | jsdom and synthetic events do not exercise native clipboard/input APIs, a real input method or screen reader. |
| Native WebKit canvas | System WebKit colours and measured layout rules on macOS. | Startup pictures are not reviewed visual-state comparisons. |
| Script tests | Workflow helpers, packages, publication, update feeds, budgets and Markdown links. | Native helpers have separate platform requirements. |

The weekly [style workflow](../.github/workflows/styles.yml) renders invented
records against pinned and newest upstream styles, locales and citeproc-js.
Known exceptions are explicit. Preview compares a numbered release with its
predecessor and attaches changed style IDs to the release notes.

## Native journeys

`scripts/smoke-native.py` drives the real desktop app with invented data.
Package install/start and offline observation run separately in Preview.
Hosted runner coverage is listed in [compatibility](compatibility.md#platforms-tested-and-targeted).

- **Linux/X11:** Xvfb/Openbox, one display and the default layout. Checks
  shortcut/focus, search, Pandoc clipboard text, hide/reopen, settings, reload
  and recovery. After Enter copies, it checks the reason, explicitly pastes
  with Shift+Insert into its xterm editor and reads the citation back.
  Require passing `smoke.json`; 100k launch/memory figures are diagnostics.
- **macOS journey:** `bash scripts/hands-on-macos.sh --journey` installs a
  nightly in an isolated profile and invokes the harness. From TextEdit,
  selected text becomes the search; Enter must replace it with the expected
  citation exactly once, verified through Accessibility. A safe copy gets
  one retry. A Terminal collector must receive no keys. Missing terminal
  Accessibility permission or an already open TextEdit makes it unavailable.
  CI runs the debug-build journey too.
- **Windows journey:** `smoke-native.py --journey` uses Notepad and an inert
  console collector. It types the search, checks selection replacement once
  through `WM_GETTEXT`, and checks that the console gets no automatic input.
  `journey.json` records the editor and console host. A locked/disconnected
  desktop is unavailable. `scripts/windows-journey.ps1` supplies native control.

The separate Windows paste primitive test bypasses the picker. Foreground
denial can skip it. macOS/Windows notice text is checked only when native
accessibility exposes it. These journeys do not establish every editor,
keyboard layout, screen reader, cloud provider or Wayland compositor.

## Installed update qualification

`scripts/update-e2e.py` installs the previous nightly, reopens it once,
identifies its owned native offer, activates Install once, verifies package
replacement and a fresh process, and checks persisted library/shortcut
choices. Ambiguous ownership, uncertain input or failed checks terminate the
run without another activation. `--self-test` exercises the harness contract.

Linux targets the verified X11 window and observes GTK focus through
`update-gtk-probe.c`; macOS verifies sheet/button Accessibility identities;
Windows verifies UI Automation ownership and invokes Install or sends one
bounded TaskDialog action. Models cannot prove an installed update.
See [signing](code-signing.md#qualification-before-claiming-permission-survival)
for the separate paste-permission check.

## Update consent keyboard safety

Automatic offers wait for empty search, an idle interval of 1.5 seconds,
no held input, no overlays or copy, and a visible focused picker. Interaction
changes retire the idle ticket; native presentation checks it again.

Later is the safe default. macOS and Windows put it before Install; Linux
explicitly sets its default and initial focus. Only the exact Install action
accepts; Escape/close cancel. Node monitor tests cover Enter, held keys,
composition and stale replies. Rust tests cover busy input and obsolete
tickets. Linux native CI runs
`gtk_offer_return_declines_and_explicit_install_button_accepts` under Xvfb.
The qualifier navigates to a verified Install button before sending Space;
it does not change dialog defaults. Installed keyboard behaviour still needs
platform evidence.

## Native evidence and replay

Each `smoke-native.py` invocation writes
`OUTPUT/evidence-RUN_ID/manifest.json`, including failure and unavailable
runs. Exit 0 means pass, 1 failure, 2 unavailable; unavailable is not coverage.
The [closed schema](../scripts/native_evidence.schema.json) is checked on
write and read. Bundles are limited to one manifest, 1 MiB and 4,000 events,
in a new directory. Other logs are separate diagnostics.

The manifest records build/harness/fixture hashes, observed environment,
fixed action/result codes, relative times and harness completion order.
Unobserved fields have reasons. It records harness order, not app-internal
IPC scheduling, and attaches no screenshot, raw readback or desktop text.

Replay on the same isolated Linux desktop:

```sh
python3 scripts/smoke-native.py --app target/debug/ibis-desktop \
  --output screenshots/replay --replay screenshots/run/evidence-RUN_ID/manifest.json
```

Replay validates build, fixtures and observed environment before acting.
Changed, missing or extra actions and results are `replay-diverged`.
Relative times are diagnostic, not replayed delays. The seed is recorded;
current deterministic scenarios do not consume it.

Use one GUI worker per isolated synthetic desktop. Preserve a failing bundle,
separate product/harness/unavailable failures, replay and minimize it, then
add the regression at the layer that owns the cause. Never export personal
library data or `.local/`. Evidence does not certify unobserved capabilities.

## Where checks run

[`ci-paths.mjs`](../scripts/ci-paths.mjs) selects checks for pull requests and
merge-queue entries. Unknown paths or unreadable change lists select all.
`check.mjs --plan` uses the same classification. `docs/cli.md` selects native
checks because its options are generated; other top-level Markdown is docs-only.
Script tests, workflow lint and dependency policy run for docs changes too.

Licences, bans, sources and commit identities always block; every non-merge
commit needs a sign-off, except those with author email
`347725936+ibis-release[bot]@users.noreply.github.com` and committer
`GitHub <noreply@github.com>`. RustSec advisories block only changes to a
`Cargo.toml`, `Cargo.lock` or `deny.toml`, release tags and the weekly check,
which keeps one issue up to date; other changes get a warning. Ubuntu package
downloads are cached.

[CI](../.github/workflows/ci.yml) reports one required `CI result`. The queue
checks against current main. Passing identical trees can be reused, except
where the queue adds macOS crate/script tests omitted on pull requests.
The commit-identity guard still runs. Main without reusable evidence and the
daily scheduled run use the full matrix; cache-input changes also refresh
main's caches.

[Preview](../.github/workflows/preview.yml) builds, installs and publishes
nightlies for relevant pushes to main. Docs/website-only changes do not build
a nightly. Release checks run beside package builds; numbered publication
requires them, while nightly publication does not wait for them. Release and
update operations are described once in [RELEASING.md](../RELEASING.md).
