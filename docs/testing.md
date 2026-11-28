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
  deadlines. Delete tests for withdrawn behaviour, call order and raw source
  or stylesheet text. Rules checked over parsed stylesheets
  (`apps/desktop/src/design.test.ts` and `ui/ownership.test.ts`) stay: they hold the
  design system's rules ([design](design.md#how-the-design-changes)). Review
  snapshot changes as code.

## Local commands

Everyday commands (setup, focused tests, landing) are in
[development](development.md#one-command-per-job). These are the specialised ones:

```sh
node --test scripts/doc-links.test.mjs
node scripts/gen.mjs --check
node scripts/contract-compat.mjs --base refs/tags/vX.Y.Z
```

Plain `cargo test` skips desktop Rust tests; `cargo test --workspace --locked`
includes them. `gen.mjs --check` checks generated files without replacing them;
For an offline contract report, pass the tag of a published release already
available locally. The separate `Contract report` workflow selects the latest
published release through GitHub. It lists additions, removals and changes
in its job summary ([decision 0028](decisions/0028-engine-first.md)).
To update a reviewed Rust snapshot, use `INSTA_UPDATE=always cargo test -p CRATE`
and inspect the `.snap` diff.

## What exists

| Layer | What it checks | Limit |
| --- | --- | --- |
| Rust unit, integration and proptest | Parsers, search, citations, settings, source lifecycle and delivery guards. | Simulated decisions do not prove native focus, permissions or insertion. |
| Engine boundary | `engine_boundary.rs` compares Cargo's dependency graph with allowed layers. | A crate boundary check, not a behaviour test. |
| CLI/MCP fixtures | [Canonical cases](../fixtures/cases/README.md), [formatted fixtures](../fixtures/formatted/README.md), schemas and error transcripts. | Covers only the requests each front end can express. |
| Zotero evidence | [Zotero's own Quick Copy output](../fixtures/zotero/quick-copy/README.md) for 297 invented items in each curated style, compared with Ibis reading them from Zotero and from an exported CSL-JSON file; every difference is listed per style. | Quick Copy text and HTML only: Word fields, layout and compiled documents are separate evidence. |
| Interface tests | Picker, Settings, keyboard, composition, delayed replies and accessibility structure with shared IPC/delivery fakes; the canvas checks portalled citation details and Reader. | jsdom and synthetic events do not exercise native clipboard/input APIs, a real input method or screen reader. |
| Native WebKit canvas | System WebKit colours and measured layout rules on macOS. | Startup pictures are not reviewed visual-state comparisons. |
| Picture gate | Every canvas PNG matches the committed visual spec in `apps/desktop/gallery/baselines/`, using pinned Linux WebKit and fonts. | Does not replace visual review or native platform checks. |
| Script tests | Workflow helpers, packages, publication, update feeds, budgets and Markdown links. | Native helpers have separate platform requirements. |

The daemon integration tests exercise every generated JSON-RPC method against
the in-process engine and stdio, race CLI and MCP startup, and check build
replacement, crash recovery and settings writes. Source-adapter tests kill a
daemon with a parser and a descendant running. The existing Windows Rust CI
job runs these tests and the named pipe's SID and first-instance tests. Unix
unit tests exercise directory permissions and peer UID comparisons.

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

## Native evidence

Each `smoke-native.py` invocation writes
`OUTPUT/evidence-RUN_ID/manifest.json`, including failure and unavailable
runs. Exit 0 means pass, 1 failure, 2 unavailable; unavailable is not coverage.
The [closed schema](../scripts/native_evidence.schema.json) is checked on
write. Bundles are limited to one manifest, 1 MiB and 4,000 events,
in a new directory. Other logs are separate diagnostics.

The manifest records build, harness and fixture hashes, the observed
environment and fixed action/result codes; unobserved fields have reasons. It
attaches no screenshot, raw readback or desktop text.

Use one GUI worker per synthetic desktop, keep a failing bundle, then add the
regression at the layer that owns the cause.

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
**CI result** is the required merge check. The interface build, web and script
tests, dependency policy and commit identities must pass. When selected, `Rust and desktop` runs
on Ubuntu, macOS and Windows, with one job per system. A newer push to a PR
cancels its superseded run. **Plan the checks** (`scripts/ci-plan.mjs`) skips
the Rust and picture jobs for prose under `docs/` or
the root `*.md` files, the workspace lint and tests for a change to the app alone,
the pictures without an interface change, and [CSL conformance](../crates/ibis-csl/CONFORMANCE.md)
unless the formatter or one of its workspace dependencies changes. Linux release
checks (release mode, minimum Rust, performance) run if a workspace crate can
change. Any other, generated or shared
path (Cargo files, toolchain, `build.rs`, `fixtures/`, `.github/`, `scripts/`,
`locales/`, `website/`, `plugins/`, `brand/`) and anything it cannot read runs the
full suite, as do main, schedules, manual runs, the `ci:full` label (then push
again) and the repository variable `CI_SELECTION=full`. **CI result** checks each
planned job as selected-and-passed or not-selected-and-skipped.
Fork PRs skip every job, so their **CI result** fails; they keep read-only title
feedback.


All three systems run on GitHub's hosted runners
([decision 0049](decisions/0049-local-gate.md)). The repository variable `RUNNERS`
can send the macOS job to the maintainer's Mac as a self-hosted runner instead:
`{"macos-15":["self-hosted","ibis-mac"]}` maps the hosted label to the machine's
labels, and `{}` or no variable keeps it hosted. That Mac serves one job at a time,
so with many open PRs it becomes the queue. Its runner is GitHub's official
[actions/runner](https://github.com/actions/runner), registered to this repository
and run as the maintainer's user with its own `CARGO_HOME` and `RUSTUP_HOME`
(set in the runner's `.env`), and the Rust build cache action runs only on hosted
machines, so jobs never touch the maintainer's toolchain. The runner keeps state
between jobs. Dependabot pull requests always use hosted runners.
Preview's builds, signing and attestations always run on fresh hosted machines.
A fork PR runs its own copy of the workflows and could target it, so the
repository requires approval for every outside contributor's workflow run; never
approve one while the runner is registered. Windows and Linux stay hosted: the Notepad paste test needs an
unlocked desktop and the Linux job installs packages with sudo.

Daily/manual runs add WebKit, two minutes of fuzzing for every parser in the
[threat model](threat-model.md) (`ibis-fuzz`, with the grown corpus cached between
nights; seeds run with `cargo test -p ibis-fuzz`) and
installed journeys against the published nightly. The macOS WebKit job also
runs ten shuffled desktop Vitest passes with two workers and no retries. It
logs each fixed seed and Vitest names any failing tests. Any failed pass fails
qualification. Reproduce a single pass with
`pnpm -C apps/desktop exec vitest run --sequence.shuffle --sequence.seed SEED --maxWorkers=2 --retry=0`
or run the first three seeds with `bash scripts/ci-flake-hunt.sh 3`.

The macOS installed job launches the verified nightly with 10,000 generated
BibTeX references and samples it after four minutes hidden. It sums physical
footprint across the app and its WebKit helpers and reports interrupt wake-ups (`ri_interrupt_wkups`) per
second over the same interval. The step summary and nightly report show both
against [0032](decisions/0032-release-gates.md)'s 200 MiB and 1 wake-up/s budgets.
These measurements are report-only. An over-budget or unavailable sample does
not fail qualification. Hosted runners do not establish shortcut-to-typeable
latency on a real desktop. The combined memory and summon gate waits for
phase 2 of [0065](decisions/0065-one-warm-engine-thin-shells.md).

Qualification failures update one issue.
**CI result** also protects publication: the one nightly a day is built from
main's head only once its push CI passed, so a red or unfinished main keeps the
preceding nightly.
Run manual qualification before a stable release.
[Preview](../.github/workflows/preview.yml) keeps full builds, signing and
install/start checks before publishing. Nix and syntax-tool workflows confirm
after merge. RustSec advisories block dependency changes on PRs and main, release
publication and the weekly check; other failures warn. Publication details are
in [RELEASING.md](../RELEASING.md).
