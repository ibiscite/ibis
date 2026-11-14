# 0044 One Lucide icon primitive

Use the approved Lucide 1.72.0 drawings from
`lucide-static`, under ISC; retain the full licence, including Feather's MIT
notices, in `brand/icons/LICENSE-lucide`. The mapping is in `brand/README.md`.

`Icon` owns two fixed pixel sizes: `--icon` (16px, controls, menus and rows)
and `--icon-tile` (20px, tiles and toolbar). Scale the 24-unit stroke to 2.25
and 1.8 units respectively so both render 1.5px, centred on the label line.
Parts choose a size through the primitive and never override its geometry.

Draw a missing glyph only when Lucide has no suitable meaning, on its 24-unit
grid with a nominal 2-unit stroke, round caps and joins and no fill. Normalize
to one path with subpaths for the guard and check animation. App and file icons
remain system artwork, with the library glyph as the fallback.
