import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { App } from "./App";
import { galleryProvider, galleryStates } from "./gallery-fixtures";
import { platformOf, setPlatform } from "./nativeAppearance";

// Vitest can load these existing runtime builtins without a Node-types dependency.
const { readFileSync, readdirSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string; readdirSync: (path: URL, options: { recursive: true }) => string[] }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");

const tokens = readFileSync(new FileURL("./tokens.css", import.meta.url), "utf8");
const styles = readFileSync(new FileURL("./styles.css", import.meta.url), "utf8");
const buttons = readFileSync(new FileURL("./ui/Button.css", import.meta.url), "utf8");
const gallery = readFileSync(new FileURL("./gallery.css", import.meta.url), "utf8");
const corners = readFileSync(new FileURL("../src-tauri/src/native_corners.rs", import.meta.url), "utf8");

afterEach(() => { cleanup(); clearMocks(); delete document.documentElement.dataset.platform; });

/** Every declaration block whose selector list is, or includes, `selector`, joined. */
function rules(css: string, selector: string): string {
  return [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
    .filter(([, head]) => {
      const list = head!.replace(/\/\*[^]*?\*\//g, "").trim();
      return list === selector || list.split(/\s*,\s*/).includes(selector);
    })
    .map(([, , body]) => body)
    .join(";");
}

test("the platform comes from the same signal as the shortcut labels", () => {
  expect(platformOf("MacIntel")).toBe("macos");
  expect(platformOf("iPad")).toBe("macos");
  expect(platformOf("Win32")).toBe("windows");
  expect(platformOf("Linux x86_64")).toBe("linux");
  expect(platformOf("")).toBe("linux");
  setPlatform("Win32");
  expect(document.documentElement.dataset.platform).toBe("windows");
});

test("the search header is the 48–56 px search bar, not an 80 px block", () => {
  const bar = rules(styles, ".search-bar");
  expect(bar).toMatch(/min-height:\s*calc\(var\(--search-bar-height\) \+ var\(--border-width\)\)/);
  expect(bar).toMatch(/padding:\s*var\(--inset\)/);
  // The nominal control band excludes its one-pixel divider.
  expect(tokens).toMatch(/--search-bar-height:\s*calc\(var\(--field-h\) \+ 2 \* var\(--inset\)\)/);
  expect(tokens).toMatch(/--field-h:\s*2\.25rem/);
  expect(rules(styles, ".search-bar .pin-button")).toMatch(/margin-inline-start:\s*auto/);
});

test("Windows and Linux search in a 16 px framed box; macOS stays frameless and large", () => {
  const framed = rules(tokens, ':root[data-platform="windows"], :root[data-platform="linux"]');
  expect(framed).toMatch(/--search-size:\s*1rem/);
  expect(rules(styles, ':root[data-platform="windows"] #search')).toMatch(/border:/);
  expect(rules(styles, ':root[data-platform="windows"] #search:focus')).toMatch(/var\(--accent\)/);
  expect(rules(styles, ':root[data-platform="linux"] #search')).toMatch(/border-radius:/);
  expect(tokens).toMatch(/--search-size:\s*var\(--type-search-size\)/);
});

test("macOS selection uses a supported native background and text pair before contrast overrides", () => {
  // WKWebView accepts AccentColorText but resolves it to black on blue.
  // The paired Apple list colours were checked in native light/dark probes.
  const guard = "@supports (background: -apple-system-selected-content-background) and (color: -apple-system-alternate-selected-text)";
  const start = tokens.indexOf(guard);
  expect(start).toBeGreaterThan(tokens.indexOf("@supports (color: AccentColorText)"));
  const block = tokens.slice(start).match(/^[^{]+\{\s*:root\[data-platform="macos"\]\s*\{([^{}]*)\}/)?.[1];
  expect(block).toMatch(/--selected-background:\s*-apple-system-selected-content-background;/);
  expect(block).toMatch(/--selected-text:\s*-apple-system-alternate-selected-text;/);
  for (const media of ["@media (prefers-contrast: more)", "@media (forced-colors: active)"]) {
    expect(tokens.indexOf(media)).toBeGreaterThan(start);
    expect(rules(tokens.slice(tokens.indexOf(media)), ":root[data-platform]")).toMatch(/--selected-background:\s*Highlight;\s*--selected-text:\s*HighlightText;/);
  }
});

test("the selected row follows each platform", () => {
  const active = rules(styles, '[role="option"][data-active]');
  expect(active).toMatch(/background:\s*var\(--row-active-background\)/);
  expect(active).toMatch(/color:\s*var\(--row-active-text\)/);
  // macOS (the default): filled accent with white text.
  expect(tokens).toMatch(/--row-active-background:\s*var\(--selected-background\)/);
  expect(tokens).toMatch(/--selected-background:\s*AccentColor/);
  expect(tokens).toMatch(/--selected-text:\s*AccentColorText/);
  // Windows: soft grey with an accent pill; Linux: GTK's selection colour (the Highlight pair).
  expect(rules(tokens, ':root[data-platform="windows"]')).toMatch(/--row-active-background:\s*color-mix\(in srgb, var\(--text\)/);
  expect(rules(styles, ':root[data-platform="windows"] [role="option"][data-active]::before')).toMatch(/background:\s*var\(--accent\)/);
  const linux = rules(tokens, ':root[data-platform="linux"]');
  expect(linux).toMatch(/--row-active-background:\s*Highlight;/);
  expect(linux).toMatch(/--row-active-text:\s*HighlightText;/);
  // No later Linux rule turns it back into a faint tint or neutral fill.
  expect(linux.slice(linux.indexOf("Highlight;"))).not.toMatch(/--row-active-background/);
});

test("high contrast keeps a non-colour edge on the active row", () => {
  const contrast = styles.slice(styles.indexOf("@media (prefers-contrast: more)"));
  expect(rules(contrast, '[role="option"][data-active]')).toMatch(/border-color:\s*var\(--active-edge\)/);
});

/** The chain's inputs for one look: the base layer, then that platform's block (rem at 16 px). */
function geometry(platform?: string): Record<string, number> {
  const px = (value: string, scope: Record<string, number>): number => {
    const v = value.trim();
    const ref = v.match(/^var\((--[\w-]+)\)$/);
    if (ref) return scope[ref[1]!]!;
    if (v.endsWith("rem")) return parseFloat(v) * 16;
    if (v.endsWith("px")) return parseFloat(v);
    throw new Error(`unexpected value ${v}`);
  };
  const scope: Record<string, number> = {};
  const blocks = [rules(tokens, ":root")];
  if (platform) blocks.push(rules(tokens, `:root[data-platform="${platform}"]`));
  for (const block of blocks) {
    for (const [, name, value] of block.matchAll(/(--(?:space-\d|inset|r-win|r-min|r-menu|menu-pad|row-pad|row-gap|tile-size|icon-glyph-size|field-h)):\s*([^;]+);/g)) {
      scope[name!] = px(value!, scope);
    }
  }
  return scope;
}

test("every inner radius follows one concentric chain per platform", () => {
  // The formulas are written once in the base layer; looks set only their inputs.
  const base = rules(tokens, ":root");
  expect(base).toMatch(/--r-row:\s*max\(var\(--r-min\), calc\(var\(--r-win\) - var\(--inset\)\)\);/);
  expect(base).toMatch(/--r-tile:\s*max\(var\(--r-min\), calc\(var\(--r-row\) - var\(--row-pad\)\)\);/);
  expect(base).toMatch(/--r-mi:\s*max\(var\(--r-min\), calc\(var\(--r-menu\) - var\(--menu-pad\)\)\);/);
  for (const name of ["--r-row", "--r-tile", "--r-mi"]) {
    for (const platform of ["windows", "linux"]) expect(rules(tokens, `:root[data-platform="${platform}"]`), `${platform} ${name}`).not.toContain(`${name}:`);
  }
  // docs/design.md, Corners: macOS before 26 (the native 12 pt corner), Windows 11, GNOME.
  const expected = { macos: [12, 8, 6, 6, 12, 6], windows: [8, 8, 4, 4, 8, 4], linux: [15, 8, 7, 6, 12, 6] };
  for (const [platform, [win, inset, row, tile, menu, item]] of Object.entries(expected)) {
    const g = geometry(platform === "macos" ? undefined : platform);
    const rRow = Math.max(g["--r-min"]!, g["--r-win"]! - g["--inset"]!);
    const rTile = Math.max(g["--r-min"]!, rRow - g["--row-pad"]!);
    const rMi = Math.max(g["--r-min"]!, g["--r-menu"]! - g["--menu-pad"]!);
    expect([g["--r-win"], g["--inset"], rRow, rTile, g["--r-menu"], rMi], platform).toEqual([win, inset, row, tile, menu, item]);
  }
  // The old literal radii are gone, so nothing bypasses the chain.
  expect(tokens).not.toMatch(/--(window|result|tile|field)-radius:/);
  expect(styles).not.toMatch(/var\(--(window|result|tile|field)-radius\)/);
});

test("the search text starts where row titles start, with the glyph over the tile column", () => {
  for (const platform of ["macos", "windows", "linux"]) {
    const g = geometry(platform === "macos" ? undefined : platform);
    const lead = g["--row-pad"]! + (g["--tile-size"]! - g["--icon-glyph-size"]!) / 2;
    const leadGap = (g["--tile-size"]! - g["--icon-glyph-size"]!) / 2 + g["--row-gap"]!;
    const textX = g["--inset"]! + lead + g["--icon-glyph-size"]! + leadGap;
    expect(textX, platform).toBe(g["--inset"]! + g["--row-pad"]! + g["--tile-size"]! + g["--row-gap"]!);
    expect(textX, platform).toBe(64);
    expect(g["--inset"]! + lead + g["--icon-glyph-size"]! / 2, platform).toBe(g["--inset"]! + g["--row-pad"]! + g["--tile-size"]! / 2);
  }
  expect(rules(styles, ".search-bar > svg")).toMatch(/margin-inline:\s*var\(--lead\) calc\(var\(--lead-gap\) - var\(--search-gap\)\)/);
  expect(rules(styles, '[role="option"]')).toMatch(/padding:\s*0 calc\(var\(--row-pad\) - var\(--border-width\)\)/);
  expect(rules(styles, '[role="option"]')).toMatch(/gap:\s*var\(--row-gap\)/);
  expect(rules(styles, ".results-area")).toMatch(/margin-block:\s*var\(--inset\);\s*padding-inline:\s*var\(--inset\)/);
  // The framed box's text start pays for its 1 px border.
  expect(rules(styles, ':root[data-platform="windows"] #search')).toMatch(/padding-inline-start:\s*calc\(var\(--lead\) \+ var\(--search-icon-width\) \+ var\(--lead-gap\) - var\(--border-width\)\)/);
  // Logical sides only, so a right-to-left picker mirrors.
  expect(rules(styles, "footer")).toMatch(/padding-inline:\s*var\(--footer-padding\)/);
});

test("tile targets stay centred when Compact shrinks the visible tile", () => {
  expect(rules(styles, ".result-tile .tile-button")).toMatch(/inset:\s*50% auto auto 50%;.*transform:\s*translate\(-50%, -50%\)/);
  expect(rules(styles, ':root[data-density="compact"] .result-tile .tile-button')).not.toMatch(/inset:/);
});

test("window radius follows each platform and macOS matches the native corner", () => {
  // macOS keeps the natively verified 12 pt corner until macOS 26 detection lands; the token must agree with it.
  const native = corners.match(/CORNER_RADIUS_POINTS: f64 = ([\d.]+);/)?.[1];
  expect(rules(tokens, ":root")).toMatch(new RegExp(`--r-win:\\s*${Number(native)}px`));
  expect(rules(tokens, ':root[data-platform="windows"]')).toMatch(/--r-win:\s*8px/);
  expect(rules(tokens, ':root[data-platform="windows"]')).toMatch(/--r-control:\s*4px/);
  expect(rules(tokens, ':root[data-platform="linux"]')).toMatch(/--r-win:\s*15px/);
});

test("per-OS overlay roles keep their shapes, easing, scrims and shadow-free contrast", () => {
  for (const [platform, ease, dialog] of [["windows", "0, 0, 0, 1", "--r-win"], ["linux", ".33, 1, .68, 1", "--r-menu"]]) {
    const look = rules(tokens, `:root[data-platform="${platform}"]`);
    expect(look).toContain(`--ease-out: cubic-bezier(${ease});`);
    expect(look).toContain(`--r-dialog: var(${dialog});`);
  }
  expect(rules(tokens, ':root[data-platform="windows"]')).toContain("--ease-in: cubic-bezier(1, 0, 1, 1);");
  expect(tokens).not.toMatch(/--(?:menu-shadow|motion-ease)\s*:/);
  expect(rules(tokens, ":root")).toContain("--scrim: rgb(0 0 0 / 30%);");
  for (const [platform, alpha] of [["macos", "20%"], ["linux", "25%"]]) expect(rules(tokens, `:root[data-platform="${platform}"]`)).toContain(`--scrim: light-dark(rgb(0 0 0 / ${alpha})`);
  const at = tokens.indexOf("@media (prefers-contrast: more), (forced-colors: active)");
  expect(at).toBeGreaterThan(0);
  for (const block of [rules(tokens.slice(at), ":root, :root[data-platform]"), rules(tokens, ':root[data-gallery-theme="contrast"]')]) {
    for (const level of ["card", "raised", "overlay", "dialog", "toast"]) expect(block).toContain(`--shadow-${level}: none;`);
    expect(block).toContain("--scrim: Canvas;");
  }
});

test("the Windows notice keeps its nominal shadow within the reserved gutter", () => {
  const notices = readFileSync(new FileURL("./notice.css", import.meta.url), "utf8");
  const gutter = Number(rules(notices, '.copied-notice[data-platform="windows"]').match(/margin:\s*([\d.]+)rem/)?.[1]) * 16;
  expect(gutter).toBeGreaterThan(0);
  const shadows = [...rules(tokens, ':root[data-platform="windows"]').matchAll(/--shadow-toast:\s*([^;]+);/g)];
  expect(shadows.length).toBeGreaterThan(0);
  for (const [, shadow] of shadows) {
    const layers = [...shadow!.matchAll(/0 (-?[\d.]+)(?:px)? ([\d.]+)px /g)];
    expect(layers.length).toBeGreaterThan(0);
    for (const [, offset, blur] of layers) expect(Math.abs(Number(offset)) + Number(blur) / 2).toBeLessThanOrEqual(gutter);
  }
});

test("menus and dialogs consume their shared elevation and separate shape roles", () => {
  const menus = readFileSync(new FileURL("./ui/Menu.css", import.meta.url), "utf8");
  const native = readFileSync(new FileURL("./nativeAppearance.css", import.meta.url), "utf8");
  expect(rules(menus, ".menu-popover")).toMatch(/box-shadow:\s*var\(--shadow-overlay\)/);
  expect(menus).not.toMatch(/--menu-shadow\s*:/);
  const dialog = rules(styles, ".keys-dialog");
  expect(dialog).toMatch(/border-radius:\s*var\(--r-dialog\)/);
  expect(dialog).toMatch(/box-shadow:\s*var\(--shadow-dialog\)/);
  expect(rules(styles, ".keys-backdrop")).toMatch(/background:\s*var\(--scrim\)/);
  expect(native).not.toMatch(/:is\(\.menu-popover,\s*\.keys-dialog\)\s*\{[^}]*border-radius:/);
});

test("scarlet marks only the citation: copied text and the shelf's quote mark", () => {
  const scarlet = [...styles.matchAll(/([^{}]+)\{[^{}]*var\(--citation\)[^{}]*\}/g)].map(([, head]) => head!.trim()).join(",");
  for (const target of [".copy-notice code", ".shelf-mark"]) {
    expect(scarlet, target).toContain(target);
  }
  // The shelf's output and chips use text colours (docs/design/preview.md).
  expect(scarlet).not.toMatch(/preview-text|\.chip/);
  // Selection, the pin and the main controls keep the system accent.
  expect(scarlet).not.toMatch(/button|\.search-bar|::before|\[role="option"\]\[data-active\]\s*(,|$)/);
  // Contrast and forced colours fall back to CanvasText.
  for (const media of ["@media (prefers-contrast: more)", "@media (forced-colors: active)"]) {
    expect(tokens.slice(tokens.indexOf(media))).toMatch(/--citation:\s*CanvasText/);
  }
});

test.each(["windows"])("meta text on the %s grey row stays at 4.5:1 in both themes", (platform) => {
  const hex = (value: string) => [1, 3, 5].map((i) => parseInt(value.slice(i, i + 2), 16) / 255);
  const luminance = (rgb: number[]) => rgb.map((c) => c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4)
    .reduce((sum, c, i) => sum + c * [0.2126, 0.7152, 0.0722][i]!, 0);
  // The look's own neutrals when it has them, otherwise the shared ones.
  const look = tokens.slice(tokens.indexOf(`:root[data-platform="${platform}"]:not([data-gallery-theme="contrast"]) {`));
  const pair = (token: string) => (look.slice(0, look.indexOf("}")).match(new RegExp(`${token}:\\s*light-dark\\((#[0-9a-f]{6}),\\s*(#[0-9a-f]{6})\\)`))
    ?? tokens.match(new RegExp(`${token}:\\s*light-dark\\((#[0-9a-f]{6}),\\s*(#[0-9a-f]{6})\\)`)))!.slice(1, 3).map(hex);
  const tint = Number(rules(tokens, `:root[data-platform="${platform}"]`).match(/--row-active-background:\s*color-mix\(in srgb, var\(--text\) (\d+)%/)?.[1]) / 100;
  const [surface, text, muted] = [pair("--surface"), pair("--text"), pair("--muted")];
  for (const mode of [0, 1]) {
    const row = surface[mode]!.map((c: number, i: number) => c * (1 - tint) + text[mode]![i]! * tint);
    const [lighter, darker] = [luminance(muted[mode]!), luminance(row)].sort((a, b) => b - a);
    expect((lighter! + .05) / (darker! + .05)).toBeGreaterThanOrEqual(4.5);
  }
});

test("an added row shows only its scarlet tile with a check: no wash, ring or scarlet key on the row", () => {
  const tile = rules(styles, '[role="option"][data-citation-selected] .result-tile');
  expect(tile).toMatch(/background:\s*var\(--citation-tile\)/);
  expect(tile).toMatch(/color:\s*var\(--on-citation\)/);
  expect(tokens).toMatch(/--on-citation:\s*#fff/);
  // Nothing outside contrast and forced colours paints the added row itself or its key.
  const outside = styles.slice(0, styles.indexOf("@media (prefers-contrast: more)"));
  for (const [, head, body] of outside.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const list = head!.replace(/\/\*[^]*?\*\//g, "").trim();
    if (!/data-citation-selected\](?!\))/.test(list)) continue;
    for (const selector of list.split(/\s*,\s*/)) {
      if (/\.result-tile$|\.tile-button$|\.type-icon$/.test(selector)) continue;
      throw new Error(`added-row style outside the tile: ${selector} { ${body} }`);
    }
  }
  expect(styles).not.toMatch(/data-citation-selected[^{]*\.result-key/);
  // High contrast inverts the tile on the active row instead of drawing a ring.
  const contrast = styles.slice(styles.indexOf("@media (prefers-contrast: more)"));
  expect(rules(contrast, "[data-active][data-citation-selected] .result-tile")).toMatch(/background:\s*var\(--row-active-text\)/);
  expect(rules(gallery, ':root[data-gallery-theme="contrast"] [data-active][data-citation-selected] .result-tile')).toMatch(/background:\s*var\(--row-active-text\)/);
});

/** WCAG relative luminance contrast of two #rrggbb colours. */
function contrast(a: string, b: string): number {
  const luminance = (hex: string) => {
    const [r, g, b2] = [1, 3, 5].map((i) => {
      const c = parseInt(hex.slice(i, i + 2), 16) / 255;
      return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    });
    return 0.2126 * r! + 0.7152 * g! + 0.0722 * b2!;
  };
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi! + 0.05) / (lo! + 0.05);
}

test("the added tile's white check keeps 4.5:1 and the tile stands 3:1 off the surface, light and dark", () => {
  const pair = (name: string) => {
    const match = tokens.match(new RegExp(`${name}:\\s*light-dark\\((#[0-9a-f]{6}),\\s*(#[0-9a-f]{6})\\)`, "i"));
    expect(match, name).not.toBeNull();
    return { light: match![1]!, dark: match![2]! };
  };
  const tile = pair("--citation-tile");
  const surface = pair("--surface");
  const check = tokens.match(/--on-citation:\s*(#[0-9a-f]{3,6})/i)![1]!.replace(/^#fff$/i, "#ffffff");
  for (const theme of ["light", "dark"] as const) {
    expect(contrast(check, tile[theme]), `check on tile, ${theme}`).toBeGreaterThanOrEqual(4.5);
    expect(contrast(tile[theme], surface[theme]), `tile on surface, ${theme}`).toBeGreaterThanOrEqual(3);
  }
});

test("the tile is a pointer-only control: no button, no Tab stop, hidden from the accessibility tree", async () => {
  mockIPC(() => { throw new Error("Native commands must not run here"); }, { shouldMockEvents: true });
  const state = galleryStates.find((candidate) => candidate.id === "many-results")!;
  render(<App {...galleryProvider(state)} />);
  await waitFor(() => expect(document.querySelectorAll("#results .result-tile").length).toBe(20));
  for (const option of document.querySelectorAll('#results [role="option"]')) {
    expect(option.querySelector("button, a, input, [tabindex]")).toBeNull();
    expect(option.querySelector(".tile-button")!.getAttribute("aria-hidden")).toBe("true");
  }
  // Each of the seven types draws its own icon.
  const icons = new Set([...document.querySelectorAll("#results .type-icon")].map((svg) => svg.innerHTML));
  expect(icons.size).toBe(7);
});

test("no active row has an edge or ring outside contrast and forced colours", () => {
  const outside = styles.slice(0, styles.indexOf("@media (prefers-contrast: more)"));
  for (const [, head, body] of outside.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (!/\[data-active\]/.test(head!) || /data-citation-selected/.test(head!)) continue;
    expect(body, head).not.toMatch(/border(-color)?:|box-shadow:\s*(?!none)|outline:/);
  }
  expect(tokens).not.toMatch(/--row-active-edge/);
  // Linux falls back to the system pair without color-mix.
  const plain = tokens.slice(0, tokens.indexOf("@supports (background: color-mix"));
  const linuxPlain = rules(plain, ':root[data-platform="linux"]');
  expect(linuxPlain).toMatch(/--row-active-background:\s*Highlight;/);
  expect(linuxPlain).not.toMatch(/:\s*color-mix/);
});

test("the framed-box icon overlap moves only the search glyph, never the pin icon", async () => {
  mockIPC(() => { throw new Error("Native commands must not run here"); }, { shouldMockEvents: true });
  const state = galleryStates.find((candidate) => candidate.id === "many-results")!;
  render(<App {...galleryProvider(state)} />);
  await screen.findAllByRole("option");
  const searchIcon = document.querySelector(".search-bar")!.firstElementChild;
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "P", code: "KeyP", ctrlKey: true, shiftKey: true });
  const pinIcon = document.querySelector(".pin-button svg");
  expect(searchIcon?.tagName.toLowerCase()).toBe("svg");
  expect(pinIcon).toBeTruthy();
  const overlap = [...styles.matchAll(/([^{}]+)\{[^{}]*margin-inline:[^{}]*search-icon-width[^{}]*\}/g)];
  expect(overlap).toHaveLength(1);
  const selector = overlap[0]![1]!.replace(/\/\*[^]*?\*\//g, "").trim();
  for (const platform of ["windows", "linux"]) {
    setPlatform(platform === "windows" ? "Win32" : "Linux x86_64");
    expect([...document.querySelectorAll(selector)], platform).toEqual([searchIcon]);
    expect(pinIcon!.matches(selector), platform).toBe(false);
  }
});

test("the framed-box icon overlap moves only the search glyph, never the pin icon", async () => {
  mockIPC(() => { throw new Error("Native commands must not run here"); }, { shouldMockEvents: true });
  const state = galleryStates.find((candidate) => candidate.id === "many-results")!;
  render(<App {...galleryProvider(state)} />);
  await screen.findAllByRole("option");
  const searchIcon = document.querySelector(".search-bar")!.firstElementChild;
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "P", code: "KeyP", ctrlKey: true, shiftKey: true });
  const pinIcon = document.querySelector(".pin-button svg");
  expect(searchIcon?.tagName.toLowerCase()).toBe("svg");
  expect(pinIcon).toBeTruthy();
  const overlap = [...styles.matchAll(/([^{}]+)\{[^{}]*margin-inline:[^{}]*search-icon-width[^{}]*\}/g)];
  expect(overlap).toHaveLength(1);
  const selector = overlap[0]![1]!.replace(/\/\*[^]*?\*\//g, "").trim();
  for (const platform of ["windows", "linux"]) {
    setPlatform(platform === "windows" ? "Win32" : "Linux x86_64");
    expect([...document.querySelectorAll(selector)], platform).toEqual([searchIcon]);
    expect(pinIcon!.matches(selector), platform).toBe(false);
  }
});


test("focus rings are keyboard-only, 2 px with a gap, in each platform's colour", () => {
  expect(rules(styles, ":focus-visible:not(.ui-button)")).toMatch(/outline:\s*var\(--focus-ring-width\) solid var\(--focus-ring-color\);\s*outline-offset:\s*var\(--focus-ring-offset\);\s*box-shadow:\s*var\(--focus-ring-shadow\)/);
  // No ring is drawn on plain :focus or :focus-within, which a click also triggers.
  for (const [, head, body] of styles.slice(0, styles.indexOf("@media (prefers-contrast: more)")).matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (/:focus(-within)?\b(?!-visible)/.test(head!)) expect(body, head).not.toMatch(/outline:\s*var/);
  }
  const base = rules(tokens, ":root");
  expect(base).toMatch(/--focus-ring-color:\s*var\(--accent\)/); // macOS
  expect(base).toMatch(/--focus-ring-width:\s*2px/);
  expect(base).toMatch(/--focus-ring-offset:\s*1px/);
  const windows = rules(tokens, ':root[data-platform="windows"]');
  expect(windows).toMatch(/--focus-ring-color:\s*var\(--text\)/);
  expect(windows).toMatch(/--focus-ring-shadow:\s*0 0 0 1px var\(--surface\)/);
  expect(rules(tokens, ':root[data-platform="linux"]')).toMatch(/--focus-ring-color:\s*color-mix\(in srgb, var\(--accent\) 50%, var\(--text\)\)/);
  // The inner Windows stroke touches the outer one, as in Fluent: offset = stroke width.
  expect(windows).not.toMatch(/--focus-ring-offset/);
  expect(windows).toMatch(/--focus-ring-shadow:\s*0 0 0 1px/);
  // Contrast and forced colours: a solid CanvasText ring on every platform.
  for (const media of ["@media (prefers-contrast: more)", "@media (forced-colors: active)"]) {
    expect(rules(tokens.slice(tokens.indexOf(media)), ":root[data-platform]"), media).toMatch(/--focus-ring-color:\s*CanvasText/);
  }
});

test("the search box never shows a ring, except with contrast or forced colours", () => {
  const outside = styles.slice(0, styles.indexOf("@media (prefers-contrast: more)"));
  expect(rules(outside, "#search:focus-visible")).toMatch(/outline:\s*none/);
  const windows = rules(outside, ':root[data-platform="windows"] #search:focus');
  expect(windows).toMatch(/outline:\s*none/);
  expect(windows).toMatch(/border-bottom-color:\s*var\(--accent\)/);
  for (const media of ["@media (prefers-contrast: more)", "@media (forced-colors: active)"]) {
    const block = styles.slice(styles.indexOf(media), styles.indexOf("\n}\n", styles.indexOf(media)));
    expect(rules(block, "#search:focus-visible"), media).toMatch(/outline:\s*var\(--focus-ring-width\) solid/);
    expect(rules(block, ":root[data-platform] #search:focus"), media).toMatch(/outline:\s*var\(--focus-ring-width\) solid/);
  }
});

test("under forced colours an active added row differs from an active row in its computed style", async () => {
  mockIPC(() => { throw new Error("Native commands must not run here"); }, { shouldMockEvents: true });
  // jsdom evaluates no media queries, so apply the forced-colours rules as forced mode would.
  const start = styles.indexOf("@media (forced-colors: active) {");
  const forced = styles.slice(start + "@media (forced-colors: active) {".length, styles.indexOf("\n}\n", start));
  const sheet = document.createElement("style");
  sheet.textContent = forced;
  document.head.append(sheet);
  try {
    const state = galleryStates.find((candidate) => candidate.id === "many-results")!;
    render(<App {...galleryProvider(state)} />);
    const row = (n: number) => document.querySelectorAll<HTMLElement>('#results [role="option"]')[n]!;
    await waitFor(() => expect(row(2)).toBeTruthy());
    const box = screen.getByLabelText("Search references");
    // Add the first two rows, then look at row 1 (active and added) and row 2 (active only).
    fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
    fireEvent.keyDown(box, { key: "ArrowDown" });
    fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
    await waitFor(() => expect(row(1).hasAttribute("data-citation-selected")).toBe(true));
    expect(row(1).hasAttribute("data-active")).toBe(true);
    const tileOf = (n: number) => row(n).querySelector<HTMLElement>(".result-tile")!;
    const activeAdded = getComputedStyle(tileOf(1)).backgroundColor;
    // The added tile keeps its check mark, the cue that is not colour.
    expect(tileOf(1).querySelector(".add-glyph path")!.getAttribute("d")).toMatch(/^m5 10\.5/);
    expect(getComputedStyle(tileOf(0)).backgroundColor).toBe(getComputedStyle(probeColor("CanvasText")).color);
    fireEvent.keyDown(box, { key: "ArrowDown" });
    await waitFor(() => expect(row(2).hasAttribute("data-active")).toBe(true));
    expect(row(2).hasAttribute("data-citation-selected")).toBe(false);
    const activeOnly = getComputedStyle(tileOf(2)).backgroundColor;
    expect(tileOf(2).querySelector(".add-glyph path")!.getAttribute("d")).toMatch(/^M10 5/);
    const system = (name: string) => getComputedStyle(probeColor(name)).color;
    expect(system("HighlightText")).not.toBe(system("Highlight"));
    // The active added tile is the text colour paired with the Highlight fill.
    expect(activeAdded).toBe(system("HighlightText"));
    expect(activeOnly).toBe(system("Highlight"));
  } finally {
    sheet.remove();
    document.querySelectorAll("[data-probe]").forEach((probe) => probe.remove());
  }
});

function probeColor(name: string): HTMLElement {
  const probe = document.createElement("span");
  probe.dataset.probe = "";
  probe.style.color = name;
  document.body.append(probe);
  return probe;
}

test("the search has a quiet clear glyph, and contrast restores its ring", () => {
  expect(tokens).toMatch(/--focus-ring-width:\s*2px/);
  const clear = rules(styles, "#search::-webkit-search-cancel-button");
  expect(clear).toMatch(/appearance:\s*none/);
  expect(clear).toMatch(/display:\s*none/);
  const glyph = rules(styles, ".search-clear::before");
  expect(glyph).toMatch(/width:\s*var\(--icon-size\)/);
  expect(glyph).toMatch(/background:\s*currentColor/);
  expect(glyph).toMatch(/mask:\s*var\(--clear-glyph\)/);
  expect(tokens).toMatch(/--control-min-size:\s*2rem/);
  expect(tokens).toMatch(/--icon-size:\s*1rem/);
  const forced = styles.slice(styles.indexOf("@media (forced-colors: active)"));
  expect(rules(forced, ".search-clear::before")).toMatch(/background:\s*ButtonText/);
  // The gallery's contrast preview restores the ring as prefers-contrast does.
  expect(rules(gallery, ':root[data-gallery-theme="contrast"] #search:focus-visible'))
    .toMatch(/outline:\s*var\(--focus-ring-width\) solid var\(--focus-ring-color\)/);
});

test("under forced colours the active row opts out of the backplate so HighlightText stays on Highlight", () => {
  const start = styles.indexOf("@media (forced-colors: active)");
  const forced = styles.slice(start, styles.indexOf("\n}\n", start));
  // Without this, Chromium and WebView2 paint a Canvas backplate behind the
  // HighlightText label: white on white in the default palette.
  expect(rules(forced, '[role="option"][data-active]')).toMatch(/forced-color-adjust:\s*none/);
  // Every colour that row then paints is a system colour from the forced palette.
  const forcedTokens = tokens.slice(tokens.indexOf("@media (forced-colors: active)"));
  expect(forcedTokens).toMatch(/--row-active-background:\s*Highlight;/);
  expect(forcedTokens).toMatch(/--row-active-text:\s*HighlightText;/);
  expect(rules(forced, '[role="option"][data-active][data-citation-selected] .result-tile')).toMatch(/background:\s*HighlightText/);
  expect(rules(forced, ".result-tile")).toMatch(/forced-color-adjust:\s*none/);
  expect(rules(forced, ':root[data-platform="windows"] [role="option"][data-active]::before')).toMatch(/background:\s*HighlightText/);
});

// Contrast against the surface (WCAG 1.4.11) with each platform's default
// accent: macOS system blue, Adwaita blue. A user's own accent may differ.
const hex = (value: string) => [1, 3, 5].map((i) => parseInt(value.slice(i, i + 2), 16) / 255);
const luminance = (rgb: number[]) => rgb.map((c) => c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4)
  .reduce((sum, c, i) => sum + c * [0.2126, 0.7152, 0.0722][i]!, 0);
const ratio = (a: number[], b: number[]) => { const [l, d] = [luminance(a), luminance(b)].sort((x, y) => y - x); return (l! + .05) / (d! + .05); };
const mix = (a: number[], b: number[], share: number) => a.map((c, i) => c * share + b[i]! * (1 - share));
const pair = (token: string) => tokens.match(new RegExp(`${token}:\\s*light-dark\\((#[0-9a-f]{6}),\\s*(#[0-9a-f]{6})\\)`))!.slice(1, 3).map(hex);

test("macOS and Linux focus rings reach 3:1 on the surface with the default accent", () => {
  const [surface, text] = [pair("--surface"), pair("--text")];
  const accents = { macos: ["#007aff", "#0a84ff"], linux: ["#3584e4", "#3584e4"] };
  for (const mode of [0, 1]) {
    expect(ratio(hex(accents.macos[mode]!), surface[mode]!), `macOS ${mode}`).toBeGreaterThanOrEqual(3);
    // Linux: the accent mixed half toward the text colour, as the token says.
    const ring = mix(hex(accents.linux[mode]!), text[mode]!, .5);
    expect(ratio(ring, surface[mode]!), `Linux ${mode}`).toBeGreaterThanOrEqual(3);
  }
});

test("the Linux active row stands out 3:1 from neighbouring rows with Adwaita's accent", () => {
  // GTK's selection colour is the accent (Adwaita #3584e4); neighbours are the surface.
  const surface = pair("--surface");
  for (const mode of [0, 1]) expect(ratio(hex("#3584e4"), surface[mode]!), `mode ${mode}`).toBeGreaterThanOrEqual(3);
  // libadwaita's 25% tint would not: the reason the full selection colour is used.
  expect(ratio(mix(hex("#3584e4"), surface[0]!, .25), surface[0]!)).toBeLessThan(3);
});


test("Windows and GNOME use their own type and neutrals, never under contrast modes", () => {
  expect(rules(tokens, ':root[data-platform="windows"]')).toMatch(/--font-family:\s*"Segoe UI Variable Text", "Segoe UI Variable", "Segoe UI", system-ui, sans-serif/);
  expect(rules(tokens, ':root[data-platform="linux"]')).toMatch(/--font-family:\s*system-ui, "Adwaita Sans", Cantarell, sans-serif/);
  for (const [platform, surface] of [["windows", "#f3f3f3, #202020"], ["linux", "#fafafb, #222226"]] as const) {
    const head = `:root[data-platform="${platform}"]:not([data-gallery-theme="contrast"]) {`;
    const at = tokens.indexOf(head);
    expect(at, platform).toBeGreaterThan(0);
    expect(tokens.slice(at, tokens.indexOf("}", at))).toContain(`--surface: light-dark(${surface})`);
    // Inside the no-preference guard, so increased contrast and forced colours keep Canvas.
    const before = tokens.slice(0, at);
    expect(before.lastIndexOf("@media (prefers-contrast: no-preference) and (forced-colors: none)"), platform).toBeGreaterThan(before.lastIndexOf("}\n}"));
  }
});

test("secondary text keeps 4.5:1 on each look's surface", () => {
  const hex = (value: string) => [1, 3, 5].map((i) => parseInt(value.slice(i, i + 2), 16) / 255);
  const luminance = (rgb: number[]) => rgb.map((c) => c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4)
    .reduce((sum, c, i) => sum + c * [0.2126, 0.7152, 0.0722][i]!, 0);
  const contrast = (a: string, b: string) => { const [l, d] = [luminance(hex(a)), luminance(hex(b))].sort((x, y) => y - x); return (l! + .05) / (d! + .05); };
  const value = (block: string, token: string) => block.match(new RegExp(`${token}:\\s*light-dark\\((#[0-9a-f]{6}),\\s*(#[0-9a-f]{6})\\)`))?.slice(1, 3);
  const base = tokens.slice(tokens.indexOf("@supports (color: light-dark(white, black))"));
  for (const platform of ["windows", "linux"]) {
    const at = tokens.indexOf(`:root[data-platform="${platform}"]:not([data-gallery-theme="contrast"]) {`);
    const block = tokens.slice(at, tokens.indexOf("}", at));
    const surface = value(block, "--surface")!, text = value(block, "--text")!, muted = value(block, "--muted") ?? value(base, "--muted")!;
    for (const mode of [0, 1]) {
      expect(contrast(muted[mode]!, surface[mode]!), `${platform} muted`).toBeGreaterThanOrEqual(4.5);
      expect(contrast(text[mode]!, surface[mode]!), `${platform} text`).toBeGreaterThanOrEqual(7);
    }
  }
});

test("spacing tokens, direct gaps/padding and resolved formulas keep the 4 px grid", () => {
  const off: string[] = [];
  for (const platform of ["macos", "windows", "linux"]) for (const mode of ["normal", "more", "forced", "gallery"]) for (const coarse of [false, true]) {
    const blocks = tokenBlocks(appSheets, platform, mode, true, false, false, coarse), scopes = tokenScopes(blocks);
    for (const block of blocks) for (const [name, value] of block.values) {
      if (/^padding-(top|right|bottom|left)$/.test(name) && block.values.some(([key]) => key === "padding")) continue;
      if (!/^(--[\w-]*(padding|gap|inset)|gap|row-gap|column-gap|padding[\w-]*)$/.test(name) || opticalSpacing(block, name, value)) continue;
      const resolved = resolveToken(value, scopes.get(block.scope)!);
      if (resolved === null) { off.push(`${name}: unresolved`); continue; }
      for (const part of cssParts(resolved)) {
        const px = gridPixels(part);
        if (px !== null && Math.abs(px / 4 - Math.round(px / 4)) > 1e-8) off.push(`${platform}/${mode} ${block.selector} ${name}: ${value} = ${px}px`);
      }
    }
  }
  expect([...new Set(off)]).toEqual([]);
});

test("unused tokens are gone and the spec's line heights apply", () => {
  for (const name of ["--forced-selected-offset", "--key-padding", "--key-radius", "--button-background", "--button-hover-background", "--button-edge", "--button-radius", "--button-padding", "--hover-background"]) expect(tokens, name).not.toContain(`${name}:`);
  expect(rules(styles, ".results-heading")).toMatch(/line-height:\s*var\(--type-label-line\)/);
  expect(rules(styles, "#search")).toMatch(/line-height:\s*var\(--search-line\)/);
});

test("shared default buttons are at least 32 px, with on-grid padding", () => {
  const button = rules(buttons, ".ui-button[data-variant]");
  expect(button).toMatch(/min-height:\s*var\(--control-min-size\)/);
  expect(tokens).toMatch(/--control-min-size:\s*2rem/);
  expect(button).toMatch(/padding:\s*0 var\(--space-3\)/);
});

test("dividers are quiet separators, distinct from 3:1 control edges", () => {
  for (const selector of [".search-bar", ':root[data-platform="windows"] footer', ':root[data-platform="linux"] footer', ".copy-notice"]) {
    expect(rules(styles, selector), selector).toMatch(/border-(top|bottom):\s*var\(--border-width\) solid var\(--separator\)/);
  }
  // Windows: Fluent's divider stroke; elsewhere Adwaita's text colour at 15%; macOS its 10% hairline.
  expect(tokens).toMatch(/:root\[data-platform="windows"\] \{[^}]*--separator:\s*light-dark\(rgb\(0 0 0 \/ 6%\), rgb\(255 255 255 \/ 8%\)\)/);
  expect(tokens).toMatch(/:root \{ --separator:\s*color-mix\(in srgb, currentColor 15%, transparent\); \}/);
  // Contrast modes give every divider and edge the full system colour, and
  // override the Windows look, which therefore comes first.
  for (const media of ["@media (prefers-contrast: more)", "@media (forced-colors: active)"]) {
    expect(rules(tokens.slice(tokens.indexOf(media)), ":root[data-platform]"), media).toMatch(/--separator:\s*CanvasText/);
    expect(tokens.indexOf('--separator: light-dark(rgb(0 0 0 / 6%)'), media).toBeLessThan(tokens.indexOf(media));
  }
});

test("the search box draws one underline: the header rule never changes on focus", () => {
  const plain = styles.slice(0, styles.indexOf("@media (prefers-contrast: more)"));
  expect(plain).not.toMatch(/\.search-bar:focus-within/);
  expect(tokens).not.toMatch(/--search-active-border/);
});

test("buttons and key caps are fills without an outline except in contrast modes", () => {
  expect(rules(buttons, ".ui-button[data-variant]")).toMatch(/border:\s*var\(--border-width\) solid var\(--stroke-control\)/);
  const kbd = rules(styles, "kbd");
  expect(kbd).toMatch(/background:\s*var\(--fill-control\)/);
  expect(kbd).toMatch(/border:\s*var\(--border-width\) solid var\(--stroke-control\)/);
  expect(kbd).toMatch(/height:\s*var\(--key-cap\)/);
  expect(rules(tokens, ":root")).toMatch(/--stroke-control:\s*var\(--clear\)/);
  for (const media of ["@media (prefers-contrast: more)", "@media (forced-colors: active)"]) {
    expect(rules(tokens.slice(tokens.indexOf(media)), ":root[data-platform]"), media).toMatch(/--stroke-control:\s*(CanvasText|ButtonText)/);
  }
});

test("the shelf is two 40 px lines of 24 px chips and one output line above the footer", () => {
  expect(rules(tokens, ":root")).toMatch(/--shelf-line:\s*2\.5rem/);
  expect(rules(tokens, ":root")).toMatch(/--chip-height:\s*1\.5rem/);
  expect(rules(styles, ".shelf-chips")).toMatch(/height:\s*var\(--shelf-line\)/);
  expect(rules(styles, ".shelf-output")).toMatch(/height:\s*var\(--shelf-line\)/);
  // The output is one line; the full text is its tooltip.
  expect(rules(styles, ".shelf-output .preview-text, .shelf-output p")).toMatch(/white-space:\s*nowrap/);
  // Chips are fills without an outline, except in contrast modes.
  const edge = styles.lastIndexOf("@media (prefers-contrast: more), (forced-colors: active) {");
  expect(rules(styles.slice(0, edge), ".chip")).toMatch(/background:\s*var\(--chip-fill\)/);
  expect(rules(styles.slice(0, edge), ".chip")).not.toMatch(/border:/);
  const contrast = styles.slice(edge);
  expect(rules(contrast, ".shelf, .chip")).toMatch(/border:\s*var\(--border-width\) solid CanvasText/);
  // Windows draws a layer card with a stroke; GNOME a flat band under one separator.
  expect(rules(styles, ':root[data-platform="windows"] .shelf')).toMatch(/border:\s*var\(--border-width\) solid var\(--stroke-control\)/);
  expect(rules(styles, ':root[data-platform="linux"] .shelf')).toMatch(/border-top:\s*var\(--border-width\) solid var\(--separator\)/);
  const shelf = readFileSync(new FileURL("./features/picker/CitationShelf.css", import.meta.url), "utf8");
  const forced = shelf.slice(shelf.indexOf("@media (forced-colors: active)"));
  expect(rules(forced, ".chip-remove::before")).toMatch(/forced-color-adjust:\s*none/);
  expect(rules(forced, ".chip-remove::before")).toMatch(/background:\s*ButtonText/);
});

test("a wrapped footer keeps breathing room above and below its lines", () => {
  // The footer has no block padding of its own (its 40 px is a min-height), so
  // the library bar carries it: 1 px divider + 3 + 32 px More + 4 = 40 px, and
  // wrapped lines never touch the divider or the window's bottom corners.
  expect(rules(styles, ".library-bar")).toMatch(/padding-block:\s*var\(--inset\)/);
  const buttons = readFileSync(new FileURL("./ui/Button.css", import.meta.url), "utf8");
  expect(rules(buttons, '.ui-button[data-size="small"]')).toMatch(/min-height:\s*1\.5rem/);
  expect(rules(tokens, ":root")).toMatch(/--control-min-size:\s*2rem/);
  expect(rules(tokens, ":root")).toMatch(/--footer-height:\s*2\.5rem/);
});

test("the thicker active-row edge in increased contrast does not move the row's text", () => {
  const contrast = styles.slice(styles.indexOf("@media (prefers-contrast: more)"));
  expect(rules(contrast, '[role="option"][data-active]')).toMatch(/padding-inline:\s*calc\(var\(--row-pad\) - var\(--contrast-border-width\)\)/);
});

test("the alias-only migration exposes the complete design-system role inventory", () => {
  const roles = `surface-window surface-raised surface-overlay surface-field
    fill-control fill-control-hover fill-control-pressed fill-subtle-hover fill-subtle-pressed fill-disabled
    text-tertiary text-disabled text-on-accent accent-text row-inactive-background
    stroke-control stroke-field stroke-overlay scrim status-info
    shadow-card shadow-raised shadow-overlay shadow-dialog shadow-toast
    ease-in ease-out motion-exit z-content z-chrome z-overlay z-tooltip z-modal z-toast overlay-margin
    r-control r-key r-card r-dialog r-full`.split(/\s+/);
  for (const role of roles) expect(tokens, role).toMatch(new RegExp(`--${role}:`));
  // This is the unscaled 16 px/rem baseline, not Larger/system-scaled geometry.
  for (const [step, px] of [[1, 4], [2, 8], [3, 12], [4, 16], [5, 20], [6, 24], [8, 32], [10, 40]]) {
    expect(Number(tokens.match(new RegExp(`--space-${step}:\\s*([\\d.]+)rem;`))?.[1]) * 16).toBe(px);
  }
  for (const old of ["focus", "focus-width", "focus-offset", "key-radius", "key-padding"]) {
    expect(tokens).not.toMatch(new RegExp(`--${old}:`));
  }
});

// CSSOM audits the live root, menu, notice and shelf scopes. Synthetic local
// fixtures also exercise inheritance and pointer modes without pooling declarations.
// This bounded dependency audit is not a CSS cascade/specificity interpreter;
// browser comparisons verify the actual winning values and rendered appearance.
const appSheets = Object.fromEntries(readdirSync(new FileURL(".", import.meta.url), { recursive: true })
  .filter((file) => file.endsWith(".css")).map((file) => [file.replaceAll("\\", "/"), readFileSync(new FileURL(file.replaceAll("\\", "/"), import.meta.url), "utf8")]));
const parsedSheets = new Map<string, CSSRuleList>();
type TokenBlock = { file: string; selector: string; values: [string, string][]; scope: string };
function tokenBlocks(sheets: Record<string, string>, platform = "macos", mode = "normal", advanced = true, material = false, notice = false, coarse = false) {
  const doc = document.implementation.createHTMLDocument();
  doc.documentElement.dataset.platform = platform;
  if (notice) doc.documentElement.classList.add("notice-window");
  if (mode === "gallery") doc.documentElement.dataset.galleryTheme = "contrast";
  if (material) doc.documentElement.dataset.nativeAppearance = "vibrancy";
  const blocks: TokenBlock[] = [];
  const media = (query: string) => query.split(",").some((part) => [...part.matchAll(/\((prefers-[\w-]+|forced-colors|pointer):\s*([\w-]+)\)/g)].every(([, feature, value]) =>
    value === ({ "prefers-contrast": mode === "more" ? "more" : "no-preference", "forced-colors": mode === "forced" ? "active" : "none", "pointer": coarse ? "coarse" : "fine", "prefers-reduced-motion": mode === "reduced" ? "reduce" : "no-preference", "prefers-reduced-transparency": mode === "transparency" ? "reduce" : "no-preference" } as Record<string, string>)[feature!]));
  for (const [file, css] of Object.entries(sheets)) {
    if (!parsedSheets.has(css)) {
      const sheet = document.createElement("style");
      sheet.textContent = css.replace(/@import[^;]+;/g, ""); document.head.append(sheet);
      parsedSheets.set(css, sheet.sheet!.cssRules); sheet.remove();
    }
    const visit = (rules: CSSRuleList) => { for (const rule of Array.from(rules)) {
      if (rule.type === 4 && !media((rule as CSSMediaRule).conditionText)) continue;
      if (rule.type === 12 && ((rule as CSSSupportsRule).conditionText.startsWith("not ") ? advanced : !advanced)) continue;
      if ("cssRules" in rule) visit((rule as CSSGroupingRule).cssRules);
      if (!("style" in rule)) continue;
      const styled = rule as CSSStyleRule;
      const selector = styled.selectorText ?? "@keyframe";
      const active = selector.split(/,(?![^()]*\))/).some((part) => {
        const prefix = part.trim().match(/^:root\S*/)?.[0];
        const look = part.match(/data-platform="(\w+)"/)?.[1];
        return (!prefix || doc.documentElement.matches(prefix)) && (!look || look === platform);
      });
      if (!active) continue;
      const scope = /\.ui-button\[data-pointer-only\]/.test(selector) ? "pointer-button" : /\.search-field|\.search-clear|#search/.test(selector) ? "search" : /\.chip-group(?:\b|-)/.test(selector) ? "chip-group" : /\.shelf(?:\b|-)/.test(selector) ? "shelf" : (/\.menu-|\.toolbar-menu/.test(selector) || (selector === "@keyframe" && file.endsWith("Menu.css"))) && !/^:root[^ ]*$/.test(selector) ? "menu" : file.endsWith("notice.css") && !selector.startsWith(":root") ? "notice" : "root";
      const values: [string, string][] = Array.from(styled.style).map((name) => [name, styled.style.getPropertyValue(name)]);
      if (scope === "root" && values.some(([name]) => name.startsWith("--")) && !selector.split(/,(?![^()]*\))/).every((part) => /^:root\S*$/.test(part.trim()))) throw new Error(`Unmodeled token scope: ${selector}`);
      blocks.push({ file, selector, scope, values });
      if (scope === "menu" && selector.includes(".keys-dialog")) blocks.push({ file, selector, scope: "root", values });
    } };
    visit(parsedSheets.get(css)!);
  }
  return blocks;
}

function cssParts(value: string, comma = false) {
  let depth = 0, start = 0; const parts: string[] = [];
  for (let i = 0; i < value.length; i++) {
    if (value[i] === "(") depth++; if (value[i] === ")") depth--;
    if (!depth && (comma ? value[i] === "," : /\s/.test(value[i]!))) { if (i > start) parts.push(value.slice(start, i)); start = i + 1; }
  }
  if (start < value.length || comma) parts.push(value.slice(start)); return parts;
}
const references = (value: string) => [...value.matchAll(/var\(\s*(--[\w-]+)/g)].map((match) => match[1]!);
function hasCycle(name: string, definitions: Map<string, string>, path: string[] = []): boolean {
  return path.includes(name) || references(definitions.get(name) ?? "").some((next) => hasCycle(next, definitions, [...path, name]));
}
function resolveToken(value: string, definitions: Map<string, string>): string | null {
  const start = value.indexOf("var("); if (start < 0) return value;
  let end = start + 4, depth = 1;
  while (end < value.length && depth) { if (value[end] === "(") depth++; if (value[end] === ")") depth--; end++; }
  if (depth) return null;
  const [name, ...fallback] = cssParts(value.slice(start + 4, end - 1), true).map((part) => part.trim());
  const raw = definitions.get(name!);
  const replacement = raw === undefined || hasCycle(name!, definitions) ? (fallback.length ? resolveToken(fallback.join(","), definitions) : null) : resolveToken(raw, definitions);
  return replacement === null ? null : resolveToken(value.slice(0, start) + replacement + value.slice(end), definitions);
}
function tokenScopes(blocks: TokenBlock[]) {
  const raw = (scope: string) => new Map(blocks.filter((block) => block.scope === scope).flatMap((block) => block.values.filter(([name]) => name.startsWith("--"))));
  const root = raw("root");
  // Inherited custom properties resolve on the parent, before child overrides.
  const inherited = [...root].map(([name, value]) => [name, resolveToken(value, root) ?? value] as [string, string]);
  return new Map([ ["root", root], ...["menu", "notice", "shelf", "chip-group", "search", "pointer-button"].map((scope) => [scope, new Map([...inherited, ...raw(scope)])] as const) ]);
}

test.each(["macos", "windows", "linux"])("every application var resolves on %s in its active mode, without alias cycles", (platform) => {
  expect(Object.keys(appSheets).length).toBeGreaterThanOrEqual(8);
  expect(tokenBlocks(appSheets).flatMap((block) => block.values).length).toBeGreaterThan(500);
  const errors: string[] = [];
  for (const mode of ["normal", "more", "forced", "gallery", "reduced", "transparency"]) for (const advanced of [true, false]) for (const material of [false, true]) for (const notice of [false, true]) for (const coarse of [false, true]) {
    const blocks = tokenBlocks(appSheets, platform, mode, advanced, material, notice, coarse), scopes = tokenScopes(blocks);
    for (const block of blocks) for (const [name, value] of block.values) {
      const definitions = scopes.get(block.scope)!;
      if ((name.startsWith("--") && hasCycle(name, definitions)) || resolveToken(value, definitions) === null) errors.push(`${platform}/${mode}/${advanced}/${material}/${notice}/${coarse} ${block.file} ${block.selector} ${name}: ${value}`);
    }
  }
  expect([...new Set(errors)]).toEqual([]);
});

test("the var audit distinguishes fallbacks, inactive definitions and cycles including fallbacks", () => {
  const definitions = new Map([["--a", "var(--b)"], ["--b", "4px"]]);
  expect(resolveToken("var(--missing)", definitions)).toBeNull();
  expect(resolveToken("var(--missing,)", definitions)).toBe("");
  expect(resolveToken("var(--missing, var(--also-missing, 8px))", definitions)).toBe("8px");
  expect(resolveToken("var(--missing, var(--also-missing))", definitions)).toBeNull();
  expect(resolveToken("var(--a, var(--missing))", definitions)).toBe("4px");
  for (const value of ["var(--a)", "var(--missing, var(--a))", "var(--b, var(--a))"]) expect(hasCycle("--a", new Map([...definitions, ["--a", value]]))).toBe(true);
  const inactive = tokenScopes(tokenBlocks({ fixture: ':root[data-platform="macos"] { --only-here: 4px; } @media (prefers-contrast: more) { :root { --contrast-only: 8px; } }' }, "windows")).get("root")!;
  expect(resolveToken("var(--only-here)", inactive)).toBeNull();
  expect(resolveToken("var(--contrast-only)", inactive)).toBeNull();
  expect(() => tokenBlocks({ fixture: ".unmodeled { --local: 4px; }" })).toThrow("Unmodeled token scope");
  const shelf = tokenScopes(tokenBlocks({ fixture: ":root { --value: 4px; } .shelf { --value: 8px; }" }));
  expect(resolveToken("var(--value)", shelf.get("root")!)).toBe("4px");
  expect(resolveToken("var(--value)", shelf.get("shelf")!)).toBe("8px");
  const chips = tokenScopes(tokenBlocks({ fixture: ":root { --value: 4px; } .chip-group { --value: 12px; }" }));
  expect(resolveToken("var(--value)", chips.get("root")!)).toBe("4px");
  expect(resolveToken("var(--value)", chips.get("chip-group")!)).toBe("12px");
  expect(() => tokenBlocks({ fixture: ":root .unmodeled { --local: 4px; }" })).toThrow("Unmodeled token scope");
  const overrides = { fixture: ':root { --a: 4px; } :root[data-platform="windows"] { --a: var(--b); --b: var(--a); }' };
  expect(hasCycle("--a", tokenScopes(tokenBlocks(overrides, "windows")).get("root")!)).toBe(true);
  expect(resolveToken("var(--a)", tokenScopes(tokenBlocks(overrides, "macos")).get("root")!)).toBe("4px");
  // No external variable contracts are currently needed. Optional browser/native
  // values such as --vibrancy-tint and --notice-from must carry real fallbacks.
});

/** Only arithmetic used by spacing declarations; 16 px/rem is the unscaled grid. */
function gridPixels(value: string): number | null {
  if (value === "auto" || value.includes("%")) return null;
  if (value.startsWith("max(") && value.endsWith(")")) {
    const options = cssParts(value.slice(4, -1), true).map(gridPixels);
    if (options.some((n) => n === null)) throw new Error("Non-length grid maximum");
    return Math.max(...options as number[]);
  }
  if (/[^0-9.\s()+*/-]/.test(value.replace(/calc|rem|px/g, ""))) throw new Error(`Unsupported spacing expression: ${value}`);
  const tokens = value.replaceAll("calc", "").match(/(?:\d*\.)?\d+(?:rem|px)?|[()+*/-]/g) ?? [];
  let at = 0;
  const atom = (): number => {
    const token = tokens[at++];
    if (token === "-") return -atom();
    if (token === "(") { const value = sum(); if (tokens[at++] !== ")") throw new Error("Unbalanced grid expression"); return value; }
    if (!token) throw new Error("Missing grid value");
    return parseFloat(token) * (token.endsWith("rem") ? 16 : 1);
  };
  const product = (): number => { let n = atom(); while (["*", "/"].includes(tokens[at]!)) { const op = tokens[at++], right = atom(); n = op === "*" ? n * right : n / right; } return n; };
  const sum = (): number => { let n = product(); while (["+", "-"].includes(tokens[at]!)) { const op = tokens[at++], right = product(); n = op === "+" ? n + right : n - right; } return n; };
  const result = sum(); if (at !== tokens.length || !Number.isFinite(result)) throw new Error(`Unsupported grid expression: ${value}`);
  return result;
}

// Native menus retain 6/10 px metrics and check-column alignment; the notice's
// inset check and pill tail are optical. Each exception names its exact value.
function opticalSpacing(block: TokenBlock, name: string, value: string) {
  const normalized = (text: string) => text.replace(/(^|[^\d.])0(?:px|rem)(?=$|[^\w.])/g, (_, lead: string) => lead + "0").replace(/(?<![\d.])\.(\d+)/g, "0.$1").replace(/\s/g, "");
  const same = (expected: string) => normalized(value) === normalized(expected);
  if (name === "--lead-gap") return same("calc((var(--tile-size) - var(--icon-glyph-size)) / 2 + var(--row-gap))");
  // Centre the 24 px fill in a 32/44 px target: the coarse inset is 10 px.
  if (block.selector === ".shelf .shelf-chips[data-expanded]" && name === "padding-block") return same("calc((var(--chip-hit-size) - var(--chip-height)) / 2)");
  const borderCompensation = [".search-bar", '[role="option"]', '[role="option"][data-active]', ".key-caps kbd", ".library-bar", ".more > button"].includes(block.selector);
  if (borderCompensation && /^0?calc\(var\(--(?:inset|row-pad|space-1)\)[+-]var\(--(?:border-width|contrast-border-width)\)\)(var\(--space-1\))?$/.test(normalized(value))) return true;
  if (block.selector.includes("#search") && name === "padding-inline-start" && same("calc(var(--lead) + var(--search-icon-width) + var(--lead-gap) - var(--border-width))")) return true;
  const native: Record<string, string[]> = {
    ".menu-popover padding": ["var(--menu-padding)"], ".menu-popover --menu-padding": ["var(--menu-pad)"],
    ".menu-item padding": ["0 10px 0 6px"], ".menu-item gap": ["6px"], ".menu-header padding-inline-start": ["22px"],
    ".toolbar-menu-button padding": ["0 8px 0 10px"], ".toolbar-menu-heading padding": ["8px 8px 4px 30px"],
    ".copied-notice-mark padding": [".2rem"],
  };
  return native[`${block.selector} ${name}`]?.some(same) || (block.file === "notice.css" && /^\.copied-notice\[data-platform="(?:macos|linux)"\]$/.test(block.selector) && name === "padding" && same("0 1.125rem 0 .5rem"));
}

test("spacing audits resolve aliases and calc without exempting arbitrary one-pixel padding", () => {
  const definitions = new Map([["--space", "1rem"], ["--gap", "calc(var(--space) / 2)"]]);
  expect(gridPixels(resolveToken("var(--gap)", definitions)!)).toBe(8);
  expect(gridPixels(resolveToken("calc(var(--gap) + 1px)", definitions)!)).toBe(9);
  expect(gridPixels("1px")).toBe(1);
  const local = { fixture: ":root { --space-1: .25rem; --control-min-size: 2rem; --chip-height: 1.5rem; } .chip-group { --chip-group-target: var(--control-min-size); } @media (pointer: coarse) { .chip-group { --chip-group-target: 2.75rem; } }" };
  for (const [coarse, target, gap] of [[false, 32, 8], [true, 44, 20]] as const) {
    const scope = tokenScopes(tokenBlocks(local, "windows", "normal", true, false, false, coarse)).get("chip-group")!;
    expect(gridPixels(resolveToken("var(--chip-group-target)", scope)!)).toBe(target);
    expect(gridPixels(resolveToken("max(var(--space-1), calc(var(--chip-group-target) - var(--chip-height)))", scope)!)).toBe(gap);
    const shelf = tokenScopes(tokenBlocks(appSheets, "windows", "normal", true, false, false, coarse)).get("shelf")!;
    expect(gridPixels(resolveToken("var(--chip-hit-size)", shelf)!)).toBe(target);
    expect(gridPixels(resolveToken("calc((var(--chip-hit-size) - var(--chip-height)) / 2)", shelf)!)).toBe(gap / 2);
  }
  expect(opticalSpacing({ file: "fixture", selector: ".other", scope: "root", values: [] }, "padding", "1px")).toBeFalsy();
});
