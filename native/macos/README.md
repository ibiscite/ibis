# Ibis Native for macOS

This repo build is the picker and delivery from
[decision 0065](../../docs/decisions/0065-one-warm-engine-thin-shells.md).
It targets macOS 15 and later, with Liquid Glass on macOS 26 ([decision 0066](../../docs/decisions/0066-native-shells-use-system-parts.md))
and uses the workspace Rust toolchain and Xcode 26 or its Command Line Tools.

```sh
scripts/native-macos.sh dev      # rebuild, quit the running copy, open the picker on Review.csljson
scripts/native-macos.sh shots    # screenshot every review state in light and dark
scripts/native-macos.sh test
scripts/native-macos.sh run --file apps/desktop/src-tauri/resources/samples.csljson --show
```

The interface is SwiftUI over the system's own parts, with AppKit for the
panel, its glass, native menus, the hot key and focused text fields
([decision 0066](../../docs/decisions/0066-native-shells-use-system-parts.md),
[the native recipe](../../docs/design/macos.md#the-native-recipe)).
The engine owns the picker session: the query, results, highlight, the
references added with their details, the format and the preview. Sources are
grouped by job: `App/` (launch, hot key, review states), `Engine/` (the daemon
client), `Picker/` (`PickerStore` sends intents and keeps the latest state,
`PickerCommands` the commands and native menus, `PickerPanel` the window and
keys, `PeekPanel` the Quick Look-style preview, `PickerView` draws),
`Settings/`, `Setup/` (the permission card) and `Shared/` (`SystemGlass`, the
text field). `shots` runs each `ReviewState` in an
isolated profile and saves the PNGs under `target/native-macos/shots`.
`screencapture` needs Screen Recording permission for the terminal once.

The build produces `target/native-macos/Ibis Native.app` with the bundle ID
`com.ibiscite.ibis.native` and a local ad-hoc signature. `--show` explicitly
opens the picker at launch. Use ⌥⇧⌘Y or the Ibis menu-bar item to summon it
later. To change the key, open Settings and select the shortcut next to
**Open Ibis**. Use Default returns to the repo build's shortcut for side-by-side
use with Tauri.
Quit Ibis Native before running a rebuilt bundle or supplying a new
`--file` argument to a fresh launch.

The published Mac nightly still packages the Tauri app from `main`. This repo
build has a separate bundle ID, shortcut and settings profile for side-by-side
testing. The Mac nightly changes shells only after the release workflow is
updated and the signed, notarised update path is verified.

Choose a library in the picker or in Settings › Library. The engine checks
the file before remembering it for future launches. The chooser accepts
BibTeX, BibLaTeX, CSL JSON, RIS and CSV. The command-line `--file` argument
sets a temporary library. Format sits with Page in the delivery tray. Its
menu names the app or site it remembers a
choice for. Use This Paste Only to hold a format until the picker closes, or
Forget to remove the named saved choice. Site scope requires existing macOS access to the captured browser window
and a listed site. Search can
explain closest, spelling and hidden-field matches. It shows the active source,
title and byline, with recent citations for the captured
app or source-added references when the query is empty. Use Up and Down to
preview a reference or Return to deliver it. The highlighted row uses the
system accent, and the footer shows exactly what Return delivers. Tab adds the
highlighted reference to the citation and clears the field for the next one;
added references sit first, under In citation. Up from the first result reaches
them, Shift-Up and Shift-Down reorder them, and Delete in an empty field removes
one. Return on an added reference, a click, or ⌘L opens its page, text before,
text after and Show as in its row; Return or Escape closes it. Hovering a row
shows Add and Peek, and right-click lists the same. ⌘T starts typed citations,
where a semicolon completes a reference. Escape closes what is open, clears the
query, then hides the picker. Actions (⌘K) opens a menu of every command that
can run now.

For automatic paste, use Pasting in Settings › General (⌘,). The switch saves your
preference and the form shows whether macOS allows the paste. If access is
missing, use **Open System Settings** to allow this copy of **Ibis Native**
separately from Ibis and Ibis Nightly. Return to your editor and summon Ibis
again. With the switch off or paste unavailable, Ibis copies the citation.
The native build uses its own settings profile in Application Support. Its
stored recent citation history contains reference identities. In Settings,
Appearance updates a live sample as you change the theme, Rows and Larger Text.
Use ⌘F to find a control by its name or task. Return jumps to the actual control
on its page. The Ibis menu bar item also opens About Ibis.
⌘+ and ⌘− temporarily scale the picker from 100% to 200%. Like Spotlight the
window is not resizable: its height follows the results.

The existing nightly sets the functional baseline for the native app
(maintainer, 2026-11-27). The next Settings work covers launch at login.
Project management, project bibliography checks and editor commands remain
parity work. The repo picker pauses delivery for an active project whose
bibliography needs checking.

The bundled engine joins a daemon from the same build. If several builds
use a profile, the newer build wins. Use an absolute `IBIS_CONFIG_DIR` for
a test run with its own daemon and settings profile. XCTest creates an
isolated profile and invented library in `.build/`. The AppKit smoke test
runs when a WindowServer session is available. Regenerate the Swift
contract with `node scripts/gen.mjs` after changing picker methods.

## Handoff

Further Settings controls, Sparkle, release signing and notarisation remain.
The macOS CI leg builds and tests the repo app. Packaging it for the Mac
nightly is release work. The Reader, advanced Zotero selection, projects and app-specific
paste approvals remain parity work. The site can describe native availability when the app ships in a
nightly.
