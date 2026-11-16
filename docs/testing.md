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
for the changed behaviour; `land.sh` runs fast local checks before requesting
auto-merge on hosted CI.

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
| Picture gate | Every canvas PNG matches the committed visual spec in `apps/desktop/gallery/baselines/`, using pinned Linux WebKit and fonts. | Does not replace visual review or native platform checks. |
| Script tests | Workflow helpers, packages, publication, update feeds, budgets and Markdown links. | Native helpers have separate platform requirements. |

Committed pictures are the visual spec: CI rejects missing/extra pictures, dimension changes, or more than 0.01% differing pixels (pixelmatch threshold 0.1); failures attach actuals and diffs.
To accept a visual change, run `bash scripts/update-picture-baselines.sh RUN_ID` on the failed run, review the PNGs and commit them with the change.
Baselines come only from that pinned CI image, never from a local render; an empty baseline folder fails until seeded this way.

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

[`land.sh`](../scripts/land.sh) runs fast local feedback from a clean, named
branch: formatting, workspace `cargo check` with all targets, TypeScript,
Biome lint and `gen.mjs --check`. While main's latest finished push CI failed,
it refuses any title but `fix` or `revert` and names the red run. Dependencies
must already be installed; Rust uses this worktree's `target` without sccache. These checks are for speed
only ([decision 0049](decisions/0049-local-gate.md)).

After passing, it pushes the checked head, preserves existing PR text or creates
a PR from `--title`/`--body-file` or the first non-merge commit, marks drafts
ready and requests squash auto-merge. Signed-off-by and Assisted-by trailers
carry into the squash commit. There is no merge queue and no up-to-date rule:
GitHub merges as soon as **CI result** passes; the script neither waits for CI
nor reports a status. Keep the worktree until the PR merges, then run
`land.sh end NAME`.

[CI](../.github/workflows/ci.yml) runs on in-repository PRs and pushes to main.
**CI result** is the required merge check: interface build, the `Rust and
desktop` suite, web/script tests, picture comparisons, dependency policy, commit identities and the
Linux release checks (release mode, minimum Rust, budgets) must all pass, so a
release tag never meets a check its commits skipped. Every PR and every push to
main runs `Rust and desktop` on Ubuntu, macOS and Windows, one job per system so
each takes a single hosted runner. Fork PRs skip every job, so their **CI result**
fails; they keep read-only title feedback.

The maintainer's own machines can take the slow or scarce CI jobs as self-hosted
runners: the macOS and Windows `Rust and desktop` jobs and the Linux release checks.
The repository variable `RUNNERS` maps a hosted label to a machine's labels, for
example `{"macos-15":["self-hosted","ibis-mac"]}`. A job whose label is missing
runs on GitHub's hosted runner, so removing a key (or the variable) moves that work
back when a machine is away. Each runner is GitHub's official
[actions/runner](https://github.com/actions/runner), registered to this repository
and run as the maintainer's user (on Windows at logon, not as a service, because the
Notepad paste test needs the desktop). These machines keep state between jobs, so
they only run reviewed code: Dependabot's PRs stay on hosted runners, and Preview's
builds, signing and attestations always run on fresh hosted machines. A fork PR
runs its own copy of the workflows and could target them, so the repository
requires approval for every outside contributor's workflow run; never approve one
while runners are registered.

Daily/manual runs add WebKit, five minutes each of fuzzing the
BibTeX loader and the PDF extractor (`ibis-fuzz`; seeds run with `cargo test -p ibis-fuzz`) and
installed journeys against the published nightly; failures update one issue.
**CI result** also protects publication: the one nightly a day is built from
main's head only once its push CI passed, so a red or unfinished main keeps the
preceding nightly.
Run manual qualification before a stable release.
[Preview](../.github/workflows/preview.yml) keeps full builds, signing and
install/start checks before publishing. Nix and syntax-tool workflows confirm
after merge. RustSec advisories block dependency changes on PRs and main, release
publication and the weekly check; other failures warn. Publication details are
in [RELEASING.md](../RELEASING.md).
