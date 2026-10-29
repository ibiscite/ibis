# Testing

Test the whole task: summon Ibis, find references, choose an output, deliver it
and keep writing. Vary the destination, settings and person using it. Every
failure should leave a replay and a smaller regression.

The maintainer approved this direction on 2026-10-28. This inventory describes
the checks in this checkout; it does not certify a passing run. Behaviour comes
from [design](design.md), limits from [architecture](architecture.md#desktop-app),
and review rules from [AGENTS.md](../AGENTS.md).

There is no budget for paid services. Default destinations are LibreOffice,
Google Docs with a dedicated free Google account, and Overleaf's free tier or
Community Edition. Native runs use the maintainer's own Mac and Windows PC as
self-hosted machines, plus existing Linux CI. Agent personas use Codex on the
maintainer's Mac within existing access, with no additional model spend.
Office licences, paid accounts, paid runners and paid model/API use are
optional extensions; none blocks the baseline plan or its release checks.

## What exists

- **Rust and proptest:** unit, integration and generated properties check
  parsing, search, citation contracts, settings and delivery guards with
  synthetic inputs. Desktop Rust tests run separately from the default
  workspace tests. These establish the asserted contracts and simulated
  decisions; native permission, focus and insertion still need a desktop.
- **CLI/MCP goldens:** [canonical cases](../fixtures/cases/README.md) and
  [formatted fixtures](../fixtures/formatted/README.md) check text, markup,
  notices, losses and refusals through the engine and CLI. MCP replays the
  subset its tools can express. Agreement covers those cases, not every
  style, language or destination.
- **Interface tests:** picker, Settings and shared controls exercise keys,
  selection, delayed replies, saved choices and accessibility structure in a
  simulated document. The [shared IPC fake](../apps/desktop/src/testing/app.ts)
  keeps preview tickets/content hashes and preference state. The
  [stateful delivery fake](../apps/desktop/src/testing/delivery.ts) models
  clipboard ownership and sequence, one-use proofs, picker presentation and
  replacement of the invented document's selection. Its
  [15 consumer scenarios](../apps/desktop/src/App.delivery.test.tsx) manually
  reorder search, preview, copy, delivery and receipt completions, asserting
  document contents, exactly-once delivery and the production receipt renderer.
  They cover late replies, hide/reopen, clipboard replacement and worker
  failures. These simulate the interface contract; they do not exercise OS
  clipboard/input/permission APIs, native deadlines or engine formatting.
- **Linux native smoke:** [smoke-native.py](../scripts/smoke-native.py) drives
  the embedded app on X11 with one display and the default keyboard layout.
  It checks shortcut/focus, typing, exact Pandoc clipboard text, hide/reopen,
  live settings preservation, file reload and recovery, plus first-run output
  persistence. Its 100k launch/memory measurements are diagnostics. It does
  not read a delivered citation from an editor. Require passed `smoke.json`.
- **Windows paste smoke:** Preview runs a low-level paste check that starts an
  empty Notepad document, sends one Ctrl+V and reads its focused text. This
  bypasses the picker and citation preparation. Explicit OS denial of
  foreground permission can skip it; a green job alone cannot prove insertion.
- **CI artifacts and budgets:** [PR CI](../.github/workflows/ci.yml) runs
  applicable Rust/interface checks and Linux native smoke. The
  [Preview workflow](../.github/workflows/preview.yml) has startup screenshots,
  offline observation and package checks across the three systems; it skips
  startup capture on Intel macOS. Screenshot failure can be only a warning;
  there are no reviewed visual comparisons. Linux release-mode gates enforce
  engine search in PR CI and open/refresh in nightly/release builds; PR builds
  also gate picker asset size. The native resource validator exists, but
  requires measurements from named release machines.

Run commands are in [AGENTS.md](../AGENTS.md); setup is in
[CONTRIBUTING.md](../CONTRIBUTING.md). A test definition, screenshot or build
result establishes only what its assertions and retained evidence show.

## Capability matrix

`tested` requires passing evidence for the stated scope. `limited` means
narrower checks exist. `planned` means the layer below still needs coverage.
No cell is certified as tested by this source inventory.

| Test layer | macOS (own Mac) | Windows (own PC) | Linux (existing CI) |
|---|---|---|---|
| Sequence/race models | limited | limited | limited |
| Native destination readback | planned | limited | planned |
| Visual states | limited | limited | limited |
| Screen readers and input methods | planned | planned | planned |
| Performance and idle resources | limited | limited | limited |
| Agent exploration | planned | planned | planned |

Planned readback starts with free local editors and LibreOffice, then the free
web destinations above. Agent-persona runs start with Codex on the Mac;
Windows/Linux agent control stays planned until free adapters are demonstrated.
These targets change the plan, not the evidence or current coverage status.

Handwritten race tests and the fake are portable, but interface CI runs on
Linux and there is no generated interaction model. Windows readback covers
one native paste primitive. macOS has a hands-on VoiceOver prompt, not retained
evidence of a complete speech journey. Structural accessibility checks and
synthetic composition events do not exercise a screen reader or a real input
method. Startup images do not cover visual states. Resource validation does
not collect native traces. Windows server runners and X11 do not establish
Windows 11 or native GNOME/KDE Wayland coverage.

## Evidence from every native or agent run

Both existing `scripts/smoke-native.py` entry points now write one
`OUTPUT/evidence-RUN_ID/manifest.json` per invocation, including failure,
timeout, interruption and unavailable capability. Exit codes remain 0 (pass),
1 (failure, including evidence I/O) and 2 (unavailable); unavailable never
counts as passing coverage. A synthetic desktop setup assertion is a harness
failure before any app action, never a product failure. Invalid replay input is
refused before a run.

The closed [version-1 schema](../scripts/native_evidence.schema.json) is checked
on write and read, with contract tests in
[test_native_evidence.py](../scripts/test_native_evidence.py) and the native
harness's `--self-test`. The manifest is limited to 1 MiB and 4,000 events.
Each run gets a new directory; existing bundles are never overwritten.
Only this manifest belongs to the exportable bundle. Legacy `smoke.json` and
startup logs remain separate diagnostics; do not include them when sharing a
bundle. No screenshots, raw readback, desktop strings or external paths are
attached by this adapter. Delete run directories with the corresponding test
artifacts under the existing CI artifact retention policy.

The manifest records the artifact SHA-256, source revision and tracked dirty
patch hash, workspace app version, ELF target architecture and inferred
debug/release profile. The harness hash covers its source, settings helper and
schema, and identifies the fixture generator. It hashes the synthetic initial
document/settings and every library fixture used by the selected scenario.
OS version/build, WebKitGTK package version, locale, display count, pixels and
physical dimensions are collected when available. Null build fields mean
`not-observed`; null environment fields are listed in `unavailable` with the
same reason. Keyboard/input method, citation language, display scale, text
size, theme, contrast, motion and font configuration are currently unavailable.
An absent build hash or environment value is never evidence of that coverage.

Input, launch and fixture/settings changes have fixed action codes; native
wait/check requests have source-line invariant IDs tied to the harness hash.
Dispatch and completion/rejection/cancellation events carry immutable request
IDs and relative times, with an explicit completion-order list. This is the
**harness** request order. App IPC completion order is marked `not-observable`;
this adapter cannot reproduce an app-internal race schedule. Results contain
fixed reason codes and a failed invariant (`null` on pass), never exception
text. `--seed` records an unsigned 32-bit seed; `deterministic-scenario` explains
why the current scenarios do not consume it.

Replay on the same isolated Linux desktop with the same artifact and harness:

```sh
/usr/bin/python3 scripts/smoke-native.py --app target/debug/ibis-desktop \
  --output screenshots/replay --replay screenshots/run/evidence-RUN_ID/manifest.json
```

The manifest restores the scenario, channel, shortcut, performance option and
seed. Replay validates the version, closed schema, build, fixtures and observed
environment before native work. It reruns the real scenario and checks every
action before dispatch and every completion against the recorded order;
extra/missing events or a different result produce `replay-diverged`, never
pass. Relative times are diagnostic, not replayed delays. Unsupported/older
formats, malformed JSON and changed inputs receive a fixed refusal message.
Unobserved environment values remain coverage gaps on replay.

Later native/agent adapters extend this evidence to the full contract below;
the current bundle does not certify these unobserved fields or attach the
planned readback, minimized sequences and visual artifacts:

- Run ID, scenario/invariant IDs and result: passed, product failure, harness
  failure or unavailable, with a reason and any missing capabilities.
- Exact revision, dirty-patch hash if any, app version, build/artifact hash,
  target architecture and debug/release profile; harness version too.
- Synthetic fixture hashes and generator version, including initial document
  and settings; destination app, OS/build and native webview versions.
- Locale, citation language, keyboard layout and input method; display count,
  size, scale/DPI, text size, theme, contrast, motion and font configuration.
- Seed and ordered actions with stable IDs and relative times. Record request
  dispatch and completion order, including rejection and cancellation, so a
  race can be replayed independently of wall-clock timing.
- Expected/observed outcome and failed invariant; `null` on success. Give a
  reason for unavailable fields or an unused seed. Attach the replay command,
  minimized sequence and relevant readback, images or numeric measurements.

Use invented libraries, documents, settings and accounts in disposable,
isolated desktop sessions, one GUI worker per desktop. Never run discovery
against a personal library or reuse a person's signed-in profile. Stop if
isolation cannot be established; report unavailable coverage.

Record actions using fixture IDs and relative paths. Keep real document text,
clipboard contents, raw accessibility trees, private paths and credentials out
of logs and published evidence. Screenshots, speech and destination readback
must come only from the isolated synthetic session. Retain structural facts
and exact synthetic comparisons rather than dumping the desktop. Keep native
performance traces local and export only the fixed numeric fields allowed by
[the measurement protocol](architecture.md#native-release-measurements-1369).
Bound artifact size and retention; never publish `.local/`.

## Learn from failures

1. Name the violated invariant and preserve the bundle before cleanup. Classify
   product failure, harness failure and unavailable coverage separately.
2. Replay the same build, fixtures, settings, seed and completion order. An
   agent's claim of completion needs an independent assertion or readback.
3. Minimize actions, data and timing constraints while keeping the invariant
   failing. Keep a seed and explicit events; a seed alone ages poorly.
4. Add the smallest regression at the layer that owns the failure: Rust
   property, CLI/MCP golden, interface sequence, native scenario or visual
   comparison. Make it fail before fixing the cause; keep native coverage when
   the failure depends on the OS. A baseline change needs review against design.
5. Group duplicates by invariant and minimized transition sequence. Link new
   evidence to that issue. Put findings below P2 in the existing 1.0 issue for
   the area. Repeated harness failures need a smaller reproduction, not retries
   until green; follow the CI-round limits in AGENTS.md.

Skipped, blocked and unsupported combinations stay visible in run results and
this matrix, with a reason and follow-up issue. A fallback cannot pass the
capability it replaced. Move a cell to tested only with linked passing evidence
and its exact scope. Fixes to shared behaviour follow the design rules; risky
changes still need an independent review of the final head under AGENTS.md.

## Where checks run

This is the intended schedule as the phases land. Existing jobs are listed
above; placing a layer here does not mean it is implemented. Keep PR checks
bounded and deterministic. Required checks report through `CI result`.
Native nightly runs use the maintainer's machines when available;
otherwise record unavailable coverage. Do not provision paid replacement runs.

| Layer | PR CI | Nightly | Before release | On demand |
|---|---|---|---|---|
| Rust, properties, CLI/MCP goldens, interface | Relevant contracts/regressions | Release-mode checks | Exact release build | Fixture replay |
| Sequence/race models | Short seeded campaigns | Larger seed/schedule range | Known failures and fixed seeds | Minimized replay |
| Native destination readback | Small stable local-app cases | Rotate apps/settings | Installed build on each OS | Destination failure |
| Visual states | Small state gallery | Native OS variants/diffs | Review baselines | Design failure |
| Screen readers/input methods | Structural/composition regressions | Native journeys | Complete task with experienced users | Speech/input failure |
| Performance/resources | Search/assets, gross regressions | Open/refresh and cycle/idle trends | Named-machine fixed budgets | Targeted measurement |
| Agent exploration | Regressions discovered earlier | Bounded rotating goals | Broader combinations | Issue-specific goal |

Native delivery must start with a selected placeholder and surrounding text,
summon Ibis normally, then read the destination independently through saved
content or native text access. Check placement, Unicode, punctuation, selection
replacement, exactly-once insertion and rich-text attributes where applicable.
Current macOS and Windows paths can paste automatically; Linux scenarios must
perform the person's explicit paste after copy. Refused targets, including
terminals, receive no automatic input; test with an inert collector.

Rotate first use, multi-reference work, changing output, missing-library
recovery, keyboard/screen-reader use, CJK/RTL text and non-US layouts. Include
picker and Settings; minimum size, large text, light/dark, contrast, reduced
motion and display scaling. Interrupt with slow/rejected replies, focus or
clipboard changes, destination closure and hide/reopen during formatting.
Agents get a task and constraints; assertions decide success. Keep one worker
per desktop and explicit time/action limits. Codex persona runs on the Mac use
existing access only; record exhausted access as unavailable, without buying
credits or switching to a paid service.

## Phases to file as issues

Work in this order. Introduce destination adapters one app per PR; keep each
slice independently useful. Later phases build on evidence and replay.

1. **Common evidence and replay.** Done when an existing native scenario emits
   the versioned bundle for pass, fail and unavailable outcomes, replays its
   recorded actions, and reports missing coverage without passing it.
2. **Sequence and race models.** Done when picker, Settings and Rust delivery
   models generate actions and reordered completions, assert stale-session,
   selection-order, composition and at-most-once invariants, and save minimized
   regressions; short campaigns run in PR CI and larger ones nightly.
3. **Destination readback.** Done when Linux editor, Windows Notepad and macOS
   TextEdit journeys run through the picker and independently verify exact
   insertion, replacement and refusal, with bundles and visible blocked cases;
   LibreOffice on the maintainer's Mac and Windows PC verifies rich text too.
4. **Visual states.** Done when setup, loading, empty/error/recovery, selection,
   formatting, menus, notices and Settings have a gallery and reviewed native
   comparisons per OS, including large text, contrast and mixed-direction text.
5. **Screen readers and input methods.** Done when VoiceOver and NVDA complete
   search-to-document journeys with checked announcements, real Chinese,
   Japanese and Korean input methods commit without delivery, and an Orca
   pilot either passes or records the compatibility blocker; retain release
   checks with experienced users required by design.
6. **Performance and idle resources.** Done when exact release builds on the
   maintainer's Mac and Windows PC produce evidence accepted by the budget
   validator, with cycle/idle trends; existing Linux CI supplies measurements
   or an explicit coverage blocker. Stop UI polling during idle measurements;
   keep architecture's sampling rules and limits, with no paid runner required.
7. **Bounded agent exploration and free web destinations.** Done when Codex
   personas on the maintainer's Mac run bounded goals with independent checks,
   replay/minimization and deduplicated regression proposals; Google Docs with
   a free Google account and Overleaf free tier or Community Edition each have
   independent readback. Schedule nightly when the Mac/access is available;
   expose gaps without requiring paid models, accounts or Office.

## What needs the maintainer

- Register/configure the maintainer's Mac and Windows PC as self-hosted machines;
  provide isolated test users/sessions, availability windows and cleanup.
  Pilot permission persistence and native accessibility exposure.
- Mac permission grants and the stable test signing identity. Verify access
  after updates; [signing setup](code-signing.md) governs the identity.
- Make LibreOffice available in the isolated sessions. Word/Office coverage
  is optional and never needs a licence purchase to complete the plan.
- Provide a dedicated free Google test account and an Overleaf free-tier test
  account or Community Edition instance; set access rules for synthetic data
  and any new harness network use.
- Make existing Codex access on the Mac available for persona runs, with
  time/action limits. No paid model budget or API key is required.

Keep cloud access and inference in the test harness. This plan adds no network
behaviour to Ibis. Record unavailable baseline capabilities in the matrix and
results until exercised. Paid extensions need separate approval if ever
requested; their absence never blocks a phase, baseline check or release.
