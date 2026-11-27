# Ibis Native for macOS

This repo build is the picker and delivery from
[decision 0065](../../docs/decisions/0065-one-warm-engine-thin-shells.md).
It targets macOS 13 or later and uses the workspace Rust toolchain and
Xcode Command Line Tools with Swift 5.9 or later.

```sh
scripts/native-macos.sh build
scripts/native-macos.sh run --file apps/desktop/src-tauri/resources/samples.csljson --show
scripts/native-macos.sh test
```

The build produces `target/native-macos/Ibis Native.app` with the bundle ID
`com.ibiscite.ibis.native` and a local ad-hoc signature. `--show` explicitly
opens the picker at launch. Use ⌥⇧⌘Y or the Ibis menu-bar item to summon it
later. The repo build has its own shortcut for side-by-side use with Tauri.
Quit Ibis Native before running a rebuilt bundle or supplying a new
`--file` argument to a fresh launch.

Choose a library from the menu or pass its path on launch. The chooser accepts
files the engine can open, including BibTeX, BibLaTeX, CSL JSON, RIS and CSV.
The file choice and format changes made in the picker last for the current
run. Search shows title and byline, with recent citations for the captured
app or source-added references when the query is empty. Use Up and Down to
select a reference and ⇧↩ to add it to the citation. ⌘T starts typed
citations, where a semicolon completes a reference. Use ⌘L to edit the active
reference’s Page field in the bar, or use its menu to choose a chapter or
another locator type. Click a citation token or press ⌘I to edit Text before,
Text after and Show as in a popover. Return runs the delivery action shown in
the bar. Escape closes the popover, clears the query, then hides the picker.

For paste, choose **Allow Paste…** in the Ibis Native menu. In macOS System
Settings, grant **Ibis Native** Accessibility separately from Ibis and Ibis
Nightly. Summon it again from an editor after granting permission. When
paste is unavailable, the citation is copied in the engine's available
clipboard formats. The native build uses its own settings profile in Application Support.
Its stored recent citation history contains reference identities. Settings (⌘,)
saves Appearance, Rows and Larger Text through the engine. Find it in More
and the Ibis menu bar item, which also opens About Ibis.
⌘+ and ⌘− temporarily scale the picker from 100% to 200%. The window keeps its
size while you type and remembers edge resizing per display.

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
