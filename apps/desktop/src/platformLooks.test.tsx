import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { App } from "./App";
import { galleryProvider, galleryStates } from "./gallery-fixtures";
import { platformOf, setPlatform } from "./nativeAppearance";

// Vitest can load these existing runtime builtins without a Node-types dependency.
const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");

const tokens = readFileSync(new FileURL("./tokens.css", import.meta.url), "utf8");
const styles = readFileSync(new FileURL("./styles.css", import.meta.url), "utf8");
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
  expect(bar).toMatch(/min-height:\s*var\(--search-bar-height\)/);
  expect(bar).toMatch(/padding:\s*var\(--inset\)/);
  // The 40 px field plus the inset above and below: 56 px, 48 px on Windows.
  expect(tokens).toMatch(/--search-bar-height:\s*calc\(var\(--field-h\) \+ 2 \* var\(--inset\)\)/);
  expect(tokens).toMatch(/--field-h:\s*2\.5rem/);
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
  const expected = { macos: [12, 8, 6, 6, 12, 6], windows: [8, 4, 4, 2, 8, 4], linux: [14, 8, 6, 4, 12, 6] };
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
    expect(textX, platform).toBe(platform === "windows" ? 60 : 64);
    expect(g["--inset"]! + lead + g["--icon-glyph-size"]! / 2, platform).toBe(g["--inset"]! + g["--row-pad"]! + g["--tile-size"]! / 2);
  }
  expect(rules(styles, ".search-bar > svg")).toMatch(/margin-inline:\s*var\(--lead\) calc\(var\(--lead-gap\) - var\(--search-gap\)\)/);
  expect(rules(styles, '[role="option"]')).toMatch(/padding:\s*0 calc\(var\(--row-pad\) - var\(--border-width\)\)/);
  expect(rules(styles, '[role="option"]')).toMatch(/gap:\s*var\(--row-gap\)/);
  expect(rules(styles, ".results-area")).toMatch(/padding:\s*var\(--inset\)/);
  // The framed box's text start pays for its 1 px border.
  expect(rules(styles, ':root[data-platform="windows"] #search')).toMatch(/padding-inline-start:\s*calc\(var\(--lead\) \+ var\(--search-icon-width\) \+ var\(--lead-gap\) - var\(--border-width\)\)/);
  // Logical sides only, so a right-to-left picker mirrors.
  expect(rules(styles, "footer")).toMatch(/padding-inline:\s*var\(--footer-padding\)/);
});

test("window radius follows each platform and macOS matches the native corner", () => {
  // macOS keeps the natively verified 12 pt corner until macOS 26 detection lands; the token must agree with it.
  const native = corners.match(/CORNER_RADIUS_POINTS: f64 = ([\d.]+);/)?.[1];
  expect(rules(tokens, ":root")).toMatch(new RegExp(`--r-win:\\s*${Number(native)}px`));
  expect(rules(tokens, ':root[data-platform="windows"]')).toMatch(/--r-win:\s*8px/);
  expect(rules(tokens, ':root[data-platform="windows"]')).toMatch(/--button-radius:\s*4px/);
  expect(rules(tokens, ':root[data-platform="linux"]')).toMatch(/--r-win:\s*14px/);
});

test("scarlet marks only the citation: copied text, preview and tray", () => {
  const scarlet = [...styles.matchAll(/([^{}]+)\{[^{}]*var\(--citation\)[^{}]*\}/g)].map(([, head]) => head!.trim()).join(",");
  for (const target of [".copy-notice code", ".format-preview .preview-text", '[aria-label="Selected references"] code']) {
    expect(scarlet, target).toContain(target);
  }
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
  expect(rules(styles, ":focus-visible")).toMatch(/outline:\s*var\(--focus-ring-width\) solid var\(--focus-ring-color\);\s*outline-offset:\s*var\(--focus-ring-offset\);\s*box-shadow:\s*var\(--focus-ring-shadow\)/);
  // No ring is drawn on plain :focus or :focus-within, which a click also triggers.
  for (const [, head, body] of styles.slice(0, styles.indexOf("@media (prefers-contrast: more)")).matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (/:focus(-within)?\b(?!-visible)/.test(head!) && !/\[data-input="pointer"\]/.test(head!)) expect(body, head).not.toMatch(/outline:\s*var/);
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

test("the format control shows its ring after Tab but not after a click", () => {
  const ring = rules(styles, ':root:not([data-input="pointer"]) .format-picker:has(select:focus-visible)');
  expect(ring).toMatch(/outline:\s*var\(--focus-ring-width\) solid var\(--focus-ring-color\)/);
  expect(styles).not.toMatch(/(^|[},]\s*)\.format-picker:focus-within/m);
});

test("the search box never shows a ring, except with contrast or forced colours", () => {
  const outside = styles.slice(0, styles.indexOf("@media (prefers-contrast: more)"));
  expect(rules(outside, "#search:focus-visible")).toMatch(/outline:\s*none/);
  const windows = rules(outside, ':root[data-platform="windows"] #search:focus');
  expect(windows).toMatch(/outline:\s*none/);
  expect(windows).toMatch(/border-bottom-color:\s*var\(--accent\)/);
  for (const media of ["@media (prefers-contrast: more)", "@media (forced-colors: active)"]) {
    const block = styles.slice(styles.indexOf(media), styles.indexOf("\n}\n", styles.indexOf(media)));
    expect(rules(block, "#search:focus-visible"), media).toMatch(/outline:\s*var\(--focus-width\) solid/);
    expect(rules(block, ":root[data-platform] #search:focus"), media).toMatch(/outline:\s*var\(--focus-width\) solid/);
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
  expect(tokens).toMatch(/--focus-width:\s*2px/);
  const clear = rules(styles, "#search::-webkit-search-cancel-button");
  expect(clear).toMatch(/appearance:\s*none/);
  expect(clear).toMatch(/width:\s*var\(--control-min-size\)/);
  expect(clear).toMatch(/background:\s*var\(--muted\)/);
  expect(clear).toMatch(/center \/ var\(--icon-size\)/);
  expect(tokens).toMatch(/--control-min-size:\s*2rem/);
  expect(tokens).toMatch(/--icon-size:\s*1rem/);
  const forced = styles.slice(styles.indexOf("@media (forced-colors: active)"));
  expect(rules(forced, "#search::-webkit-search-cancel-button")).toMatch(/background:\s*FieldText/);
  // The gallery's contrast preview restores the ring as prefers-contrast does.
  expect(rules(gallery, ':root[data-gallery-theme="contrast"] #search:focus-visible'))
    .toMatch(/outline:\s*var\(--focus-width\) solid var\(--focus\)/);
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

test("every spacing token sits on the 4 px grid", () => {
  // Row heights, type, 1 px borders and hidden-element offsets, radii and the gallery's own frame are exempt.
  const exempt = new Set<string>();
  const off: string[] = [];
  for (const [, name, value] of tokens.matchAll(/(--[\w-]*(?:padding|margin|gap|space|inset|-h|height|cap|compact|size)[\w-]*):\s*([^;]+);/g)) {
    if (/^--(type|gallery|row-|heading-size|title-size|metadata-size|key-size|hint-size|footer-size|search-size|search-line|larger)/.test(name!) || exempt.has(name!)) continue;
    for (const [, digits, unit] of value!.matchAll(/(-?[\d.]+)(rem|px)\b/g)) {
      const number = Number(digits);
      const px = unit === "rem" ? number * 16 : number;
      if (Math.abs(px) !== 1 && px % 4 !== 0) off.push(`${name}: ${value}`);
    }
  }
  expect(off).toEqual([]);
});

test("unused tokens are gone and the spec's line heights apply", () => {
  for (const name of ["--forced-selected-offset", "--key-padding", "--key-radius", "--space-6"]) expect(tokens, name).not.toContain(`${name}:`);
  expect(rules(styles, ".results-heading")).toMatch(/line-height:\s*var\(--type-label-line\)/);
  expect(rules(styles, "#search")).toMatch(/line-height:\s*var\(--search-line\)/);
});

test("every button is at least 32 px, with on-grid padding", () => {
  const button = rules(styles, "button");
  expect(button).toMatch(/min-height:\s*var\(--control-min-size\)/);
  expect(tokens).toMatch(/--control-min-size:\s*2rem/);
  expect(tokens).toMatch(/--button-padding:\s*0 var\(--space-3\)/);
});

test("dividers are quiet separators, distinct from 3:1 control edges", () => {
  for (const selector of [".search-bar", "footer", ".copy-notice", ".format-preview", '[aria-label="Selected references"]']) {
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
  expect(rules(styles, "button")).toMatch(/border:\s*var\(--border-width\) solid var\(--button-edge\)/);
  const kbd = rules(styles, "kbd");
  expect(kbd).toMatch(/background:\s*var\(--button-background\)/);
  expect(kbd).toMatch(/border:\s*var\(--border-width\) solid var\(--button-edge\)/);
  expect(kbd).toMatch(/height:\s*var\(--key-cap\)/);
  expect(rules(tokens, ":root")).toMatch(/--button-edge:\s*var\(--clear\)/);
  for (const media of ["@media (prefers-contrast: more)", "@media (forced-colors: active)"]) {
    expect(rules(tokens.slice(tokens.indexOf(media)), ":root[data-platform]"), media).toMatch(/--button-edge:\s*(CanvasText|ButtonText)/);
  }
});

test("a wrapped footer keeps breathing room above and below its lines", () => {
  // The footer has no block padding of its own (its 40 px is a min-height), so
  // the library bar carries it: 1 px divider + 3 + 32 px More + 4 = 40 px, and
  // wrapped lines never touch the divider or the window's bottom corners.
  expect(rules(styles, ".library-bar")).toMatch(/padding-block:\s*calc\(var\(--space-1\) - var\(--border-width\)\) var\(--space-1\)/);
  expect(rules(styles, ".more > button")).toMatch(/min-height:\s*var\(--control-min-size\);\s*padding-block:\s*0/);
  expect(rules(tokens, ":root")).toMatch(/--control-min-size:\s*2rem/);
  expect(rules(tokens, ":root")).toMatch(/--footer-height:\s*2\.5rem/);
});

test("the thicker active-row edge in increased contrast does not move the row's text", () => {
  const contrast = styles.slice(styles.indexOf("@media (prefers-contrast: more)"));
  expect(rules(contrast, '[role="option"][data-active]')).toMatch(/padding-inline:\s*calc\(var\(--row-pad\) - var\(--contrast-border-width\)\)/);
});
