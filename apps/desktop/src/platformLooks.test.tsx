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
  const padding = tokens.match(/--search-padding:\s*([^;]+);/)?.[1];
  expect(padding).toBe("var(--space-1) var(--space-4)");
  expect(tokens).toMatch(/--search-bar-height:\s*3\.5rem/);
});

test("Windows and Linux search in a 16 px framed box; macOS stays frameless and large", () => {
  const framed = rules(tokens, ':root[data-platform="windows"], :root[data-platform="linux"]');
  expect(framed).toMatch(/--search-size:\s*1rem/);
  expect(rules(styles, ':root[data-platform="windows"] #search')).toMatch(/border:/);
  expect(rules(styles, ':root[data-platform="windows"] #search:focus')).toMatch(/var\(--accent\)/);
  expect(rules(styles, ':root[data-platform="linux"] #search')).toMatch(/border-radius:/);
  expect(tokens).toMatch(/--search-size:\s*var\(--type-search-size\)/);
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

test("window radius follows each platform and macOS matches the native corner", () => {
  // macOS keeps the natively verified 12 pt corner for 0.2; the token must agree with it.
  const native = corners.match(/CORNER_RADIUS_POINTS: f64 = ([\d.]+);/)?.[1];
  expect(rules(tokens, ":root")).toMatch(new RegExp(`--window-radius:\\s*${Number(native)}px`));
  expect(rules(tokens, ':root[data-platform="windows"]')).toMatch(/--window-radius:\s*8px/);
  expect(rules(tokens, ':root[data-platform="windows"]')).toMatch(/--button-radius:\s*4px/);
  expect(rules(tokens, ':root[data-platform="linux"]')).toMatch(/--window-radius:\s*14px/);
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
  const pair = (token: string) => tokens.match(new RegExp(`${token}:\\s*light-dark\\((#[0-9a-f]{6}),\\s*(#[0-9a-f]{6})\\)`))!.slice(1, 3).map(hex);
  const tint = Number(rules(tokens, `:root[data-platform="${platform}"]`).match(/--row-active-background:\s*color-mix\(in srgb, var\(--text\) (\d+)%/)?.[1]) / 100;
  const [surface, text, muted] = [pair("--surface"), pair("--text"), pair("--muted")];
  for (const mode of [0, 1]) {
    const row = surface[mode]!.map((c: number, i: number) => c * (1 - tint) + text[mode]![i]! * tint);
    const [lighter, darker] = [luminance(muted[mode]!), luminance(row)].sort((a, b) => b - a);
    expect((lighter! + .05) / (darker! + .05)).toBeGreaterThanOrEqual(4.5);
  }
});

test("an added row is a faint scarlet wash, not a ring, and keeps the ring where color-mix is missing", () => {
  const ring = /box-shadow:\s*inset 0 0 0 var\(--focus-width\) var\((--[\w-]+)\)/;
  // Fallback first (older WebKitGTK): the 2 px ring on every row, active or not.
  const plain = styles.slice(0, styles.indexOf("@supports (background: color-mix"));
  expect(rules(plain, '[role="option"][data-citation-selected]').match(ring)?.[1]).toBe("--citation");
  expect(rules(plain, '[role="option"][data-active][data-citation-selected]').match(ring)?.[1]).toBe("--row-active-text");
  // With color-mix: a wash and no ring or edge, and the active row keeps only its platform fill.
  const supports = styles.slice(styles.indexOf("@supports (background: color-mix"));
  const mixed = supports.slice(0, supports.indexOf("\n}\n") + 3);
  const added = rules(mixed, '[role="option"][data-citation-selected]');
  expect(added).toMatch(/box-shadow:\s*none/);
  expect(added).toMatch(/border-color:\s*var\(--clear\)/);
  expect(added).toMatch(/background:\s*color-mix\(in srgb, var\(--citation\) 8%, transparent\)/);
  const activeAdded = rules(mixed, '[role="option"][data-active][data-citation-selected]');
  expect(activeAdded).toMatch(/background:\s*var\(--row-active-background\)/);
  expect(activeAdded).toMatch(/box-shadow:\s*none/);
  // High contrast brings the ring back on both.
  const contrast = styles.slice(styles.indexOf("@media (prefers-contrast: more)"));
  expect(rules(contrast, '[role="option"][data-citation-selected]').match(ring)?.[1]).toBe("--citation");
  expect(rules(contrast, '[role="option"][data-active][data-citation-selected]').match(ring)?.[1]).toBe("--row-active-text");
  expect(rules(gallery, ':root[data-gallery-theme="contrast"] [role="option"][data-citation-selected]').match(ring)?.[1]).toBe("--citation");
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
    const activeAdded = getComputedStyle(row(1)).outlineColor;
    fireEvent.keyDown(box, { key: "ArrowDown" });
    await waitFor(() => expect(row(2).hasAttribute("data-active")).toBe(true));
    expect(row(2).hasAttribute("data-citation-selected")).toBe(false);
    const activeOnly = getComputedStyle(row(2));
    const system = (name: string) => {
      const probe = document.createElement("span");
      probe.style.color = name;
      document.body.append(probe);
      const value = getComputedStyle(probe).color;
      probe.remove();
      return value;
    };
    expect(system("HighlightText")).not.toBe(system("Highlight"));
    // The ring on an active added row uses the text colour paired with the Highlight fill.
    expect(activeAdded).toBe(system("HighlightText"));
    // An active row that is not added has no such ring.
    expect(activeOnly.outlineStyle === "" || activeOnly.outlineStyle === "none" || activeOnly.outlineColor !== activeAdded).toBe(true);
  } finally {
    sheet.remove();
  }
});

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
  expect(rules(forced, '[role="option"][data-active][data-citation-selected]')).toMatch(/outline-color:\s*HighlightText/);
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

