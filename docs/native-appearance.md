# Native appearance groundwork

The picker keeps system fonts, system shadows and its existing opaque system-colour
surface by default. Windows 11 rounded corners are already requested by Tauri's
undecorated `shadow: true`; this change adds no custom corner geometry.
macOS outer corners remain square in the inspected borderless-window capture.
`window-vibrancy` rounds only its effect view (None means radius zero), not the
outer native window or WebView. The panel crate's corner setter changes its content
layer radius without enabling clipping. Neither establishes native outer corners;
this macOS foundation requirement remains open. Changing native titlebar styles
or choosing/clipping a custom radius needs separate native input/shadow evidence.

On macOS and Windows, window and WebView transparency are configured together
before window creation. A safe native accessibility check precedes macOS Popover
vibrancy or Windows 11 Mica. The web surface becomes transparent only after the
native effect reports success. Unknown preferences, unsupported systems and API
failures retain the opaque surface. Linux retains its current opaque window.
Contrast and reduced-transparency CSS queries independently restore Canvas.

Preferences refresh on every summon. Windows uses safe preference notifications;
if notification registration fails, it keeps opacity. macOS notification
registration requires unsafe in the current bindings, so a one-second visible-only
poll queues at most one main-thread refresh. Effects and events change only when
policy changes. A failed effect is retried after a preference-policy change or
restart, rather than continuously reapplying it. Diagnostics use fixed text.

The direct `window-vibrancy` dependency is the same version already locked through
Tauri. Unlike Tauri `set_effects`, its APIs report application failures. Direct
`objc2-app-kit` and `windows` dependencies expose safe native accessibility APIs;
no Ibis unsafe code, network access, or additional package versions are required.
The underlying macOS implementation still invokes a private corner-radius selector.
Its success result does not prove appearance, clipping or every native failure mode.

Policy and interface tests cover fallback decisions and event ordering. Real
material appearance, live accessibility toggles, light/dark mode, corner silhouette,
resize/shadow, fullscreen summon, chooser and focus/clipboard behavior still need
native evidence. Existing opaque-picker screenshots do not establish these checks.
This groundwork remains draft while integration with current main/gallery/smoke
and its exact checks are pending. Material appearance and live accessibility feel
remain explicit cells in the final combined native session; a startup fallback
is useful evidence but does not pass those cells. IME and VoiceOver speech are
separate native checks.

## Evidence and remaining checks

At commit `f4ad1a210c9e546f4013ef9b7fa98ca981939604`,
[CI 342](https://github.com/ibiscite/ibis/actions/runs/39172930723) passed all
platform builds, tests and startup captures. Its
[native artifact](https://github.com/ibiscite/ibis/actions/runs/39172930723/artifacts/12153581580)
was inspected directly:

| Platform | Observed | Limit |
| --- | --- | --- |
| macOS 15 | Picker visible, flat opaque surface matching the CSS fallback; startup log empty. | No selected-mode evidence yet; cannot distinguish accessibility fallback from an unexposed effect. |
| Windows Server 2025 | Picker visible and opaque; fixed appearance-unavailable diagnostic. | Establishes a fallback observation, not Windows 11 Mica acceptance. |
| Linux | Picker visible and opaque. | Expected unsupported-material fallback; no material is requested. |

At evidence commit `365b136445a9bfcf982a5f983666d112a85bd8ae`,
[CI 350](https://github.com/ibiscite/ibis/actions/runs/39174798201) passed all
three desktop jobs and the full matrix. Its inspected
[combined artifact](https://github.com/ibiscite/ibis/actions/runs/39174798201/artifacts/12153902926)
contains these actual native startup selections:

| Platform | Fixed mode/reason | Screenshot observation |
| --- | --- | --- |
| macOS 15 | `opaque / accessibility` | Visible opaque picker; square outer silhouette. |
| Windows Server 2025 | `opaque / unknown-preferences` | Visible opaque picker; unavailable-preferences fallback. |
| Linux | `opaque / unsupported` | Visible opaque picker, as expected. |

These runs established initial fallback observations. The later focused macOS
session below supplies actual material and live-toggle observations.

New startup logs emit one fixed `ibis: native appearance mode=… reason=…` line
for initial selection and policy changes. Modes are `opaque`, `vibrancy` or
`mica`; reasons are `unsupported`, `not-transparent`, `unknown-preferences`,
`accessibility`, `native-failure` or `applied`. An `applied` observation means
the maintained native API returned success, not that a material was visible.
An absent line is missing evidence. Logs never include native errors or paths.
The existing screenshot/startup artifact already captures these lines; no new
workflow or app testing mode is required.

The pinned [window-vibrancy 0.8.1 implementation](https://docs.rs/crate/window-vibrancy/0.8.1/source/src/)
checks Windows build support and DWM results; macOS checks the main thread and
AppKit version. These static guarantees support fallback policy but cannot prove
compositor output. macOS still uses a private corner selector internally.

Use the final integrated build, with gallery mode disabled and a synthetic
library, for one combined native appearance/accessibility session per target OS:

1. With OS transparency enabled and contrast disabled, use a controlled
   nonuniform window backdrop on macOS and a controlled desktop wallpaper for
   Windows Mica, whose material is wallpaper-derived. Record exact commit, OS,
   fixed mode line and screenshot.
   Require `vibrancy/applied` on supported macOS or `mica/applied` on Windows 11,
   then verify the backdrop/material is visible and text stays legible in both
   light and dark mode. An API success with a flat opaque surface fails this check.
2. While the picker stays open, enable Reduce Transparency on macOS or disable
   Transparency effects on Windows. Require opaque output and an accessibility
   fallback observation, within the macOS polling interval. Also enable the OS
   contrast setting and confirm the opaque legible surface. Restore original
   settings and confirm material can return. Missing native preference support
   is recorded as unavailable, never passed from CSS/unit tests alone.
3. In the same session, resize and inspect native shadow/corners, then run the
   existing native shortcut/copy/chooser/fullscreen checks on this exact build.
   Reuse that evidence rather than repeating it as a separate appearance ritual.

At `f0ec49ba14e1e1595d933d1e8338668601baf574`, the macOS
[native artifact](https://github.com/ibiscite/ibis/actions/runs/39177725318/artifacts/12154819869)
records all five expected mode observations: vibrancy with both accessibility
settings off, opaque Reduce Transparency, restored vibrancy, opaque Increase
Contrast (which also enabled Reduce Transparency), then restored vibrancy.
The five inspected screenshots show the owned RGB backdrop blurred and tinted
through the material, an opaque reduced-transparency surface, and black text and
lines in high contrast. These are real rendering and live preference observations.
The overall run failed because a final restoration action timed out and Settings
cleanup failed. Its final actual preference values were not recorded, so verified
restoration and a complete successful session remain required. Draft acceptance
is unchanged by those partial successes; macOS outer corners remain square.

Do not modify protected OS preference databases or infer permission denial from
an unexplained helper error. IME/VoiceOver and display/provider cells remain in
the shared compatibility checklist, not extra appearance-only hand checks.

The focused macOS cloud probe uses ordinary System Settings Accessibility Display
checkboxes. It refuses a preexisting System Settings process and existing Ibis
settings, saves both native boolean values before changes, and restores them in
`finally`, including after a failed observation. It never writes preference/TCC
databases or requests permissions. Only its newly identified System Settings
process and owned app/backdrop subprocesses can be closed.
Each action checks fresh AX arrays across all owned Settings windows, with the
same process identity and traversal bounds. A single pass reads each node's role
once and labels only checkbox nodes, rather than scanning the entire tree twice.
Every action requires one exact match for each fixed preference label. Positions
and unchanged tree counts cannot prove uniqueness; no control cache is reused.
Restoration records all attempts, the actual final booleans and whether
they match the originals. An attempt failure still fails the run even when a
later read matches. Cleanup observes termination after a normal quit request;
only a still-live revalidated owned Settings process, after verified restoration,
can be force-terminated. Its requested/terminated/forced facts are recorded.

```sh
python3 -B scripts/test_native_appearance_macos.py
python3 scripts/check-native-appearance-macos.py --app target/debug/ibis-desktop \
  --output screenshots/appearance-macOS
```

The ordinary picker stays over the same owned red/green/blue native window while
the background settings controls are toggled. The probe requires new allowlisted
mode observations for both accessibility fallbacks and material restoration;
gallery mode is cleared. It captures each phase and records whether original OS
preferences were restored. Exit2 means explicit missing native capability, and
still supplies no acceptance pass. Exit0 records observed mode transitions and
screenshots; a reviewer must inspect those captures to establish visible material
and legibility, rather than treating API success alone as visual evidence.
