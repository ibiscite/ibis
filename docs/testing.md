# Testing strategy

Citation software fails at the edges: unusual names, damaged files, rare item types, other scripts, odd editors. Ibis relies on automated tests, rather than manual review, to find those edges before users do. Layers marked "planned" are tracked in the linked issues and not yet running; all others run in CI on every change.

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

For an optional current-core scale measurement, run
`cargo test -p ibis-core --release --locked --lib owned_index_100000_reference_measurement -- --ignored --nocapture`.
The ignored test builds 100,000 synthetic multilingual records and checks common,
late-key and absent matches at the desktop's 20-result request and 200-result
backend bound. It checks full identities across sources and prints timings with
no machine-dependent threshold. A Linux x86_64 run with Rust 1.99.1 measured
200 ms to index, 31–37 ms mean common searches and 6–9 ms late-key/absent searches
(ten samples each). This measures core indexing/search only: parsing, native IPC,
serialization and rendering are separate; it is not an end-to-end picker claim.

CI tests the core and CLI with the declared Rust 1.90 minimum on Linux. The web interface is checked
and built once on Linux; all three native desktop builds embed that same artifact.

CI also runs the complete core suite in release mode on Linux, covering malformed
dates, private parser-panic containment and the property tests with release arithmetic.

The full check runs the default workspace's `cargo test --locked`, including the
CLI's Pandoc fidelity tests when `pandoc --version` succeeds. Without Pandoc those
comparisons skip; set `IBIS_REQUIRE_PANDOC=1` to require them. Missing optional
Pandoc does not cause the check script's incomplete (exit 2) result. To enforce
fidelity in PowerShell:

```powershell
$env:IBIS_REQUIRE_PANDOC = '1'
node scripts/check.mjs
```

The metamorphic suite generates small synthetic libraries with multilingual
text, inherited metadata and field warnings. Each of seven transformations
runs 64 cases in normal CI and 2,048 weekly (Monday): entry order, field order,
whitespace/CRLF/indentation, equivalent value and entry delimiters, type/field
name case, macro expansion/concatenation, and comments. It compares complete
record identities and normalized fields, plus warning kind/key/message;
verbatim source text, record/warning order and warning line numbers are the
only differences ignored. The generators exclude duplicate keys, duplicate
fields and conflicting macro definitions because their first/last-definition
precedence is meaningful. Quoted values exclude quote/group syntax that would
make delimiters semantically different. Failed weekly runs preserve the
shrunk example and regression seeds as an artifact. Turn reader regressions
into explicit synthetic fixtures or tests before fixing them.

The existing Vitest interface suite runs axe-core structural checks for first-run
and loaded-library states, with a deliberately broken combobox proving violations
are detected. JSDOM has no rendered layout/canvas, so color contrast is excluded
from axe; theme contrast has a numeric test. These checks do not establish text
reflow, native focus behavior or VoiceOver/NVDA acceptance (#8/#79).

## Native picker smoke

`scripts/smoke-native.py` launches the embedded native executable and reads the
OS accessibility tree. It does not use a browser, development server, injected
JavaScript or a test-only IPC command. Build with `--features tauri/custom-protocol`
first. Run only in a disposable desktop session: the suite owns the clipboard,
types synthetic text, opens a temporary editor and sends global shortcuts.
Windows opens Notepad, macOS opens TextEdit, and Linux opens an xterm running
`cat` as a synthetic text destination. The test verifies the copy-then-paste
contract by checking the OS clipboard and that the destination receives focus.
It does not inject a citation into the editor.

```sh
# Ubuntu/Debian runner dependencies (test tools, not app dependencies):
sudo apt-get install xvfb xauth xdotool xclip xterm openbox python3-pyatspi
xvfb-run -a -s '-screen 0 1280x800x24' dbus-run-session -- \
  /usr/bin/python3 scripts/smoke-native.py \
  --app target/debug/ibis-desktop --output screenshots/smoke-Linux

# macOS, in the logged-in runner session:
python3 scripts/smoke-native.py --app target/debug/ibis-desktop \
  --output screenshots/smoke-macOS

# Windows, in the logged-in runner session (PowerShell):
python scripts/smoke-native.py --app target/debug/ibis-desktop.exe \
  --output screenshots/smoke-Windows
```

Windows reads UI Automation through PowerShell and sends keyboard events with
`SendInput`; macOS reads accessibility through `osascript`/System Events and
posts global CoreGraphics HID keyboard events; Linux uses AT-SPI and xdotool
inside Xvfb with Openbox. Windows retains the native picker HWND first identified
by its editable search control; another same-process tray window cannot satisfy
the visibility assertion. Linux waits for both the editor's active-window and
actual X11 input-focus ownership before sending a shortcut. macOS first checks `AXIsProcessTrusted`, System Events
accessibility, Automation access and `CGPreflightPostEventAccess`. No event-posting
permission request is made. A missing TCC grant is **unavailable**,
never a pass; the test does not modify the TCC database or bypass consent.
macOS refuses any existing TextEdit process and opens an owned empty text file
through Launch Services, without sending Apple Events to TextEdit. Cleanup
requires the same PID, bundle identity and owned AXDocument before termination.
The macOS default shortcut is Control+Command+R; use
`--shortcut super+shift+space` when testing older builds. macOS must use a fresh
CI account without saved Ibis preferences. Windows reads its real native roaming
configuration folder through `GetFolderPath(ApplicationData)`; changing `APPDATA`
does not redirect Rust's known-folder lookup. Both platforms refuse existing
settings, backup and temporary preference files, then remove only files their run
can create. Copying can save the format even when `IBIS_LIBRARY` is set. Linux
uses a temporary `XDG_CONFIG_HOME`. Each test library has a synthetic path containing spaces and non-Latin
characters and is deleted on exit. Existing libraries are never opened or edited.

The suite asserts a loaded native window; accessibility names and roles for the
search field and result list, plus a named result-count status; shortcut activation
into the foreground with search focus; a second launch exiting successfully and
activating the original picker with focused search; native typed search; the exact Pandoc
citation on the OS clipboard after Enter, starting from a distinct synthetic
clipboard sentinel so an earlier run cannot satisfy the check; hidden picker and restored editor focus;
preservation of a valid external settings-field edit across the native copy-format
save, with all owned settings/backup bytes restored before later scenarios;
replacement of the previous query on reopening; a watched library edit; a missing
file error; an empty library count; recovery through Reload; damaged-library startup
with valid-record recovery; and invalid/newer settings messages and preservation.
On reopening, AT-SPI may expose the active result option as focused through
`aria-activedescendant`. The test proves search focus and selection by typing
the replacement query without selecting text, checking the exact accessible
search value and expected result. Initial empty-query activation still requires
the named search control to expose focus.
It records each
completed assertion in `smoke.json` and preserves startup logs. Failed runs record
visibility, foreground, search-focus booleans and focused control roles, without
accessible text. Scripted helpers identify their fixed action when they time out;
Windows timeouts retain only allowlisted initialization-stage lines from stderr.
macOS records fixed editor-visibility and foreground-ownership booleans from
System Events, NSWorkspace and NSRunningApplication before activation and after
copy/hide, without recording process IDs, bundle identifiers or document text.
An unexplained timeout is a failure, not evidence of denied permissions. Exit 1 means an
assertion failed, and exit 2 means a required capability was unavailable. A failed
run can contain earlier passed assertions; its overall status still fails.
Do not infer acceptance from merely installing tools or inspecting this code.
`python3 scripts/smoke-native.py --self-test` checks harness contracts; it does
not exercise the native app.

Independent `check-offline` probes in desktop CI establish the network check;
the smoke suite does not duplicate their polling or claim that a UI pass establishes
offline behavior. Screenshot capture likewise remains separate from assertions.

On Linux/macOS, capture the process baseline **before** starting Ibis with
`python3 scripts/check-offline.py --record-baseline screenshots/process-baseline.json`,
then run `python3 scripts/check-offline.py --pid PID --baseline screenshots/process-baseline.json --seconds 60`.
Linux samples numeric TCP/UDP endpoints with `ss` and first checks read access to
the selected processes' `/proc/PID/fd` directories. macOS uses `lsof` on the app
tree plus WebKit networking helpers that appeared after the baseline. Existing
helpers from other apps are excluded. A new reparented XPC helper's ownership is
not proven; any traffic is reported as a failed **session** observation rather
than attributed to Ibis. Use a quiet disposable runner. Permission diagnostics,
missing tools and malformed output fail the check. IPv4/IPv6 loopback and
unspecified binding addresses are allowed; link-local and private network
addresses are external. Sampling cannot prove absence of connections shorter
than the polling interval, and ownerless closed/TIME-WAIT sockets cannot be
attributed to an app. The check makes no packet-capture claim.

### Performance evidence and budgets (#9)

After the behavior checks, the suite creates 100,000 synthetic references and
restarts the app. `smoke.json` records milliseconds from process launch until the
accessible library count reaches 100,000, milliseconds from native query input
until the late key `smoke099999` appears in the native accessibility tree, and
current summed process-tree RSS (Windows: working set) after that result. These
observations include automation overhead and polling. Shared pages may be counted
more than once. WebKit helpers reparented outside the app process tree are not
included; memory is not a peak measurement. Record runner image and architecture
with any baseline. Each run records three fresh app processes against the 100k
fixture, retaining all samples and using each metric's maximum for enforcement.
Operating-system file caches persist between samples; these are not cold-disk
measurements or lifetime peak-memory measurements.

Initial hosted **debug** qualification ceilings are recorded with their evidence
in [smoke-budgets.json](../scripts/smoke-budgets.json). Desktop CI passes
`--budgets scripts/smoke-budgets.json`; every metric must pass for every sample.
The summary uses per-metric maxima, so a faster sample cannot hide a slower one.
A run without `--budgets` explicitly records `measured-without-budget`.

| Hosted runner cell | Observed maximum startup / first result / current tree memory | Initial ceilings |
| --- | --- | --- |
| Ubuntu 24.04 x86_64, Xvfb/Openbox | 21,280.26 ms / 1,945.87 ms / 818.20 MiB | 32,000 ms / 3,000 ms / 1,280 MiB |
| Windows Server 2025 x86_64 | 25,438 ms / 8,657 ms / 482.96 MiB | 40,000 ms / 13,000 ms / 768 MiB |
| macOS 15 arm64 | 20,323.48 ms / 7,871.04 ms / 351.25 MiB | 32,000 ms / 12,000 ms / 576 MiB |

Each ceiling rounds up at least 1.5 times the observed hosted maximum: startup
to two seconds, first result to one second and current tree memory to 64 MiB.
This allows runner variation while rejecting a substantial regression. Three fresh app
processes were observed on each OS; Linux also has repeated successful runs.
[macOS at 8d64d85](https://github.com/ibiscite/ibis/actions/runs/39177722811/job/117823838538)
and [Windows at 8d64d85](https://github.com/ibiscite/ibis/actions/runs/39177722811/job/117823838584)
passed all 13 interaction checks. Earlier Windows evidence comes from the separate
macOS-panel stack based on `bacb54d`; its Windows app and native harness are
unchanged from that baseline. The JSON records the exact runner images and links jobs.
An enforced final run is still required before claiming the performance gate
passed. These ceilings include UIA/accessibility/native-control probing and
input overhead; they do not describe app search latency alone. They qualify
those debug runner cells only. Release packages, other images/architectures,
peak memory and local container/proot observations need separate evidence.

The first enforced Linux run at `8b4f4da` failed its original 20-second startup
ceiling: all three samples were 20,359–21,280 ms, compared with earlier 10,665–13,302 ms.
All 13 interaction checks passed. The apps tree and native harness were identical
to the preceding `8d64d85`; both jobs used Ubuntu 24.04.5 image 20261012.334.1,
the same Rust toolchain and an unoptimized debug build. Runner regions differed,
but no CPU model/core count or load facts were recorded, so a hardware or region
cause is unproven. The initial calibration now includes that observed range:
the same 1.5-times rounding policy yields 32 seconds. This changes the initial
qualification baseline; it is not a runtime optimization or a diagnosed app fix.
An exceeded budget records `performance.status: failed`, the metric and numeric
ceilings, retaining and printing all three samples instead of calling the
attempted gate `measured-without-budget`.

### Manual-only checks (final pass before sharing)

Everything else is automated in CI on Linux, macOS and Windows: the picker,
search, copy, settings, damaged libraries, screenshots of every state and a
real Zotero instance with synthetic items. The items below need a person, real hardware or a real account, so they are
done once, by the maintainer, on the nightly build linked in
[#80](https://github.com/ibiscite/ibis/issues/80) when the app is otherwise
ready. Linux relies on automation only.

On Windows and on macOS:

1. **Your Zotero.** With Zotero open, choose *Use Zotero* and search your own
   library; check a few results look right. Nothing is written to Zotero.
2. **Screen reader.** With NVDA or Narrator (Windows) or VoiceOver (macOS), hear
   the search label, the changed result count and the selected result.
3. **Typing and shortcut.** On your keyboard layout and input method (including
   IME composition if you use one), type a search and check the displayed
   shortcut opens the picker.
4. **Paste for real.** Paste a copied citation into your writing app; on macOS
   also summon the picker from a full-screen app.
5. **Sleep and wake.** Sleep and wake the computer, then search and copy again.
6. **Displays.** If you use several displays or scales, unplug one and summon the
   picker, and move it between screens; check it stays readable.
7. **Synced folders.** If you keep files in iCloud, OneDrive or a network drive,
   put only a synthetic `.bib` there, edit it and check the picker recovers.
8. **Feel.** At your normal scaling, the picker is readable and feels fast.

macOS only: on a fresh user account, check the permission prompts and that the
app explains how to recover if one is declined.

Hosted automation cannot establish real screen-reader speech, full-screen
Spaces, multiple displays, non-US layouts and IMEs, sleep and wake, cloud-folder
sync, a person's own library, or subjective feel; that is why these stay manual.

## Picker screenshots

Every CI run builds and launches the real debug app on Windows 2025, macOS 15
and Ubuntu 24.04, including pull requests that change `apps/desktop/`. To capture
another commit, choose **Actions → CI → Run workflow** and select its branch.
No development server or app startup switch is needed: the build embeds the web
interface with `--features tauri/custom-protocol`, and debug windows stay visible
when they lose focus.

Open the run's **Artifacts** and download `picker-<commit SHA>`. The SHA identifies
the checked-out source (the merge commit for a pull request). It contains
`picker-Windows.png`, `picker-macOS.png`, `picker-Linux.png` and startup logs, kept
for 14 days. The three platform shards are combined after the desktop jobs finish;
failed jobs can leave a partial artifact. A process that exits during the startup
wait fails its job; capture errors produce warnings. Check that all three PNGs
exist and actually show the picker before claiming cross-platform evidence.

The app opens the synthetic `fixtures/bibtex/real-world.bib` through an absolute
`IBIS_LIBRARY` path, without saving a library preference. Screenshots show the
initial empty-query view after the 60-second startup socket observation: review layout, font rendering,
scaling, library status and window edges by eye. There is no pixel comparison or
automated visual verdict. Linux uses a 1280×800 Xvfb display without a desktop
window manager; its theme and shadows may differ from a user's desktop. These
images do not establish result-list behavior, typing, focus, shortcuts, clipboard
integration or screen-reader accessibility; those still need separate checks.

## macOS launcher regression (#198)

The native reproducer is a document in TextEdit in its own full-screen Space:
summon Ibis, search the synthetic `who2020` reference, press Enter, and paste.
Before the panel change macOS switches to the desktop or leaves the picker
outside the full-screen Space; a window-only hide also leaves Ibis active (#195).
The picker should stay over the document and paste should reach that document.

The macOS-only `tauri-nspanel` dependency converts the existing Tauri window to
an NSPanel with non-activating style, all-Spaces/full-screen auxiliary behavior,
and a floating level. Its safe API confines native calls to Tauri's main-thread
callbacks; the app retains `forbid(unsafe_code)`. It enables Tauri's macOS private
API feature, as the adapter requires. `Accessory` policy removes the Dock icon;
Quit stays available in the More menu and menu bar item. These flags on a normal
window were already tested natively and did not solve the full-screen problem
(see the evidence in #198).

Run the release build on macOS, quitting other Ibis instances first. Check:

- Full-screen TextEdit and another Space, with the pointer on each display of a
  mixed-scale pair; the Space must not switch and the picker must be visible.
- Stage Manager; typing and Japanese Kana conversion; VoiceOver search/result
  announcements. A compiling panel does not prove IME or accessibility behavior.
- Enter, Escape, shortcut and close; the previous app must receive the next
  paste. Repeat after a library dialog, which intentionally activates Ibis.
- Choose library from the picker and tray, then cancel and select a synthetic
  `.bib`; the dialog must receive input and the picker must remain available.
- Windows/Linux dismissal, foreground activation and virtual desktops in #133.

After integrating the native adapters from #217, run the focused scenario in
that same disposable macOS session:

```sh
python3 scripts/check-panel-macos.py --self-test
python3 scripts/check-panel-macos.py --app target/debug/ibis-desktop \
  --output screenshots/panel-macOS
```

The scenario requires actual AXFullScreen state plus on-screen editor geometry,
then checks that summoning keeps the editor in its full-screen Space while the
picker is visible and focused. It verifies native search, an independently primed
clipboard, Enter/Escape dismissal, and editor focus return. On the ordinary
desktop it opens the real titled library chooser, verifies dialog focus while
the picker stays visible, cancels, and checks keyboard access to the picker and
return to the editor. Cancellation may restore the Choose-button focus. It does
not select a file, change an input source, read VoiceOver speech, or establish
second-display/Stage Manager acceptance. Exit 2 and an `unavailable` report mean
no native pass; contract self-tests validate the probe only.

Global HID events target the actual key window, including a nonactivating panel
whose editor application intentionally remains frontmost. System Events keyboard
dispatch alone could target that editor instead. Unicode storage is retained
through each event post, and physical chords attempt every key release after a
failure. Debug builds log only actual AppKit key-window and first-responder
booleans after showing the panel; accessible focus alone is not that proof.
The panel helper initially makes its outer NSView the first responder. Ibis then
uses the stable Tauri Webview API to focus the actual WKWebView without activating
the application; AppKit key/responder booleans alone do not prove HID delivery.
The same WebView focus is applied after initial panel configuration only when
the native window is already visible. This covers fresh startup after damaged
settings recovery without showing a future hidden startup window.
Initial setup uses the same panel ordering and responder sequence as summons.
Debug builds observe initial state, native focus changes and the first completed
page load without adjusting focus in those event callbacks. These fixed booleans
include whether the first responder is the outer content view; no object identity
or page data is logged. Native key-window state alone did not prove that a fresh
startup received hardware typing, so the damaged-library startup assertion remains
required alongside all five fullscreen and chooser checks.
The prior hosted probe showed the panel in the full-screen Space, then failed
native search. That observation does not establish input, copy or chooser success.

Linux unit/interface checks do not execute AppKit. Hosted macOS compilation and
these native interactions must be recorded before calling this change verified;
full-screen, Stage Manager, IME, VoiceOver and second-display behavior remain
unverified until that evidence is attached to #198.

## Nightly fuzzing

Install `cargo-fuzz` with `cargo install --locked cargo-fuzz --version 0.13.1` and
nightly with `rustup toolchain install nightly --profile minimal`, then run:

```sh
cargo +nightly fuzz run load_str -- -max_total_time=60 -timeout=5 -rss_limit_mb=1024 -max_len=16384
```

Targets are `split_parse`, `load_str`, `csl_export` and `cite_render` (escaping
of every key syntax in `cite::render`). The nightly workflow gives
each 300 seconds with a persistent corpus seeded from synthetic fixtures, and
minimizes and reports crashes, timeouts and OOMs as deduplicated `area:testing`
issues. Fixed inputs become fixtures with regression coverage. Fuzz targets share
the root lockfile and are outside default application/test members. Only
cargo-fuzz enables nightly instrumentation; the app toolchain stays pinned. See `fuzz/README.md` for
reproduction, reporting and resource limits.
