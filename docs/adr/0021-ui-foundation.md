# 0021. Build the desktop interface on React Aria Components and layered CSS tokens

Status: accepted (2026-10-20, maintainer decision)

## Context

The picker's interface (`apps/desktop/src`) is React and TypeScript in a Tauri 2 webview: WKWebView on macOS, WebView2 on Windows and WebKitGTK on Linux. `App.tsx` holds about 1,000 lines: the header, result list, More menu, keys dialog and notices. Menus, keyboard-only focus rings and the format choice are written by hand, and the format choice is a native `<select>`, whose popup is native only on macOS. Coming work needs many more of the same parts: the toolbar menus for format and library scope (#536, #537), tooltips, the preview pane with page navigation, a companion window, settings, and tray and menu actions ([design](../design.md), #223).

Accessible menus, filterable lists, tooltips and dialogs are where hand-written interfaces most often go wrong: keyboard paths, focus return, `aria-activedescendant`, screen-reader announcements, input-method composition and right-to-left layout. The paused draft for #536 reached this point. Five maintained headless React libraries were compared: React Aria Components, Radix Primitives, Base UI, Ark UI and Headless UI. The comparison, the sources and a rough bundle estimate are in #541.

For tokens, the W3C Design Tokens Community Group format reached a stable version (2025.10), with a resolver module for themes. Its colour type allows only numeric colour components. Ibis's colour roles are mostly CSS system colours (`Canvas`, `Highlight`, `AccentColor`), `light-dark()` and `color-mix()`, so that they follow the system accent, dark mode and high contrast. The DTCG colour type cannot express these.

## Decision

1. **One headless component library.** The desktop interface uses React Aria Components (npm `react-aria-components`, Apache-2.0, maintained by Adobe in `adobe/react-spectrum`) for interactive parts: buttons, menus and toolbar menus, filterable lists, popovers, tooltips, dialogs, tabs, switches and text fields. The version is pinned exactly. Other headless or styled component libraries are not added alongside it. Ibis keeps its own appearance: no React Spectrum styles, no CSS framework. Only Ibis's own components in `src/ui/` import `react-aria-components`; feature code imports those components, never the library. The pull request that adds the dependency also adds a check to `node scripts/check.mjs` that fails when a file outside `src/ui/` imports `react-aria-components` or `react-aria`. ESLint is not set up in the desktop app, so this is a small import scan rather than a new linter. Swapping the library later is then a change contained in `src/ui/`.
2. **Shared components live in `src/ui/`.** They know nothing about references or libraries. Screens and their state live in `src/features/<feature>/`. Each `ui/` component has its own CSS file, a vitest test for each platform look, and a state in the gallery.
3. **Behaviour gates for every migrated control.** A control moves to `src/ui/` only when its tests prove that today's behaviour is kept:
   - Ctrl/⌘ K opens and closes the More menu. With a menu or dialog open, Escape closes only that layer, never the picker.
   - Focus returns to the search box, or to the control that opened the menu, after a pick, Escape or Tab.
   - Typing with an input method (IME composition) never triggers a shortcut or a pick while composing.
   - Disabled items stay focusable and are announced as unavailable, so arrow keys still reach them.
   - The Zotero library checkboxes in the More menu stay `menuitemcheckbox` items with their checked state, and the last library cannot be turned off.
   - Roles, accessible names and axe-core pass.

   Vitest and gallery frames run in jsdom or a desktop browser, so they do not prove behaviour in the native webviews. Native behaviour is shown only by the native smoke tests and the CI captures on macOS, Windows and Linux. A migration pull request reports both kinds of evidence.
4. **Tokens stay hand-written CSS in three layers.** The base layer holds sizes, type, motion and raw brand colours. The role layer holds surface, text, accent, selection, status and citation. The look layer holds overrides under `:root[data-platform="macos|windows|linux"]`, then increased contrast and forced colours. Components read role tokens only. A test checks that every custom property used is defined and that spacing tokens (`--space-*`, gaps and padding) sit on the 4 px grid. Type sizes, row heights and border widths are not checked against the grid, because the agreed design uses 13 and 15 px type, 58 px rows and 1 px borders. A DTCG file and a generator are added only when a second consumer needs the same values, such as native Rust windows or a design tool. Even then, they cover only the non-colour tokens unless DTCG gains system colours.
5. **Real OS menus outside the window.** The menu bar, the tray menu and context (right-click) menus use the operating system's own menus through Tauri's native menu API. Web menus built on React Aria are used only for controls inside the window that need search, samples or a custom layout, such as the format and library menus and the More menu. These in-window menus follow each platform's native menu metrics (corner radius, row height, padding, separators and motion) through the look layer, so they sit naturally beside the real OS menus.
6. **Platform behaviour stays in Ibis's own tables.** Shortcut labels come from `keys.ts`. The pointer-selection rule (`pointerSelection.ts`) and the native Copied notice window (`notice.ts`) stay. React Aria's input-modality tracking replaces hand-written modality code.
7. **Migration is incremental**, one control per pull request, each under about 300 changed lines.
   1. Move the More menu and the keys dialog out of `App.tsx` into `src/features/picker/` with no change in behaviour or appearance.
   2. Run one pilot: add the dependency and build the format menu (#536) on it.
   3. Move the other controls one at a time. The result list moves last.

   The pilot pull request measures the production bundle change from `vite build` output. The 70 KB gzip figure in #541 is an estimate from a separate scratch bundle and is a hypothesis until then.

   The pilot also checks the Linux target. Tauri 2 needs the WebKitGTK 4.1 API (`libwebkit2gtk-4.1`). Ibis's Linux packages are built on Ubuntu 22.04 (`preview.yml`, kept for its older glibc), and that is the oldest Linux Ibis targets. So the target is WebKitGTK as shipped in Ubuntu 22.04 updates (`libwebkit2gtk-4.1-0` 2.50.4 on 2026-10-20). The `.deb` uses the system copy and the AppImage bundles the build host's copy.

   Success is measured as hand-written behaviour code deleted (menu keyboard handling, focus and modality tracking) against wrapper code added in `src/ui/`. `App.tsx` is not expected to shrink much from this, because most of it manages library, search and copy state, which a control library does not replace.

## Consequences

- One tested implementation of menus, focus and filtering serves every window. Right-to-left layout and built-in strings in more than 30 locales come from the library.
- Ibis depends on Adobe continuing to maintain React Aria. A replacement would touch only `src/ui/`, because screens use Ibis's own components, not the library directly, and the import check keeps it that way. The documented fallback is Base UI (npm `@base-ui/react`, MIT, maintained by MUI). It covers the same components, including a filterable list and tooltips.
- React Aria's Toast is still marked unstable. In-window notices use `role="status"` until it is stable.
- Popovers cannot extend beyond the webview window, so menus must fit inside the picker. Native popup windows, as Raycast uses, are a separate decision if ever needed.
- The result list keeps its current combobox semantics (#485) and moves last, only once moving it is shown to be worth the risk.
- No network use, data format, settings field or security boundary changes.
