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

2026-10-30: Run current Windows qualification on GitHub runners without waiting
for a local Windows Ibis project; existing release gates remain.

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
- [Release download counts](../.github/workflows/release-counts.yml) snapshots
  public `stable` asset counters weekly into `stats:counts.csv`: UTC date, asset
  name, `download_count`, asset `updated_at`. These are rough estimates without
  version inference or publication-order checks. Script tests cover raw
  timestamps, no manifest download, formula-name refusal, CSV validation,
  append-only rows, weekly deduplication and API/ref failures and races.

Run commands are in [AGENTS.md](../AGENTS.md); setup is in
[CONTRIBUTING.md](../CONTRIBUTING.md). A test definition, screenshot or build
result establishes only what its assertions and retained evidence show.

## Installed update qualification

`scripts/update-e2e.py` requires one single-instance reopen, a real owned native
offer, one activation, package replacement, a fresh process that stays up, and
persisted library/shortcut choices. Read-only observations share the existing
deadlines; ambiguity, changed ownership and uncertain input fail without another
reopen or activation. Helper reports contain fixed stages and counts, never
window text, private paths or native exception text.

Run 41130217340 used the merged observation retries. Its supplied `update4.log`
establishes:

| Platform | Evidence and cause |
|---|---|
| Linux | Lines 1006–1030 prove one visible, focused native offer from 31.985 through 184.968 seconds. Lines 1036–1045 report a complete fresh AT-SPI scan with zero offer/action matches. Presentation works; this accessibility provider does not expose the native dialog. More cache invalidation or retries cannot establish an action that is absent from that tree. Qualification now uses the smoke harness's exact X11 title, PID, managed-client and native-focus checks, re-enumerates the same window identity, and deliberately activates the Install control in that XID (see the keyboard-safety update below). Replacement and restart must still follow. |
| macOS | Lines 383–398 show `cg-window-title` failing persistently from 33.441 through 184.547 seconds; `code: null` comes from the harness's own missing-name assertion. Two CG windows are counted, but the picker/offer counts were assigned only after the loop, so zero counts do **not** prove either window disappeared. AX still sees one window without errors (426–434). The product parents its offer; locked `rfd 0.16.0` uses `beginSheetModalForWindow` and sets the alert's **message text**, not its window title. An anonymous sheet is therefore expected. The harness now ignores unnamed CG auxiliaries for identity, proves the visible picker separately, and enumerates owned AX sheets and separate windows by exact alert heading and button. It rechecks native focus and CF element identity before one AXPress. The original log cannot identify which of the two CG windows lacked a name. |
| Windows | Lines 180–190 report seven owned native windows, **zero** offer captions, and one visible foreground picker, without census errors. This is a genuinely absent offer, not UIA hiding an existing native dialog. Lines 212–216 record successful availability. The product's remaining automatic presentation conditions are visibility, focus, the still-enabled choice, and a free chooser lease. The seeded choice stays enabled and no harness action acquires a chooser. The locked runtime reveals a focus defect: single-webview `IsFocused` reads Tao's top-level `is_active && is_focused` cache, which `WM_KILLFOCUS` clears when focus moves into WebView2; WebView2's separate focus events do not repair that cache. The harness proves exactly that foreground-with-focused-Search state. The updater now compares the foreground HWND with its exact picker HWND. The old log has no internal focus/lease sample; this source-based diagnosis still needs a rebuilt Windows nightly to verify. |

The supplied `update2.log` for run 41107965372 contains only Windows and Linux
jobs (446 and 778 lines). It supplies no passing macOS observation or action
record, so a passing-Mac comparison cannot be reconstructed from that file.

No extra Show, hide, explicit update check, or invocation retry is used. The
Windows product focus fix cannot repair the already-built previous nightly;
a native rerun must use a previous nightly containing that fix. Turning an
automatic check into an explicit one would change the qualified outcome.

Run `update-e2e.py --self-test`, the native provider model tests, and focused
workflow/feed tests before a native rerun. The Windows provider model requires
`pwsh`; a skipped provider contract is not Windows evidence. The models prove
refusal, I/O failure, delayed A after B, close/reopen, and one attempted action;
only installed runs establish replacement/restart across the three systems.

Local worker validation on 2026-10-30: 64 qualifier self-tests passed (the
PowerShell provider test skipped because `pwsh` is absent), all 46 focused
workflow/feed/macOS provider tests and all nine Rust offer tests passed, as did
desktop Clippy and formatting. The full desktop run had 349 passes, nine
sandbox-denied socket binds, one unchanged AppKit HTML decoder failure, and
three ignored tests. Native reruns, the missing passing-Mac log, and independent
review of the updater change remain outstanding. The full script runner also
invokes Git and is outside this worker's no-Git boundary.

Run 41165132554 qualified `0.2.3-nightly.20261031022408` against
`0.2.3-nightly.20261031025230`. The maintainer reports the first complete Linux
pass. The supplied `update5.log` contains the Windows and macOS failures:

| Platform | Evidence and cause |
|---|---|
| Windows | Lines 154–183 prove a focused offer, one matching action, no InvokePattern and **no attempted activation**; 194–196 independently prove the visible foreground native offer. The failure is in the harness's activation fallback, after #1504's presentation fix worked. Source already selected the exact Install label and custom ID 1008 (Later is first, Install second); changing button order again would not fix it. The old fallback required a physical native Button HWND. TaskDialog actions can be virtual, so qualification now sends one bounded `TDM_CLICK_BUTTON` for ID 1008 to the verified foreground dialog when InvokePattern is absent. UIA must first resolve and recheck the exact Install label, ownership, ancestry, enabled/visible state and immutable identities. An uncertain send remains terminal; replacement and restart still prove its outcome. The log does not retain the particular failed Win32 observation. |
| macOS | Lines 466–470 prove one successful automatic check for the expected version, with no failed check. At 33.262 seconds the census changes to three CG windows, one unnamed, and loses picker focus through 184.089 seconds, while still reporting zero sheets/actions (405–422). The observer only searched `AXSheets` and separate `AXWindows`, omitting attached sheets in the picker's standard `AXChildren` tree. Qualification now searches those children too, deduplicates native identities and retains parent/PID, heading, exact action, focus and final identity checks before one AXPress. This source/model diagnosis fits a missed focused sheet; the log has no AX hierarchy to prove that particular runtime representation. |

The harness never types or seeds a query. No evidence here establishes a
retained-query or idle-readiness product defect, so keep the empty-input and
1.5-second idle rule and the product unchanged. Do not clear input, reopen
again, force focus or issue an explicit check to repair automatic qualification.
The child-only sheet and TaskDialog-action regressions failed before the fixes.
The macOS model also uses Later as the initially focused first button, checks
the same sheet exposed through multiple attributes, and refuses changed
ownership, duplicate child sheets, failed child I/O and close/reopen before
activation. The Windows provider model runs the production native readiness
and click code against an OS boundary model, including virtual actions with no
Button HWND, refusal, timeout, changed ownership/focus and replacement.

Local validation: 69 qualifier self-tests passed; the PowerShell provider
contract skipped because `pwsh` is absent. All 51 focused workflow, feed,
macOS-provider and idle-input tests passed with Node 24. Windows provider
execution and installed Windows/macOS reruns remain necessary; model tests do
not establish package replacement or restart. No product or workflow files
changed, and no commits were made.

Run 41167455728 reaches replacement and a live restarted process on macOS and
Windows, then fails the synthetic library comparison (`update6.log`, lines 192
and 420). This is a harness path-spelling defect. Its SHA-256 matches the local
qualifier before this fix. The nightlies' timestamps identify sources
`cdfca68bc467` (02:24:08 UTC) and `a40f5152cba8` (02:52:30 UTC). Both use the same
`com.ibiscite.ibis.nightly` identifier, app-config settings location, schema 2
and BibTeX enum shape. #1512 does not change `crates/ibis-core/src/settings.rs`;
#1500 only adds defaulted, optional `picker_windows` geometry there.

Both versions already canonicalize the library during startup and persist the
captured read identity: `library/tasks.rs`'s `OpenIntent::Read.saves_choice`
allows the write, and `s.library = Some(chosen)` (line 410 in both sources)
saves `PreparedFileLoad::source_path`. This resolves macOS's `/var` directory
alias and retains Windows's `\\?\` prefix. Linux's synthetic path keeps its
spelling. The old verifier instead compared the entire library object with
`{"bibtex": str(library)}`. It therefore rejected the same file even though
the previous app could already have rewritten it before offering the update.
No Settings window interaction, schema migration or restart-write wait is
involved. The supplied log does not contain the saved path itself.

Qualification now requires exactly one string-valued `bibtex` member naming
the same existing regular file by native file identity. The shortcut still
matches exactly. Other files, even with identical contents, missing or unreadable
identity, relative paths and malformed choices fail immediately without raw
values in diagnostics. No retry or environment library override masks a lost
choice. Regressions cover canonical spelling, directory aliases, failed identity
I/O and the final restart result. Installed macOS/Windows reruns remain required.

Local validation: 74 qualifier self-tests passed; the PowerShell provider
contract skipped because `pwsh` is absent. All 26 focused feed, macOS-provider
and idle-input Node tests, 36 Rust settings tests and three settings-helper
tests passed. Node 24 with `NODE_OPTIONS=` and `NODE_USE_SYSTEM_CA=0` resolved
the sandbox's Node certificate startup failure. The full script runner was
not run because it invokes Git, outside this worker's boundary.

## Update consent keyboard safety

PR #1504's foreground fix exposed a pre-existing consent hazard: Return meant
for Search could accept an automatic update. The locked sources were checked
locally, without changing dependencies:

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
`gtk_offer_return_declines_and_explicit_install_button_accepts` test waits for
mapping and GTK's focused-widget state within a two-second deadline per phase.
On fresh dialogs it proves that Return/default activation cancels, Later stays
the default even when Install is focused, and clicking the native Install button
emits Accept. It does not invoke the answer callback or fabricate a response.
The existing Linux native CI step runs it under Xvfb before the picker smoke.
Real Tab/Space navigation remains covered by the managed XTEST fixture below.
Source inspection and mocks do not replace installed native keyboard
qualification on each platform.

The Linux qualifier builds `scripts/update-gtk-probe.c` as a private GTK module
and loads it only in its owned test processes. It resolves the AppImage’s GTK
symbols, reads the visible offer’s actual button label, response membership and
focus, and publishes only bounded numeric facts on that exact X11 window. It
never changes focus, defaults, responses or input. This avoids the documented
AT-SPI omission. Missing observations share the existing action deadline;
invalid ownership, ambiguous Install controls and exhausted navigation fail
before Space. At most six Tabs navigate to a verified Install; Space is sent
once and never retried. Like native picker smoke, input uses the XTEST server
keyboard path after proving focus, rather than directed `XSendEvent` input via
`xdotool --window`. The GTK fixture requires exactly one real Space press and
release with `send_event` false before accepting an Install response. The Linux native CI step runs
`update-gtk-probe.test.py` with `IBIS_GTK_CONTROL_TEST=1` under Xvfb/dbus: real GTK
dialogs exercise both button orders and refusal when Install is absent. Each
`--layout old|new|missing` invocation gets its own Xvfb server and D-Bus session;
an exited window manager's root properties cannot carry into the next case.
CI uses the distro `/usr/bin/python3`, forces `GDK_BACKEND=x11`, and checks the
wrapper tools before launching. Inside the display, a bounded preflight checks
the native tools, Python GI/GTK3 and its X11 backend, and the XTEST extension.
Missing dependencies, a nonlocal DISPLAY or an unavailable display fail rather than skip.
The real qualification still refuses a desktop with an existing window manager.
Fixtures write bounded result files and wait for their own PID to exit, so
accessibility services retaining inherited stdio cannot extend an EOF wait.
A process regression covers that inherited-pipe failure without requiring GTK. The
AppImage rerun remains necessary to verify loading against its bundled GTK.

The supplied `ci-1504c.log`, lines 82–98, shows a live fixture PID timing out
after the attempted activation. That rules out the earlier inherited-pipe
explanation for that run: availability, dialog observation and display setup
had already succeeded. The subsequent `ci-1504-d.log`, lines 85–107, records all
three managed GTK fixtures passing, including real Tab/Space on the Later-first
layout. Lines 124–127 instead show Cancel from the Rust test's unobserved
`move-focus` signal on bare Xvfb. That test did not establish its intermediate
focus state and kept using the same dialog after cancellation, although
production closes it. This is evidence of a test assumption, not a broken
person-facing Tab chain. The Rust test now uses the deterministic consent
checks above; its revised Linux execution and the bundled AppImage still need
native verification.

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
