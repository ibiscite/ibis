# Testing

Test the whole task: summon Ibis, find references, choose an output, deliver it
and keep writing. Vary the destination, settings and person using it. Every
failure should leave a replay and a smaller regression.

This inventory describes the checks in this checkout; it does not certify a
passing run. Behaviour comes from [design](design.md), limits from [architecture](architecture.md#desktop-app),
and review rules from [AGENTS.md](../AGENTS.md).

There is no budget for paid services. Default destinations are LibreOffice,
Google Docs with a dedicated free Google account, and Overleaf's free tier or
Community Edition. Native runs use the maintainer's own Mac and Windows PC as
self-hosted machines, plus existing Linux CI. Agent personas use Codex on the
maintainer's Mac within existing access, with no additional model spend.
Office licences, paid accounts, paid runners and paid model/API use are
optional extensions; none blocks the baseline plan or its release checks.
Windows qualification runs on GitHub runners.

## What a test must earn

Maintainer, 2026-11-05, after Sol's review. Tests are kept for what they protect, not counted.

- **Keep:** citation fidelity (golden cases and pinned Zotero comparisons, written independently of the code), parser properties, the public CLI, JSON-RPC and MCP contracts, failed saves that keep the old state, cancellation and late replies, exactly-once delivery, privacy boundaries, a few complete journeys across layers, and native behaviour no simulation can show.
- **Each new test** names a distinct supported outcome or regression a person or client sees, and fails without the change. Prefer adding a golden case to writing a new test.
- **Delete:** tests for withdrawn behaviour, a second test of an outcome already covered, and assertions that pin implementation (source text, CSS declarations, call order). In a mixed file keep the behavioural assertions. One stylesheet check and the native WebKit colour checks stay; architecture and security rules may use structured static checks.
- **No real sleeps** in unit tests: use deferred promises and fake clocks. Native tests may wait with a bounded deadline.
- **Size:** consolidation should shrink test code. A pull request that adds more test lines than product lines says why.

## What exists

- **Rust and proptest:** unit, integration and generated properties check
  parsing, search, citation contracts, settings and delivery guards with
  synthetic inputs. Desktop Rust tests run separately from the default
  workspace tests. These establish the asserted contracts and simulated
  decisions; native permission, focus and insertion still need a desktop.
- **Engine boundary:** `crates/ibis-engine/tests/engine_boundary.rs` reads the
  crate graph from `cargo metadata` and pins which crate may ship with which
  (decision 0028). #1260 removes the front ends' remaining lower crates from it.
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
  persistence. Its 100k launch/memory measurements are diagnostics. After
  Enter copies, it checks the notice's reason, pastes with Shift+Insert into
  the xterm editor as a person would, and reads back exactly the citation.
  Require passed `smoke.json`.
- **Native journey (macOS):** `bash scripts/hands-on-macos.sh --journey` on
  the maintainer's Mac installs the newest nightly, isolates its profile and
  runs `smoke-native.py --journey`: Ibis starts with invented references, the
  shortcut opens it from TextEdit with the selected words as the search,
  Enter pastes, and TextEdit's text, read through Accessibility, must be
  exactly the expected sentence, once; a safe copy instead (document untouched,
  exactly the citation copied) gets one retry. Then a Terminal collector must get no
  keystrokes and the clipboard exactly the key, with the notice on screen.
  The notice's words are checked only when Accessibility exposes them (an
  inactive Ibis often hides that window from it); Linux checks them exactly.
  It fails on a wrong or missing citation and exits 2 when this terminal lacks
  Accessibility access or TextEdit is open. CI runs the same command on the
  debug build (Desktop job, macOS); there Ibis's paste permission is the
  runner's. Ubuntu desktops are checked by hand.
- **Native journey (Windows):** the same `smoke-native.py --journey` on the
  debug build in the Desktop job (windows-2025), or on a Windows PC from its
  signed-in desktop with Notepad closed and no Ibis profile for that build
  (checked on Windows 10 22H2). Ibis reads a selection as the search only on
  macOS, so after the shortcut from Notepad the query is typed; Enter must replace the selected words, and Notepad's text, read with
  `WM_GETTEXT`, must be exactly the sentence with the default Pandoc key, once.
  Then a console (cmd.exe, or the default terminal hosting it) runs an inert
  collector: the clipboard must be exactly the key, the notice on screen and
  the collector must get nothing. Keys go to Ibis only while its picker is the
  foreground window and has the keyboard (UI Automation, through
  `scripts/windows-journey.ps1`); the notice's words are best effort, as on
  macOS. `journey.json` names the Notepad (classic or modern) and the console
  host that ran. It uses PowerShell 7 when installed, else Windows PowerShell,
  exits 2 in a locked or disconnected session (run it from the desktop, not
  over ssh), and removes the Ibis folders it created, so it can run again.
- **Windows paste smoke:** Preview runs a low-level paste check that starts an
  empty Notepad document, sends one Ctrl+V and reads its focused text. This
  bypasses the picker and citation preparation. Explicit OS denial of
  foreground permission can skip it; a green job alone cannot prove insertion.
- **CI artifacts and budgets:** see [where checks run](#where-checks-run). The
  [Preview workflow](../.github/workflows/preview.yml) has startup screenshots,
  offline observation and package checks across the three systems; it skips
  startup capture on Intel macOS. Screenshot failure can be only a warning;
  there are no reviewed visual comparisons. The native resource validator
  exists, but requires measurements from named release machines.
- [Release download counts](../.github/workflows/release-counts.yml) snapshots
  public `stable` asset counters weekly into `stats:counts.csv`: UTC date, asset
  name, `download_count`, asset `updated_at`. These are rough estimates without
  version inference or publication-order checks. Script tests cover raw
  timestamps, no manifest download, formula-name refusal, CSV validation,
  append-only rows, weekly deduplication and API/ref failures and races.

`node scripts/check.mjs` runs what CI would for a branch's changes; setup is in
[CONTRIBUTING.md](../CONTRIBUTING.md). A test definition, screenshot or build
result establishes only what its assertions and retained evidence show.

## Installed update qualification

`scripts/update-e2e.py` requires one single-instance reopen, a real owned native
offer, one activation, package replacement, a fresh process that stays up, and
persisted library/shortcut choices. Read-only observations share the existing
deadlines; ambiguity, changed ownership and uncertain input fail without another
reopen or activation. Helper reports contain fixed stages and counts, never
window text, private paths or native exception text.

What each platform's offer turned out to be, from the update runs of October 2026:

| Platform | What the harness must do |
|---|---|
| Linux | AT-SPI does not expose the native dialog. Qualification uses the smoke harness's exact X11 title, PID, managed-client and native-focus checks, re-enumerates the same window identity and activates the verified Install control in that XID. |
| macOS | `rfd` sets the alert's message text, not a window title, so an anonymous parented sheet is expected. The harness ignores unnamed CG auxiliaries for identity, proves the visible picker separately, searches `AXSheets`, `AXWindows` and the picker's `AXChildren` for the exact heading and button, deduplicates native identities, and rechecks native focus and CF element identity before one AXPress. |
| Windows | TaskDialog actions can be virtual, with no Button HWND. After UIA resolves and rechecks the exact Install label, ownership, ancestry, enabled/visible state and identities, the harness invokes it or sends one bounded `TDM_CLICK_BUTTON` for ID 1008 to the verified foreground dialog. An uncertain send is terminal. Tao's focus cache is cleared when focus moves into WebView2, so the updater compares the foreground HWND with its exact picker HWND. |

No extra Show, hide, explicit update check or invocation retry is used: turning
an automatic check into an explicit one would change the qualified outcome. The
harness never types or seeds a query, and the empty-input and 1.5-second idle
rule stays. A product fix cannot repair an already-built previous nightly, so a
native rerun needs a previous nightly that contains it.

The library check requires exactly one string-valued `bibtex` member naming the
same existing regular file by native file identity, because startup canonicalizes
the saved path (macOS's `/var` alias, Windows's `\\?\` prefix). The shortcut
matches exactly. Other files, missing or unreadable identity, relative paths and
malformed choices fail immediately without raw values in diagnostics.

Run `update-e2e.py --self-test`, the native provider model tests, and focused
workflow/feed tests before a native rerun. The Windows provider model requires
`pwsh`; a skipped provider contract is not Windows evidence. The models prove
refusal, I/O failure, delayed A after B, close/reopen, and one attempted action;
only installed runs establish replacement and restart across the three systems.

## Update consent keyboard safety

Return meant for Search must never accept an automatic update. The locked
sources behave as follows:

| Locked source | Keyboard behaviour and resulting implementation |
|---|---|
| `rfd 0.16.0`, `backend/win_cid/message_dialog.rs` | TaskDialog uses `nDefaultButton: 0`. Standard Yes/No defaults to Yes. Custom actions preserve vector order: custom Yes=1004, No=1008, Cancel=1001. Ibis now puts Later first, Install second and Cancel third. The qualifier invokes Install or sends one bounded `TDM_CLICK_BUTTON` for ID 1008 to its verified owned foreground dialog after rechecking the exact Install label; it never presses the default or requires a physical Button HWND. |
| `rfd 0.16.0`, `backend/macos/message_dialog.rs`; locked AppKit `NSAlert::addButtonWithTitle` documentation | Buttons are added in argument order. The first gets Return; the Cancel title gets Escape. Later is first, Install second, Cancel third. AXPress continues to target only the exact owned Install action. |
| `rfd 0.16.0`, `backend/gtk3/message_dialog.rs` | Adds custom buttons in argument order but never calls `gtk_dialog_set_default_response`. Ibis owns this small GTK dialog instead, explicitly assigning both the default and initial focus to Later. Only Accept installs; Escape/close cancel. The qualifier observes the actual focused Install control, rechecks the same owned, focused XID, then sends one Space. It supports both the previous nightly’s Install-first order and the new Later-first order. |
| `tauri-plugin-dialog 2.8.3`, `desktop.rs` and `MessageDialogBuilder::show` | `show(bool)` treats the first custom label as affirmative. The Linux adapter also maps native cancellation to the custom cancel label, including close. Simply reversing a two-button dialog could therefore map Escape/close to Install. Ibis uses `show_with_result` and matches only the exact Install label; a separate Cancel slot remains safe. |

The shared input monitor captures real keyboard/input/composition events before
picker handlers. One 1.5-second idle timer belongs to its latest interaction;
non-empty input, held keys/pointers, overlays, copies and hidden/unfocused state
block it. The format chooser is tracked separately from More; both its open and close transitions invalidate the old idle ticket. Only the matching native activity ticket permits a fresh check. Native
presentation checks that gate again; the safe default covers IPC/native focus
races. No text or installer metadata is retained in the readiness state.

Node keyboard regressions dispatch Enter through that monitor and a picker
handler, cover held keys, composition, failed I/O and delayed A after B, and run
on both Node 22 and 24. Rust offer tests cover unknown/busy input, typing during
a check, obsolete tickets and fresh metadata after idle. The Linux-only ignored
`gtk_offer_return_declines_and_explicit_install_button_accepts` test proves on
fresh dialogs that Return/default activation cancels, Later stays the default
even when Install is focused, and clicking the native Install button emits
Accept. Linux native CI runs it under Xvfb before the picker smoke. Source
inspection and mocks do not replace installed native keyboard qualification on
each platform.

The Linux qualifier builds `scripts/update-gtk-probe.c` as a private GTK module
and loads it only in its owned test processes. It reads the visible offer's
actual button label, response membership and focus, and publishes only bounded
numeric facts on that exact X11 window; it never changes focus, defaults,
responses or input. Invalid ownership, ambiguous Install controls and exhausted
navigation fail before Space. At most six Tabs navigate to a verified Install;
Space is sent once and never retried, through the XTEST server keyboard path
after proving focus. `update-gtk-probe.test.py` exercises real GTK dialogs for
both button orders and refusal when Install is absent, each `--layout` with its
own Xvfb server and D-Bus session. Missing dependencies, a nonlocal DISPLAY or
an unavailable display fail rather than skip.

## Capability matrix

`tested` requires passing evidence for the stated scope. `limited` means
narrower checks exist. `planned` means the layer below still needs coverage.
No cell is certified as tested by this source inventory.

| Test layer | macOS (own Mac) | Windows (own PC) | Linux (existing CI) |
|---|---|---|---|
| Sequence/race models | limited | limited | limited |
| Native destination readback | limited | limited | limited |
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

Every `scripts/smoke-native.py` entry point writes one
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

Later native and agent adapters extend this evidence with destination, keyboard,
display and input-method details, minimized sequences and readback. An unobserved
field needs a stated reason; the current bundle does not certify any of them.

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
and its exact scope. Fixes to shared behaviour follow the design rules.

Rules for native harnesses, each learned from a false result:

- Settle synthetic native windows before launching Ibis. Accessibility focus does
  not prove keyboard ownership.
- On Linux, bind a unique managed window to its fixed fixture title and PID
  before testing hints; a visible auxiliary of the same process is not enough.
- Control generated sandboxed Mac test apps by verified PID. App lookups can hang
  or relaunch them, so give helper processes deadlines.
- Before a benchmark, declare what makes a baseline valid and use a counterbalanced
  order. Keep every result and reject comparisons with unexplained baseline jumps.

## Where checks run

[CI](../.github/workflows/ci.yml) reports through one required check, `CI result`.
Each change is checked twice at most, on its pull request and in the merge queue,
and each run checks only what its paths can affect
([`ci-paths.mjs`](../scripts/ci-paths.mjs)); an unclear file list runs everything.
The queue classifies its entry against current main. An entry that is exactly its
pull request's head, already on top of main, reuses that pull request's passing
run unless it changes a crate or a script, whose macOS tests pull requests leave
to the queue; main reuses the queue's run of the commit it points at. The commit
identity guard runs in every run, reused or not, because the squash commit's
message is new. Main runs
everything when no passing queue run of its commit exists or when its caches'
inputs (`Cargo.lock`, manifests, toolchain, pnpm lockfile, `ci.yml`) changed, so
that it saves new caches; a daily run on main runs everything, for what changes
with time and so that caches do not expire. A reused run counts only when GitHub
reports a passing run of the identical tree. Release Please's own updates to its
pull request run nothing ([RELEASING.md](../RELEASING.md#how-a-release-is-built)).
`node scripts/check.mjs` makes the same choice locally, and `--plan` prints it for
`scripts/land.sh`, which compiles and type checks what the change selects and
leaves test suites to CI.
Workflow logic that a test covers lives in a script under `scripts/`, and the
test runs that script; no test matches workflow text with regular expressions
(decision 0035). Rules only the structure can show, such as "CI result" running
`always()` and needing every job, are read with a YAML parser
(`scripts/test_workflow_policy.py`). Scripts the workflows run (`ci-*.sh`,
`publish-*.sh`, `keep-previous-nightly.sh`, `update-test-decision.sh`) are
workflow code: changing them needs the same review as the workflow.

| What changed | What runs | Typical wait |
|---|---|---|
| Documentation | Script tests, dependency policy | 1 minute |
| Website | Those, plus type check, lint, interface tests and picker size | 4 minutes |
| Node and shell scripts (not `ci-*` or `check.mjs`) | Those, plus the script tests on Windows (and macOS in the queue), without building the app | 5 minutes |
| Interface | Those, plus colours in the system's own WebKit on macOS | 4 minutes |
| Inside `apps/desktop` | Those, plus the app on Linux, macOS and Windows and the native smoke | 6 minutes |
| Crates, fixtures, Python and CI scripts, workflows, anything else | Those, plus the crates on Linux (with citation fidelity, minimum Rust and the search budget) and Windows | 13 minutes |

The crates' full tests run on macOS only in the merge queue and on main: from 2026-10-25 to 2026-11-02,
in 200 failed runs, macOS failed alone twice, while Windows found real bugs in the
crates. The app's own tests still run on macOS on every native change.

[Preview](../.github/workflows/preview.yml) builds, installs and publishes a nightly
for each push to main, about 15 minutes after it lands; a run keeps only the newest
waiting push. Documentation and website changes build no nightly. Release checks
(release-mode tests, minimum Rust tests and speed limits) run beside the builds: a
failure stops a numbered release and marks the run, but never holds a nightly back.

The plan for each layer as the phases land; placing a layer here does not mean it
is implemented. Keep pull request checks bounded and deterministic. Native nightly
runs use the maintainer's machines when available; otherwise record unavailable
coverage. Do not provision paid replacement runs.

| Layer | PR CI | Nightly | Before release | On demand |
|---|---|---|---|---|
| Rust, properties, CLI/MCP goldens, interface | Relevant contracts/regressions | Release-mode checks | Exact release build | Fixture replay |
| Sequence/race models | Short seeded campaigns | Larger seed/schedule range | Known failures and fixed seeds | Minimized replay |
| Native destination readback | Small stable local-app cases | Rotate apps/settings | Installed build on each OS | Destination failure |
| Visual states | Small state gallery | Native OS variants/diffs | Review baselines | Design failure |
| Screen readers/input methods | Structural/composition regressions | Native journeys | Complete task with experienced users | Speech/input failure |
| Performance/resources | Search/assets, gross regressions | Open/refresh and cycle/idle trends | Named-machine fixed budgets | Targeted measurement |
| Agent exploration | Regressions discovered earlier | Bounded rotating goals | Broader combinations | Issue-specific goal |

Colours need the real engine: [`apps/desktop/gallery/webkit.swift`](../apps/desktop/gallery/webkit.swift)
renders the canvas in the Mac's own WKWebView and checks every accent in System,
Light and Dark. Bugs that passed in jsdom have failed here, so it is required for
every interface change, and `node scripts/check.mjs` runs it on a Mac.

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

1. **Sequence and race models.** Done when picker, Settings and Rust delivery
   models generate actions and reordered completions, assert stale-session,
   selection-order, composition and at-most-once invariants, and save minimized
   regressions; short campaigns run in PR CI and larger ones nightly.
2. **Destination readback.** Done when Linux editor, Windows Notepad and macOS
   TextEdit journeys run through the picker and independently verify exact
   insertion, replacement and refusal, with bundles and visible blocked cases;
   LibreOffice on the maintainer's Mac and Windows PC verifies rich text too.
3. **Visual states.** Done when setup, loading, empty/error/recovery, selection,
   formatting, menus, notices and Settings have a gallery and reviewed native
   comparisons per OS, including large text, contrast and mixed-direction text.
4. **Screen readers and input methods.** Done when VoiceOver and NVDA complete
   search-to-document journeys with checked announcements, real Chinese,
   Japanese and Korean input methods commit without delivery, and an Orca
   pilot either passes or records the compatibility blocker; retain release
   checks with experienced users required by design.
5. **Performance and idle resources.** Done when exact release builds on the
   maintainer's Mac and Windows PC produce evidence accepted by the budget
   validator, with cycle/idle trends; existing Linux CI supplies measurements
   or an explicit coverage blocker. Stop UI polling during idle measurements;
   keep architecture's sampling rules and limits, with no paid runner required.
6. **Bounded agent exploration and free web destinations.** Done when Codex
   personas on the maintainer's Mac run bounded goals with independent checks,
   replay/minimization and deduplicated regression proposals; Google Docs with
   a free Google account and Overleaf free tier or Community Edition each have
   independent readback. Schedule nightly when the Mac/access is available;
   expose gaps without requiring paid models, accounts or Office.

## Computer-use release check

Before each numbered release, a computer-use agent drives the newest nightly on the Mac, the Windows PC and Ubuntu, after the maintainer's own `hands-on-macos.sh` run. Sol writes the plan from the roadmap and `docs/design/`, at most 45 items ordered by risk, each with steps, the outcome a person sees and what counts as a failure. The agent uses only the sample references or the synthetic files that `hands-on-macos.sh` installs, switches away from any real library before a screenshot, restores every setting it changed, and reports each item as pass, fail, blocked or skipped with a screenshot per failure, plus anything else that looks wrong. Every failure becomes a regression test with its fix, or a line in the next milestone's issue; blocked items say what blocked them. The 0.3 run (2026-11-07) found Change Last Citation refusing a selected citation, copies without a reason, unreliable paste into TextEdit and a drag that skipped a refusal.

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
