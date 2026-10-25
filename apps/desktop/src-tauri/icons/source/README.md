# Desktop app icons

These are platform adaptations of the [final brand kit](../../../../../brand/README.md),
not separate logo masters. `generate.py` reads `brand/master.svg`, `settings.json`
and the drawing helpers in `brand/build.py`. Never edit the generated SVGs or
packaged images by hand.

From the repository root, with Python 3 and librsvg's `rsvg-convert`:

```sh
python3 brand/build.py
pnpm -C apps/desktop exec vitest run src-tauri/icons/assets.test.mjs
```

- **Shared:** the final scarlet bird with a cut-out eye, with shared depth on the whole
  silhouette. The bird fills 62% of a tile, with the kit's optical centring and
  shadow clearance. Below 48px, entries omit the eye and use the kit's thicker neck and flat colour.
- **macOS:** ICNS, with an 824px face in a 1024px canvas. The 16/32px images
  (including the 16pt @2x entry) use the small art. Layered Icon Composer delivery
  remains [#663](https://github.com/ibiscite/ibis/issues/663); the retired, unshipped
  B/G1 prototype is removed, not bundled.
- **Windows:** the bird on transparency, without a tile. ICO holds 16, 24, 32, 48
  and 256px images; entries below 48px are flat, with the thicker neck.
- **Linux:** the existing GNOME 104px face and 4px front on a 128px grid, adapted
  to the final light tile. The 128/256/512px hicolor images and one-colour symbolic
  SVG retain their existing names and package paths.

Each target size is rendered directly from vector art; no large bitmap is
downscaled to make a small entry. The shelf's small beak is generated from the same brand
master in `src/brand.ts`, using `currentColor` without a shadow. The Copied notice's check
is a status glyph, not a separate Ibis logo.

Asset/container tests and source renders do not establish installed Dock,
taskbar, tray or launcher behaviour. Report those native checks separately.
