# App icon B / G1 (#579)

`full.svg` is the approved optical transform and white bird on the exact G1
gradient, full bleed and unmasked. `small.svg` widens the neck to 11 units and
removes the eye. Both retain the original curves. The one-colour symbolic
master uses a 16px grid and a 1.75px neck. Do not centre the bird geometrically.

From the repository root, with the locked `cargo tauri` 2.12.3 and librsvg's
`rsvg-convert` (available in Linux CI):

```sh
python3 apps/desktop/src-tauri/icons/source/generate.py
pnpm -C apps/desktop exec vitest run src-tauri/icons/assets.test.mjs
```

Tauri renders the large PNGs directly from vector art. librsvg renders every
platform size directly from its master; Python's standard library packs those
PNGs into ICO and ICNS. No large bitmap is downscaled for the small entries.
The ICNS face is 824px in a 1024px canvas. Its 16/32px images use the small
master (including the 16pt @2x entry). The Windows 16/24px images use the small
master. The GNOME face has a 4px front profile on the 128px grid.

macOS ships **ICNS only**, the fallback explicitly allowed by #579.
`Ibis.icon` is retained as an **unshipped prototype** with three separate parts:
the root gradient fill, the bird group and the eye group. The locked Tauri
CLI/bundler has an `Assets.car` / `CFBundleIconName` path, but the actual app
bundle at `492fb6f` failed with `actool: Bad file descriptor`. Focused controls
also fail; the cause is unknown. Earlier direct compilations succeeded, which
does not establish reliable shipping. The hosted preview path also defaults
to Xcode 16.4, where the bundler skips the catalog. [#663](https://github.com/ibiscite/ibis/issues/663)
tracks reliable layered delivery; no workflow or toolchain change is made here.
Icon Composer orders groups front to back, so the eye is listed before the bird.

For example, export a template view with the bundled Apple CLI (on macOS):

```sh
"/Applications/Xcode.app/Contents/Applications/Icon Composer.app/Contents/Executables/ictool" \
  apps/desktop/src-tauri/icons/source/Ibis.icon --export-image \
  --output-file /tmp/ibis-default-1024.png --platform macOS \
  --rendition Default --width 1024 --height 1024 --scale 1
```

Repeat with `Dark`, `ClearLight`, `ClearDark`, `TintedLight`, `TintedDark` and
sizes 128, 32 and 16 for the review set. The historical labelled sheets, source
commit, file hashes and binary audit are [archived in Git](https://github.com/ibiscite/ibis/tree/5704c7a8790e4e43a5247025c870402bbd0438c9/apps/desktop/src-tauri/icons/review).
Those are CLI exports composited on synthetic backgrounds, not native editor,
Dock or taskbar screenshots.

The Windows adaptation needs the maintainer's final look. Windows uses the
same curves in G1 on transparency, with an eye cutout, to satisfy the no-tile
requirement. Its `viewBox="20 19 70 70"` fits the unchanged bird curves into
the Windows slot with about one pixel of clearance at 16px, instead of keeping
the plate's empty margins. This is a proposed adaptation, not a recovered
board-R master.
The unshipped layered prototype's Dark explicitly retains G1 and the white bird:
the board's darker endpoint colours were not available in the public
specification. Clear and Tinted are Apple's treatment of that prototype, not
shipped appearances. No custom lighting, shadow or glow is baked into it.

The Linux `deb.files` and `appimage.files` maps install the three named hicolor
PNGs and symbolic SVG. The shared Debian desktop template (also used by the
AppImage bundler) selects that icon name. Native package installation, system
tray resampling and Windows 100/125/150% pixel alignment need native evidence;
source renders and config tests alone do not establish those results.
