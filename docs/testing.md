# Testing

This page lists checks in this checkout and what they establish. A test
definition does not certify a passing run. Plans live in
[ROADMAP.md](../ROADMAP.md); performance checks and limits are in
[architecture](architecture.md#performance-checks).

## What a test must earn

- Assert a distinct supported outcome or regression a person or client sees;
  it must fail without the change. Prefer an existing fixture over a second
  assertion of the same outcome.
- Protect citation fidelity, parser bounds, public contracts, failed saves,
  cancellation, stale replies, delivery ownership and privacy. Keep native
  tests where the operating system decides the outcome.
- Use invented data, deferred promises and fake clocks; native waits need
  deadlines. Delete tests for withdrawn behaviour and for source text, CSS
  declarations or call order. Review snapshot changes as code.

## Local commands

Setup is in [CONTRIBUTING.md](../CONTRIBUTING.md#setup). Run the focused check
for the changed behaviour; `land.sh` runs the complete local gate before merging.

```sh
bash scripts/land.sh
cargo test -p ibis-engine --locked TEST_FILTER
cargo test -p ibis-desktop --locked TEST_FILTER
pnpm -C apps/desktop exec vitest run FILE --maxWorkers=1
node --test scripts/doc-links.test.mjs
node scripts/gen.mjs --check
```

Plain `cargo test` skips desktop Rust tests; `cargo test --workspace --locked`
includes them. `gen.mjs --check` checks generated files without replacing them.
To update a reviewed Rust snapshot, use `INSTA_UPDATE=always cargo test -p CRATE`
and inspect the `.snap` diff.

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

## Native journeys and installed updates

Real-app journeys and the installed-update qualification are specified in the
headers of `scripts/smoke-native.py` and `scripts/update-e2e.py`; hosted runner
coverage is in [compatibility](compatibility.md#platforms-tested-and-targeted),
and paste permission survival in [signing](code-signing.md#qualification-before-claiming-permission-survival).

## Update consent keyboard safety

An update offer waits for an idle, empty, focused picker; Later is the default
and only the exact Install action accepts. Node monitor tests cover Enter, held
keys, composition and stale replies; Rust tests cover busy input and obsolete
tickets; Linux CI runs `gtk_offer_return_declines_and_explicit_install_button_accepts`
under Xvfb. Installed keyboard behaviour still needs platform evidence.

## Native evidence and replay

Each `smoke-native.py` invocation writes
`OUTPUT/evidence-RUN_ID/manifest.json`, including failure and unavailable
runs. Exit 0 means pass, 1 failure, 2 unavailable; unavailable is not coverage.
The [closed schema](../scripts/native_evidence.schema.json) is checked on
write and read. Bundles are limited to one manifest, 1 MiB and 4,000 events,
in a new directory. Other logs are separate diagnostics.

The manifest records build, harness and fixture hashes, the observed
environment and fixed action/result codes; unobserved fields have reasons. It
attaches no screenshot, raw readback or desktop text.

Replay on the same isolated Linux desktop:

```sh
python3 scripts/smoke-native.py --app target/debug/ibis-desktop \
  --output screenshots/replay --replay screenshots/run/evidence-RUN_ID/manifest.json
```

Replay validates build, fixtures and environment before acting; changed,
missing or extra actions and results are `replay-diverged`. Use one GUI worker
per synthetic desktop, keep a failing bundle, replay it, then add the regression
at the layer that owns the cause.

## Where checks run

[`land.sh`](../scripts/land.sh) is the merge gate ([decision 0049](decisions/0049-local-gate.md)).
Run it on the Mac from a clean, named branch. It checks for conflicts with main,
pushes without opening a PR, then runs these machines in parallel:

| Machine | Checks |
| --- | --- |
| Mac | Workspace Clippy with all targets (including desktop), workspace tests, TypeScript, Biome, full Vitest, Node script tests (two workers), headless generated-file freshness, PR title, formatting, repository guards and dependency licence/bans/source policy. |
| Ubuntu (`db-scr-ubuntu`) | Workspace Clippy and tests, full Vitest; one build at a time in `~/Code/ibis`. A detached build and SHA-bound end marker survive dropped SSH connections. |
| Windows laptop | When packaging or PowerShell paths change (including the gate itself): packaging-script tests and every `*.tests.ps1` under Windows PowerShell 5.1; no Rust. |

Changes matching the independent-review path table in `land.sh` need an existing
PR with a maintainer-authored comment containing this exact line:
`Reviewed-by-session: SESSION FULL_HEAD_SHA`. A session that did not write the
change reviews the final head, fixes every P1/P2, then posts the record; SESSION
must differ from the writing session's branch name. The gate prints the affected
paths and the command to add a missing record. It checks before running machines
and again before merging, against the current main. New commits need a new review.

Mac and Ubuntu gate tests use the same fixed `PROPTEST_RNG_SEED`, defined in
`land.sh`, and failures print it. To reproduce a property failure, run
`PROPTEST_RNG_SEED=20261114 cargo test -p CRATE --locked TEST_FILTER`.
Proptest 1.12 reads this decimal `u64` from the environment, including with
`Config::with_cases`; no global Cargo configuration fixes seeds for other runs.
Daily/manual CI fuzzing keeps fresh seeds.

Warm dependencies must already be installed on the Mac and Ubuntu. Mac targets
stay in this worktree's `target` (or `CARGO_TARGET_DIR`); Ubuntu reuses its one
`target`. Neither uses sccache. Ubuntu's lock refuses competing gates or a dirty
checkout; Windows uses a locked, reusable checkout in its temporary directory.
Logs stay in `.local/gates/` on the Mac and Ubuntu. Failures print the first
errors per machine and stop. Unreachable machines fail; `--skip mac|ubuntu|windows`
is an explicit exception, recorded in both the PR gate line and status summary.

On success, the script preserves an existing PR's title/body and appends one
gate line. New PRs use `--title` or the first non-merge commit subject, and
`--body-file` or that commit's body without trailers. It posts **Local gate**
success on the exact SHA, then squash-merges immediately without auto-merge.
A changed head, dirty files, missing checks or conflict stop landing. The release
app's PR uses the same gate; no CI dispatch is needed to prepare it.

[CI](../.github/workflows/ci.yml) runs after pushes to main and on daily/manual
runs, with Linux, macOS and Windows suites on every revision. Daily/manual runs
add WebKit, release-mode/MSRV checks, five-minute fuzzing and installed journeys
against the published nightly; failures update one issue. **CI result** remains
publication evidence: a red main keeps the preceding nightly. Run manual
qualification before a stable release. [Preview](../.github/workflows/preview.yml)
keeps full builds, signing and install/start checks before publishing.

Only the read-only title workflow runs on outside PRs. Nix and syntax-tool
workflows confirm after merge. RustSec advisories block dependency changes,
release publication and the weekly check; other failures warn. Publication
details are in [RELEASING.md](../RELEASING.md).
