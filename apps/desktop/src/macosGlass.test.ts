import { expect, test, vi } from "vitest";

// Vitest can load these existing runtime builtins without a Node-types dependency.
const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const read = (name: string) => readFileSync(new FileURL(`./${name}`, import.meta.url), "utf8");
const tokens = read("tokens.css");
const native = read("nativeAppearance.css");
const libraryStatus = read("ui/LibraryStatus.css");

test("readable overlays use the panel surface at 92% for menus and 96% for sheets", () => {
  expect(tokens).toMatch(/--surface-overlay:\s*light-dark\(rgb\(250 250 252 \/ 92%\), rgb\(44 44 48 \/ 92%\)\)/);
  expect(tokens).toMatch(/--sheet:\s*light-dark\(rgb\(250 250 252 \/ 96%\), rgb\(44 44 48 \/ 96%\)\)/);
});

function cssRules(source: string): CSSRule[] {
  const sheet = document.createElement("style");
  sheet.textContent = source;
  document.head.append(sheet);
  try { return [...sheet.sheet!.cssRules]; } finally { sheet.remove(); }
}

function macSkinRules(): CSSRule[] {
  const skin = cssRules(native).find((rule) => rule.type === CSSRule.MEDIA_RULE
    && (rule as CSSMediaRule).conditionText === "(prefers-contrast: no-preference) and (forced-colors: none)") as CSSMediaRule;
  return [...skin.cssRules];
}

/** The macOS glass token block: one rule, inside its contrast guard. */
function macBlock(): { guard: string; body: string } {
  const match = tokens.match(/@supports \(color: light-dark\(white, black\)\) \{\s*(@media [^{]+)\{(?:\s*\/\*[^]*?\*\/)?\s*:root:not\(\.notice-window\) \{([^{}]*)\}/);
  expect(match, "macOS glass tokens").toBeTruthy();
  return { guard: match![1]!.trim(), body: match![2]! };
}

function rgba(body: string, token: string): [number, number, number, number][] {
  const value = body.match(new RegExp(`${token}:\\s*light-dark\\(([^;]+)\\);`))?.[1];
  expect(value, token).toBeTruthy();
  return [...value!.matchAll(/rgb\((\d+) (\d+) (\d+) \/ ([\d.]+)%\)/g)].map((m) => [+m[1]!, +m[2]!, +m[3]!, +m[4]! / 100]);
}

/** A role's light and dark colours; translucent text is flattened onto the window surface. */
function hexes(body: string, token: string): number[][] {
  const value = body.match(new RegExp(`${token}:\\s*light-dark\\(([^;]+)\\);`))?.[1];
  expect(value, token).toBeTruthy();
  const surfaces = [[252, 252, 253], [36, 36, 38]];
  return [...value!.matchAll(/#([0-9a-f]{6})|rgb\((\d+) (\d+) (\d+) \/ ([\d.]+)%\)/g)].map((m, mode) => m[1]
    ? [1, 3, 5].map((i) => parseInt(`#${m[1]}`.slice(i, i + 2), 16))
    : [+m[2]!, +m[3]!, +m[4]!].map((c, i) => c * +m[5]! / 100 + surfaces[mode]![i]! * (1 - +m[5]! / 100)));
}

/** Translucent text as painted on a given background. */
const painted = (ink: [number, number, number, number], under: number[]) => ink.slice(0, 3).map((c, i) => c * ink[3]! + under[i]! * (1 - ink[3]!));

const luminance = (rgb: number[]) => rgb.map((c) => c / 255).map((c) => c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4)
  .reduce((sum, c, i) => sum + c * [0.2126, 0.7152, 0.0722][i]!, 0);
const contrast = (a: number[], b: number[]) => {
  const [lighter, darker] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (lighter! + .05) / (darker! + .05);
};

test("the shared macOS look keeps native materials guarded by accessibility preferences", () => {
  const { guard } = macBlock();
  expect(guard).toBe("@media (prefers-contrast: no-preference) and (forced-colors: none)");
  expect(native).toMatch(/@media \(prefers-contrast: no-preference\) and \(forced-colors: none\) \{/);
  // Every macOS element rule is scoped to macOS and skips the contrast preview.
  const scoped = "\n" + macSkinRules().map((rule) => rule.cssText).join("\n");
  for (const [, head] of scoped.matchAll(/\n\s*([^{}\n/][^{}]*)\{/g)) {
    if (head!.trim().startsWith("@supports")) continue;
    expect(head!.trim()).toMatch(/^:root\[data-platform="macos"\][^ ]*/);
  }
  // Shared tokens carry no alternate Windows or Linux palette.
  expect(macBlock().body).not.toMatch(/data-platform="(windows|linux)"/);
});

test("macOS fills and hairlines are translucent so the native material shows through", () => {
  const { body } = macBlock();
  for (const token of ["--separator", "--fill-4", "--fill-3", "--fill-2"]) {
    for (const [, , , alpha] of rgba(body, token)) expect(alpha, token).toBeLessThan(.2);
  }
});

test("only native success shows the material, under a veil; reduced transparency stays opaque", () => {
  // Webviews without light-dark() fall back to the surface at 75%.
  for (const mode of ["vibrancy", "liquid-glass"]) {
    const rule = native.match(new RegExp(String.raw`:root\[data-native-appearance="${mode}"\][^{]*\{([^{}]*)\}`))?.[1];
    expect(rule).toContain("background: var(--vibrancy-tint, color-mix(in srgb, var(--surface) 75%, transparent))");
  }
  expect(native).toMatch(/:root\[data-native-appearance="acrylic"\] \{ background: var\(--acrylic-tint, var\(--surface\)\); \}/);
  // The defensive override comes after, with the same specificity, so it wins.
  const veil = native.indexOf('[data-native-appearance="vibrancy"]');
  const fallback = native.indexOf("@media (prefers-reduced-transparency: reduce), (prefers-contrast: more), (forced-colors: active)");
  expect(fallback).toBeGreaterThan(veil);
  expect(native.slice(fallback)).toMatch(/:root\[data-native-appearance\] \{ background: var\(--surface\); \}/);
  // The panel edge and glass menus exist only while the material is applied.
  expect(native).toMatch(/\[data-native-appearance="vibrancy"\][^{]*body::after/);
  expect(native).toMatch(/\[data-native-appearance="vibrancy"\][^{]*\.menu-popover[^{]*\{[^}]*backdrop-filter/);
});

test("Acrylic text roles keep contrast over black and white backdrops in both themes", () => {
  const selector = ':root[data-system="windows"][data-native-appearance="acrylic"]:not(.notice-window)';
  const roles = native.slice(native.indexOf(selector)).match(/^[^{]+\{([^{}]*)\}/)?.[1] ?? "";
  const shared = macBlock().body;
  const tints = rgba(roles, "--acrylic-tint");
  expect(tints).toHaveLength(2);
  for (const [theme, tint] of tints.entries()) {
    for (const under of [0, 255]) {
      const background = tint.slice(0, 3).map((c) => c * tint[3]! + under * (1 - tint[3]!));
      // The quaternary fill is black or white at 3%.
      const hover = background.map((c) => theme ? c * .97 + 255 * .03 : c * .97);
      for (const fill of [background, hover]) {
        expect(contrast(painted(rgba(shared, "--text")[theme]!, fill), fill), `--text/${theme}/${under}`).toBeGreaterThanOrEqual(4.5);
        expect(contrast(painted(rgba(roles, "--text-secondary")[theme]!, fill), fill), `--text-secondary/${theme}/${under}`).toBeGreaterThanOrEqual(4.5);
        for (const role of ["--scarlet", "--done", "--stop"]) {
          expect(contrast(hexes(roles, role)[theme]!, fill), `${role}/${theme}/${under}`).toBeGreaterThanOrEqual(4.5);
        }
      }
      expect(hexes(roles, "--look")).toEqual(hexes(tokens, "--look"));
      expect(contrast(hexes(roles, "--look")[theme]!, hexes(tokens, "--surface")[theme]!)).toBeGreaterThanOrEqual(3);
    }
  }
  // Accessibility overrides follow the material rule, never exposing its tint.
  expect(native.indexOf("@media (prefers-reduced-transparency: reduce)")).toBeGreaterThan(native.indexOf(selector));
});

test("secondary text keeps 4.5:1 over the veiled material, on the brightest and darkest backdrops", () => {
  const { body } = macBlock();
  const [light, dark] = rgba(body, "--vibrancy-tint");
  // A thin veil let a busy desktop show through the list (hands-on check, 2026-10-27).
  for (const veil of [light!, dark!]) expect(veil[3]).toBeGreaterThanOrEqual(.75);
  const muted = rgba(body, "--text-secondary");
  const text = rgba(body, "--text");
  // CI's appearance probe (#232) saw the light Popover material lift a black
  // channel to 141; the dark material is assumed no brighter than 120 over white.
  const over = (veil: [number, number, number, number], under: number) => veil.slice(0, 3).map((c) => c * veil[3] + under * (1 - veil[3]));
  for (const under of [141, 255]) {
    expect(contrast(painted(muted[0]!, over(light!, under)), over(light!, under))).toBeGreaterThanOrEqual(4.5);
    expect(contrast(painted(text[0]!, over(light!, under)), over(light!, under))).toBeGreaterThanOrEqual(7);
  }
  for (const under of [0, 120]) {
    expect(contrast(painted(muted[1]!, over(dark!, under)), over(dark!, under))).toBeGreaterThanOrEqual(4.5);
    expect(contrast(painted(text[1]!, over(dark!, under)), over(dark!, under))).toBeGreaterThanOrEqual(7);
  }
});

/** Native macOS colour roles, inside the same guard. */
function materialRoles(mode: string): string {
  const body = tokens.match(new RegExp(String.raw`:root\[data-platform="macos"\]\[data-native-appearance="${mode}"\]:not\(\.notice-window\)[^{]*\{([^{}]*)\}`))?.[1];
  expect(body, "vibrancy colour roles").toBeTruthy();
  return body!;
}

test("the window edge is a hairline, never a white rim or glow (prototype round six)", () => {
  const { body } = macBlock();
  const [light, dark] = rgba(body, "--glass-edge");
  // Light: a dark hairline. Dark: a faint white hairline, at most 10%.
  expect(light!.slice(0, 3)).toEqual([0, 0, 0]);
  expect(light![3]).toBeLessThanOrEqual(.12);
  expect(dark![3]).toBeLessThanOrEqual(.1);
  // One layer only: no second highlight anywhere.
  expect(tokens + native).not.toMatch(/--glass-highlight/);
  const edge = native.match(/body::after \{([^}]*)\}/)?.[1] ?? "";
  expect(edge.match(/box-shadow:\s*([^;]+);/)?.[1]).toBe("inset 0 0 0 var(--border-width) var(--glass-edge, transparent)");
});

test("the Look glyph keeps the frozen colours and an opaque adjacent surface for 3:1 contrast", () => {
  expect(hexes(tokens, "--look")).toEqual([[176, 122, 0], [245, 197, 66]]);
  expect(libraryStatus).toMatch(/\.status-dot\[data-health="look"\]\s*\{[^}]*outline:\s*var\(--border-width\) solid var\(--surface\)/);
});

test.each(["vibrancy", "liquid-glass"])("text keeps 4.5:1 and the Look glyph 3:1 with the qualified %s material", (mode) => {
  const { body } = macBlock();
  const [light, dark] = rgba(body, "--vibrancy-tint");
  const over = (veil: [number, number, number, number], under: number) => veil.slice(0, 3).map((c) => c * veil[3] + under * (1 - veil[3]));
  const roles = materialRoles(mode);
  expect(hexes(roles, "--look")).toEqual(hexes(tokens, "--look"));
  const [surfaceLight, surfaceDark] = hexes(tokens, "--surface");
  const [lookLight, lookDark] = hexes(roles, "--look");
  expect(contrast(lookLight!, surfaceLight!)).toBeGreaterThanOrEqual(3);
  expect(contrast(lookDark!, surfaceDark!)).toBeGreaterThanOrEqual(3);
  for (const role of ["--scarlet", "--done", "--stop"]) {
    const [onLight, onDark] = hexes(roles, role);
    for (const under of [141, 255]) expect(contrast(onLight!, over(light!, under)), `${role} light over ${under}`).toBeGreaterThanOrEqual(4.5);
    for (const under of [0, 120]) expect(contrast(onDark!, over(dark!, under)), `${role} dark over ${under}`).toBeGreaterThanOrEqual(4.5);
  }
  // The opaque surface keeps Ibis scarlet itself.
  expect(body).not.toMatch(/--scarlet/);
});

test("the Copied notice window keeps its own pill border and colours", () => {
  document.documentElement.className = "notice-window";
  document.documentElement.dataset.platform = "macos";
  try {
    const selector = ':root[data-platform="macos"]:not(.notice-window)';
    expect(document.documentElement.matches(selector)).toBe(false);
    document.documentElement.className = "";
    expect(document.documentElement.matches(selector)).toBe(true);
  } finally {
    document.documentElement.className = "";
    delete document.documentElement.dataset.platform;
  }
});

test("the frameless macOS writing toggles use the shared pressed fill", () => {
  // jsdom evaluates no media queries: apply the macOS rules, then the picker's
  // own rules, in the order styles.css imports them.
  const styles = read("styles.css").replace(/@import[^;]+;/g, "");
  const mac = macSkinRules().filter((rule) => rule.type === CSSRule.STYLE_RULE).map((rule) => rule.cssText).join("\n");
  const sheet = document.createElement("style");
  // jsdom does not resolve custom properties in border colours: substitute them.
  const values: Record<string, string> = { "--clear": "transparent", "--selected": "rgb(10, 122, 255)", "--selected-text": "rgb(255, 255, 255)", "--separator": "rgb(1, 2, 3)", "--border-width": "1px", "--stroke-field": "transparent", "--fill-3": "rgb(240, 240, 240)", "--fill-1": "rgb(220, 220, 220)", "--text": "rgb(20, 20, 20)" };
  sheet.textContent = (mac.replace(/@supports[^{]*\{[^{}]*\{[^{}]*\}\s*\}/g, "") + styles + read("ui/Button.css"))
    .replace(/var\((--[\w-]+)\)/g, (whole, name: string) => values[name] ?? whole);
  document.head.append(sheet);
  document.documentElement.dataset.platform = "macos";
  document.body.innerHTML = '<section class="setup-writing"><button type="button" class="ui-button" data-variant="secondary" data-selected aria-pressed="true">Markdown</button><button type="button" class="ui-button" data-variant="secondary" aria-pressed="false">Word</button></section>';
  try {
    const [pressed, plain] = [...document.querySelectorAll("button")];
    const border = (element: Element) => getComputedStyle(element).getPropertyValue("border-top-color") || getComputedStyle(element).borderColor;
    expect(border(plain!)).toMatch(/^(transparent|rgba\(0, 0, 0, 0\))$/);
    expect(border(pressed!)).toMatch(/^(transparent|rgba\(0, 0, 0, 0\))$/);
    expect(getComputedStyle(pressed!).backgroundColor).toBe("rgb(220, 220, 220)");
    expect(getComputedStyle(pressed!).color).toBe("rgb(20, 20, 20)");
    expect(getComputedStyle(plain!).backgroundColor).not.toBe("rgb(220, 220, 220)");
  } finally {
    sheet.remove();
    document.body.innerHTML = "";
    delete document.documentElement.dataset.platform;
  }
});

test("menus stay opaque where the webview cannot blur behind them", () => {
  const fallback = native.slice(native.indexOf("@supports not ((backdrop-filter: blur(1px)) or (-webkit-backdrop-filter: blur(1px)))"));
  expect(fallback).toMatch(/\.menu-popover, \.keys-dialog\)\s*\{\s*background:\s*var\(--surface\);/);
});

// JSDOM does not evaluate media/supports queries. Apply the real rules for a
// blur-capable webview under each preference, retaining selector specificity.
function overlayRules(source: string, preferences: Set<string>): string {
  const select = (rules: CSSRuleList | CSSRule[]): string => [...rules].map((rule) => {
    if (rule.type === CSSRule.STYLE_RULE) return rule.cssText;
    if (rule.type === CSSRule.MEDIA_RULE) {
      const media = rule as CSSMediaRule;
      return media.conditionText.split(",").some((query) => query.split(/\s+and\s+/).every((part) => preferences.has(part.trim())))
        ? select(media.cssRules) : "";
    }
    if ("cssRules" in rule) {
      const group = rule as CSSSupportsRule;
      return group.conditionText?.startsWith("not ") ? "" : select(group.cssRules);
    }
    return "";
  }).join("\n");
  return select(cssRules(source));
}

test.each([
  ["macos", "opaque", "normal"], ["macos", undefined, "normal"],
  ["macos", "opaque", "reduce"], ["macos", "vibrancy", "reduce"], ["macos", "liquid-glass", "reduce"],
  ["macos", "liquid-glass", "contrast"], ["macos", "liquid-glass", "forced"],
  ["macos", "vibrancy", "normal"], ["macos", "liquid-glass", "normal"],
  ["windows", "acrylic", "reduce"], ["linux", "opaque", "reduce"],
] as const)("%s overlay cascade: %s material with %s preferences", (platform, mode, preference) => {
  const preferences = new Set([
    `(prefers-contrast: ${preference === "contrast" ? "more" : "no-preference"})`,
    `(forced-colors: ${preference === "forced" ? "active" : "none"})`,
    `(prefers-reduced-transparency: ${preference === "reduce" ? "reduce" : "no-preference"})`,
  ]);
  const sheet = document.createElement("style");
  // These are the production import order and real component selectors.
  // Also put the old native skin last: shared roles must win independent of
  // import order. JSDOM cannot evaluate these colour functions/custom props.
  const source = read("ui/Overlay.css") + read("styles.css").replace(/@import[^;]+;/g, "") + read("ui/Menu.css") + read("ui/KeysSheet.css") + native;
  const solid = preference !== "normal" || mode !== "vibrancy" && mode !== "liquid-glass";
  sheet.textContent = overlayRules(source, preferences)
    .replace(/var\(--(?:surface|surface-overlay|sheet)\)/g, "rgb(252, 252, 253)")
    .replace(/var\(--blur\)/g, solid ? "none" : "blur(40px) saturate(1.6)");
  document.head.append(sheet);
  document.documentElement.dataset.platform = "macos";
  document.documentElement.dataset.system = platform;
  if (mode) document.documentElement.dataset.nativeAppearance = mode;
  document.body.innerHTML = '<div class="menu-popover ui-overlay">Menu</div><div class="keys-dialog ui-overlay ui-sheet">Shortcuts</div>';
  try {
    for (const element of document.body.children) {
      const style = getComputedStyle(element);
      expect(style.backgroundColor).toBe("rgb(252, 252, 253)");
      if (solid) {
        expect(style.getPropertyValue("backdrop-filter")).toBe("none");
      } else {
        expect(style.getPropertyValue("backdrop-filter")).toBe("blur(40px) saturate(1.6)");
      }
    }
  } finally {
    sheet.remove(); document.body.innerHTML = "";
    delete document.documentElement.dataset.platform;
    delete document.documentElement.dataset.system;
    delete document.documentElement.dataset.nativeAppearance;
  }
});
