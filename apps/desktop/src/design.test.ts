// The design system's one stylesheet check (decision 0035, #1678). Every stylesheet
// is parsed into the CSS object model, and each rule of the system
// (docs/design.md, docs/design/foundations.md) is one entry in `rules`: run over
// every declaration, and over a planted stylesheet it must catch, so a rule can
// never pass because the parser missed what it forbids. Contrast is computed
// from the token values themselves; pictures of the result live in gallery/.
import { expect, test, vi } from "vitest";

const { readFileSync, readdirSync } = await vi.importActual<{
  readFileSync: (path: URL, encoding: string) => string;
  readdirSync: (path: URL, options: { recursive: boolean }) => string[];
}>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const root = new FileURL("./", import.meta.url);
const read = (name: string) => readFileSync(new FileURL(name, root), "utf8");
const sheets = [
  ...readdirSync(root, { recursive: true }).filter((name) => name.endsWith(".css")).sort().map((name) => [name, read(name)] as const),
  ...["style.css", "demo.css"].map((name) => [`website/${name}`, readFileSync(new FileURL(`../../../website/${name}`, import.meta.url), "utf8")] as const),
];
const tokens = read("tokens.css").replace(/\/\*[^]*?\*\//g, "");
/** The tokens with the colour parameters written out, for the contrast checks. */
const param = (name: string) => tokens.match(new RegExp(`${name}: (#[0-9a-f]{6});`))![1]!;
const resolved = tokens.replaceAll("var(--ibis-dark)", param("--ibis-dark")).replaceAll("var(--ibis)", param("--ibis"));

type Declaration = { file: string; selector: string; property: string; value: string; style: CSSStyleDeclaration; reduced: boolean; motionOnly: boolean };
/** Every declaration as the CSS object model reads it, inside every @media and @supports. */
function declarations(file: string, css: string): Declaration[] {
  const element = document.createElement("style");
  element.textContent = css;
  document.head.append(element);
  const found: Declaration[] = [];
  const walk = (list: CSSRuleList, reduced: boolean, motionOnly: boolean) => {
    for (const rule of Array.from(list)) {
      if (rule.type === CSSRule.STYLE_RULE) {
        const { selectorText: selector, style } = rule as CSSStyleRule;
        for (const property of Array.from(style)) found.push({ file, selector, property, value: style.getPropertyValue(property).trim(), style, reduced, motionOnly });
      } else if (rule.type !== CSSRule.KEYFRAMES_RULE && "cssRules" in rule) {
        const condition = (rule as CSSMediaRule).conditionText ?? "";
        walk((rule as CSSGroupingRule).cssRules, reduced || /prefers-reduced-motion:\s*reduce\b/.test(condition),
          motionOnly || /prefers-reduced-motion:\s*no-preference/.test(condition));
      }
    }
  };
  try { walk(element.sheet!.cssRules, false, false); } finally { element.remove(); }
  return found;
}
const all = sheets.flatMap(([file, css]) => declarations(file, css));

const outsideTokens = (file: string) => file !== "tokens.css";
/** The app's own sheets: the website composes its own page from the shared tokens. */
const app = (file: string) => outsideTokens(file) && !file.startsWith("website/");
/** Parts read values; the token file and the two native skins state them. */
const parts = (file: string) => !["tokens.css", "nativeAppearance.css", "nativeAccent.css"].includes(file) && !file.startsWith("website/");
// A length written out, other than zero (which the object model reads as 0px).
const length = /(^|[\s(,*/+-])-?(?!0+(\.0+)?(px|rem)\b)\d*\.?\d+(px|rem)\b/;
const colour = /#[0-9a-f]{3,8}\b|\b(rgba?|hsla?|oklch|oklab|color)\(|\b(white|black)\b/i;
const rules: { says: string; applies?: (file: string) => boolean; breaks: (d: Declaration) => boolean; planted: string }[] = [
  { says: "corners are the three steps, the window's, a capsule or the app icon's", applies: app,
    // A part may name a corner its container sets, as long as it falls back to a step.
    breaks: ({ property, value }) => /radius$/.test(property) && !/^(0|0px|inherit|(var\(--[\w-]+, )?var\(--r-(small|control|large|win|full|icon)\)\)?)$/.test(value),
    planted: ".planted { border-radius: 7px; }" },
  { says: "padding, margins and gaps come from the space scale", applies: parts,
    breaks: ({ property, value }) => /^(padding|margin|gap|row-gap|column-gap)/.test(property) && length.test(value),
    planted: ".planted { padding: 10px var(--space-2); }" },
  { says: "colours come from tokens", applies: parts,
    breaks: ({ property, value }) => !property.startsWith("--") && colour.test(value), planted: ".planted { color: #123456; }" },
  { says: "type sizes come from tokens", applies: (file) => parts(file),
    breaks: ({ property, value }) => /^font(-size)?$/.test(property) && length.test(value), planted: ".planted { font-size: 13px; }" },
  { says: "only the shared overlay paints the raised layer, its shadow and its material", applies: (file) => outsideTokens(file) && file !== "ui/Overlay.css",
    breaks: ({ property, value }) => /backdrop-filter$/.test(property) || /var\(--(surface-overlay|shadow-overlay|blur)\)/.test(value),
    planted: ".planted { background: var(--surface-overlay); }" },
  { says: "one scroll bar, drawn only by Foundation.css", applies: (file) => app(file) && file !== "ui/Foundation.css",
    breaks: ({ property, selector }) => /^scrollbar-(width|color)$/.test(property) || selector.includes("::-webkit-scrollbar"),
    planted: ".planted { scrollbar-width: thin; }" },
  { says: "only tokens.css sets a colour scheme", applies: app,
    breaks: ({ property }) => property === "color-scheme", planted: ".planted { color-scheme: dark; }" },
  { says: "a one-sided edge is a divider, never a state", applies: (file) => !file.startsWith("website/"),
    breaks: ({ property, value }) => /^border-(inline|block)-(start|end)(-color)?$/.test(property) && /solid/.test(value)
      && !/--separator|--stroke-field|transparent|currentcolor|CanvasText|ButtonText/i.test(value),
    planted: ".planted { border-inline-start: 3px solid var(--accent); }" },
  { says: "tertiary ink stays on placeholders and resting glyphs, never words a person reads",
    breaks: ({ property, value, selector }) => property === "color" && value === "var(--text-tertiary)" && !/placeholder|-glyph|-icon|-remove/.test(selector),
    planted: ".planted-label { color: var(--text-tertiary); }" },
  { says: "a one-shot movement takes the motion tokens; only a looping indicator keeps its own period", applies: parts,
    breaks: ({ property, value }) => /^(transition|animation)(-duration)?$/.test(property) && /(^|[\s,])(?!0m?s\b)\d*\.?\d+m?s\b/.test(value) && !/infinite/.test(value),
    planted: ".planted { transition: opacity 300ms ease; }" },
];

test.each(rules)("$says", (rule) => {
  expect(all.filter((found) => (rule.applies?.(found.file) ?? true) && rule.breaks(found))
    .map(({ file, selector, property, value }) => `${file}: ${selector} { ${property}: ${value} }`)).toEqual([]);
  expect(declarations("planted.css", rule.planted).some(rule.breaks), "the rule sees what it forbids").toBe(true);
});

/** Movements without a Reduce Motion answer: the same selector in the same file
 * answers under prefers-reduced-motion: reduce, or the movement reads a token
 * that the reduced :root redefines. */
function unanswered(found: readonly Declaration[]) {
  const reducedTokens = new Set(found.filter((d) => d.reduced && d.selector === ":root" && d.property.startsWith("--")).map((d) => d.property));
  return found.flatMap((d, index) => {
    const family = /^(animation|transition)/.exec(d.property)?.[1];
    if (!family || d.reduced || d.motionOnly || !/^(animation|transition)(-name|-property|-duration|-delay|-iteration-count)?$/.test(d.property)
      || /^(none|0|0s|initial|unset|revert)$/.test(d.value)) return [];
    const answered = found.slice(index + 1).some((a) => a.reduced && a.file === d.file && a.selector === d.selector && a.property.startsWith(family))
      || [...d.value.matchAll(/var\((--[\w-]+)/g)].some((match) => reducedTokens.has(match[1]!));
    return answered ? [] : [`${d.file}: ${d.selector}: ${family}`];
  });
}

test("every movement has a Reduce Motion answer, nested groups included", () => {
  expect([...new Set(unanswered(all.filter((d) => !d.file.startsWith("website/"))))]).toEqual([]);
  const motion = "@supports (display: grid) { @media (min-width: 1px) { .planted { transition: transform var(--dur); } } }";
  expect(unanswered(declarations("planted.css", motion))).toEqual(["planted.css: .planted: transition"]);
  expect(unanswered(declarations("planted.css", `${motion} @media (prefers-reduced-motion: reduce) { .planted { transition: none; } }`))).toEqual([]);
});

const defined = new Set(all.filter((d) => d.property.startsWith("--")).map((d) => d.property));
// Set from script at run time: the native accent, how far a list can scroll and measured widths.
const fromScript = /^--(native-accent|scroll-fade-|choice-columns|footer-fix|trigger-width|preview-window-[wh]|progress)$|^--(native-accent|scroll-fade-)/;

test("every token a stylesheet reads is defined, and every token is read", () => {
  const reads = all.flatMap((d) => [...d.value.matchAll(/var\((--[\w-]+)/g)].map((match) => [d.file, match[1]!] as const));
  expect([...new Set(reads.filter(([, name]) => !defined.has(name) && !fromScript.test(name)).map(([file, name]) => `${file}: ${name}`))]).toEqual([]);
  const scripts = readdirSync(root, { recursive: true }).filter((name) => name.endsWith(".tsx")).map(read).join("");
  const used = new Set([...reads.map(([, name]) => name), ...[...scripts.matchAll(/var\((--[\w-]+)/g)].map((match) => match[1]!)]);
  const own = new Set(declarations("tokens.css", read("tokens.css")).filter((d) => d.property.startsWith("--")).map((d) => d.property));
  expect([...own].filter((token) => !token.startsWith("--website-") && !used.has(token))).toEqual([]);
});

test("the motion tokens stay within 280 ms", () => {
  for (const name of ["--dur", "--dur-panel"]) {
    const value = all.filter((d) => d.file === "tokens.css" && d.property === name).map((d) => Number.parseFloat(d.value));
    expect(value.length, name).toBeGreaterThan(0);
    for (const ms of value) expect(ms, name).toBeLessThanOrEqual(280);
  }
});

/** The cascade fixes a shared part needs, read from the parsed rules: each was a bug when missing. */
const required: [file: string, selector: string, property: string, value: string][] = [
  // A keyboard-focused choice keeps its fill and outranks the global ring, whatever loads after it.
  ["ui/ChoiceList.css", ".choice-screen .choice-row[data-focus-visible]", "box-shadow", "none"],
  ["ui/ChoiceList.css", ".choice-row[data-focused]", "background", "var(--selected)"],
  ["ui/ChoiceList.css", ".choice-screen .choice-row", "color", "var(--text)"],
  // The Settings search clear button outranks the shared button's position.
  ["ui/SettingsGroup.css", ".ui-settings-search > .ui-settings-search-clear.ui-button", "position", "absolute"],
  // A key cap on the primary uses the pressed fill, so its label stays readable on every accent.
  ["ui/Button.css", ".ui-button[data-variant=\"primary\"] .key-caps kbd", "background", "var(--accent-pressed)"],
  // Problems and the copied mark read in the text colour, never a status hue.
  ["ui/SettingsGroup.css", ".ui-settings-row-problem", "color", "var(--text)"],
  ["features/picker/CitationShelf.css", ".shelf-copied", "color", "var(--text)"],
  // Settings reserves the title bar only where macOS draws its buttons over the window.
  ["tokens.css", ":root[data-system=\"macos\"]", "--titlebar-height", "var(--touch-target)"],
  // The focus ring is solid accent text, so it keeps 3:1 on every surface.
  ["tokens.css", ":root", "--ring", "var(--accent-text)"],
];

test.each(required)("%s: %s keeps %s: %s", (file, selector, property, value) => {
  expect(all.some((d) => d.file === file && d.selector === selector && d.style.getPropertyValue(property).trim() === value)).toBe(true);
});

const channel = (value: number) => value <= .03928 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4;
const luminance = (hex: string) => {
  const [r, g, b] = [1, 3, 5].map((start) => channel(Number.parseInt(hex.slice(start, start + 2), 16) / 255));
  return .2126 * r! + .7152 * g! + .0722 * b!;
};
const contrast = (a: string, b: string) => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi! + .05) / (lo! + .05);
};
/** A role's light and dark colours; translucent text is flattened onto what is under it (the window by default). */
const surfaces = ["#ffffff", "#1e1e1e"] as const;
const flatten = (value: string, mode: number, under: string = surfaces[mode]!) => {
  const rgb = value.match(/rgb\((\d+) (\d+) (\d+) \/ ([\d.]+)%\)/);
  if (!rgb) return value;
  const alpha = Number(rgb[4]) / 100;
  return `#${[1, 2, 3].map((i) => Math.round(Number(rgb[i]) * alpha + Number.parseInt(under.slice(2 * i - 1, 2 * i + 1), 16) * (1 - alpha)).toString(16).padStart(2, "0")).join("")}`;
};
const pair = (token: string) => {
  const match = resolved.match(new RegExp(`${token}: light-dark\\((#[0-9a-f]{6}|rgb\\([^)]*\\)), (#[0-9a-f]{6}|rgb\\([^)]*\\))\\)`));
  expect(match, token).toBeTruthy();
  return [match![1]!, match![2]!] as const;
};


test.each([["--text", "--surface", 4.5], ["--text-secondary", "--surface", 4.5], ["--scarlet", "--surface", 3],
  ["--text", "--surface-overlay", 4.5], ["--text-secondary", "--surface-overlay", 4.5]] as const)(
  "%s keeps %s:1 on %s in light and dark", (token, under, ratio) => {
    const surface = pair(under), value = pair(token);
    expect(contrast(flatten(value[0], 0, surface[0]), surface[0])).toBeGreaterThanOrEqual(ratio);
    expect(contrast(flatten(value[1], 1, surface[1]), surface[1])).toBeGreaterThanOrEqual(ratio);
  });

/** A translucent material's tint, light and dark, as [r, g, b, alpha]. */
const tint = (css: string, name: string) => [...(css.match(new RegExp(`${name}: light-dark\\(([^;]*)\\);`))?.[1] ?? "").matchAll(/rgb\((\d+) (\d+) (\d+) \/ ([\d.]+)%\)/g)]
  .map(([, r, g, b, a]) => [Number(r), Number(g), Number(b), Number(a) / 100] as const);
const hexOf = (rgb: readonly number[]) => `#${rgb.slice(0, 3).map((c) => Math.round(c).toString(16).padStart(2, "0")).join("")}`;
const over = (veil: readonly number[], under: number) => veil.slice(0, 3).map((c) => c * veil[3]! + under * (1 - veil[3]!));

test("text keeps 7:1 over the veiled Mac material and 4.5:1 over Acrylic, on the brightest and darkest backdrops", () => {
  const text = pair("--text");
  const [light, dark] = tint(tokens, "--vibrancy-tint");
  // A thin veil let a busy desktop show through the list (hands-on check, 2026-10-27).
  for (const veil of [light!, dark!]) expect(veil[3]).toBeGreaterThanOrEqual(.75);
  // CI's appearance probe (#232) saw the light material lift a black channel to 141.
  for (const under of [141, 255]) expect(contrast(flatten(text[0], 0, hexOf(over(light!, under))), hexOf(over(light!, under)))).toBeGreaterThanOrEqual(7);
  for (const under of [0, 120]) expect(contrast(flatten(text[1], 1, hexOf(over(dark!, under))), hexOf(over(dark!, under)))).toBeGreaterThanOrEqual(7);
  const acrylic = tint(read("nativeAppearance.css"), "--acrylic-tint");
  expect(acrylic).toHaveLength(2);
  for (const [mode, veil] of acrylic.entries()) for (const under of [0, 255]) {
    const backdrop = hexOf(over(veil, under));
    expect(contrast(flatten(text[mode]!, mode, backdrop), backdrop), `Acrylic ${mode} over ${under}`).toBeGreaterThanOrEqual(4.5);
  }
});

test("the ladders hold the values measured from AppKit on macOS 27.2", () => {
  for (const value of ["--text: light-dark(rgb(0 0 0 / 84.7%), rgb(255 255 255 / 84.7%))", "--text-secondary: light-dark(rgb(0 0 0 / 60%), rgb(255 255 255 / 54.9%))",
    "--text-tertiary: light-dark(rgb(0 0 0 / 25.9%), rgb(255 255 255 / 24.7%))", "--fill-1: light-dark(rgb(0 0 0 / 9.8%), rgb(255 255 255 / 9.8%))",
    "--separator: light-dark(rgb(0 0 0 / 10%), rgb(255 255 255 / 12%))", "--surface: light-dark(#ffffff, #1e1e1e)", "--selected: light-dark(#0064e1, #0059d1)"]) expect(tokens).toContain(value);
});

test("a value is declared once: lengths come from the space scale, colours from one primitive", () => {
  const primitives = /^:root \{[^]*?^\}/m.exec(tokens)![0];
  const declared = [...primitives.matchAll(/(--[\w-]+):\s*([^;]+);/g)].map(([, name, value]) => [name!, value!.trim()] as const);
  const scale = new Set(declared.filter(([name]) => name.startsWith("--space-")).map(([, value]) => value));
  expect(declared.filter(([name, value]) => !name.startsWith("--space-") && !name.startsWith("--type-") && scale.has(value))).toEqual([]);
  const hexes = [...primitives.matchAll(/#[0-9a-f]{3,8}\b/gi)].map(([hex]) => hex.toLowerCase());
  expect(hexes.filter((hex, index) => hexes.indexOf(hex) !== index)).toEqual([]);
});

const parameters = /--ibis: (#[0-9a-f]{6}); --ibis-dark: (#[0-9a-f]{6});/.exec(tokens)!;
const white = "#ffffff";

test("Ibis scarlet carries white like every macOS accent, and stays readable in both modes", () => {
  const [, light, dark] = parameters;
  expect(tokens).toContain("--selected: var(--ibis); --primary: var(--selected); --selected-text: #fff;");
  expect(contrast(white, light!), "white on the selection, in both modes").toBeGreaterThanOrEqual(4.5);
  expect(contrast(light!, surfaces[0]!), "accent text, light").toBeGreaterThanOrEqual(4.5);
  expect(contrast(dark!, surfaces[1]!), "accent text, dark").toBeGreaterThanOrEqual(4.5);
});

test("menus, buttons and choice rows use title case, as the Mac does", () => {
  const ftl = read("../../../locales/en-US.ftl");
  const messages = [...ftl.matchAll(/^[a-z][\w-]* = (.+)$/gm)].map(([, text]) => `"${text}"`).join("\n");
  for (const label of ["Clear Search", "Keep Window Open", "Keyboard Shortcuts", "Remove from Citation", "Search All Zotero Libraries", "Choose Library…", "Reload Library",
    "Find Them on This Computer", "Try Sample References", "Try Again", "Use Another Library", "Use Your Own Library", "Look Again", "Choose a Found Library",
    "Add to Citation", "Close Menu", "Copy Only"]) expect(messages).toContain(`"${label}"`);
  for (const label of ["Clear search", "Keep window open", "Keyboard shortcuts", "Remove from citation", "Reload library", "Choose library…",
    "Find them on this computer", "Try sample references", "Try again", "Use another library", "Add to citation", "Close menu", "Copy only"]) expect(messages).not.toContain(`"${label}"`);
  expect(ftl).toMatch(/^about-check = Check for Updates$/m);
});

/** Every word people read, from the one catalogue. */
function userStrings(): [string, string][] {
  return read("../../../locales/en-US.ftl").split("\n").filter((line) => line && !line.startsWith("#"))
    .map((line) => ["en-US.ftl", line.replace(/^[a-z][\w-]* =/, "").replace(/\{[^}]*\}/g, "")]);
}

test("people read typographic apostrophes, never typewriter ones", () => {
  const straight = userStrings().filter(([, text]) => /[A-Za-z]'|'[A-Za-z]/.test(text));
  expect(straight.map(([where, text]) => `${where}: ${text}`)).toEqual([]);
  // The lint reads the catalogue: a planted straight apostrophe would be caught.
  expect(userStrings().some(([, text]) => text.includes("’"))).toBe(true);
});

test("Settings descriptions are sentences that end with a full stop", () => {
  const ftl = read("../../../locales/en-US.ftl");
  const descriptions = [...ftl.matchAll(/^([a-z][\w-]*-description) = (.+)$/gm)];
  expect(descriptions.length).toBeGreaterThan(10);
  expect(descriptions.filter(([, , text]) => !/[.?]$/.test(text!.trim())).map(([, id]) => id)).toEqual([]);
});

test("Settings row labels are sentence case, except Apple's own name for a login item", () => {
  const ftl = read("../../../locales/en-US.ftl");
  const labels = ["general-menu-bar-label", "general-small-surprises-label", "backup-label", "restore-label", "general-paste-label", "about-automatic", "about-check-label",
    "look-appearance", "look-window-size", "look-text-size", "citations-default", "citations-recent", "citations-editor", "citations-editor-address"]
    .map((id) => ftl.match(new RegExp(`^${id} = (.+)$`, "m"))![1]!);
  // After the first word, only names stay capitalised.
  const names = /^(Ibis|Zotero|Mac|BibTeX|GitHub)$/;
  expect(labels.filter((label) => label.split(" ").slice(1).some((word) => /^[A-Z]/.test(word) && !names.test(word)))).toEqual([]);
});

test("every icon is one shape, so translucent colours paint it once", () => {
  const files = readdirSync(root, { recursive: true }).filter((name) => name.endsWith(".tsx") && !name.includes(".test."));
  const split = files.flatMap((name) => [...read(name).matchAll(/<svg[^]*?<\/svg>/g)]
    .filter((match) => (match[0].match(/<(path|circle|rect|line)\b/g) ?? []).length > 1).map(() => name));
  expect(split).toEqual([]);
  const joined = [...read("icons.tsx").matchAll(/\[("[^\]]*")\]/g)].flatMap(([, list]) => list!.split(/",\s*"/).slice(1));
  expect(joined.filter((path) => path.replace(/^"/, "").startsWith("m"))).toEqual([]);
});


test("status colours use the measured system hues only for marks and preserve stronger contrast values", () => {
  for (const [name, light, dark] of [["ok", "#34c759", "#30d158"], ["warning", "#ffcc00", "#ffd600"], ["error", "#ff383c", "#ff4245"]]) {
    expect(pair(`--status-${name}`)).toEqual([light, dark]);
    const more = tokens.slice(tokens.indexOf("@media (prefers-contrast: more) and (forced-colors: none)"));
    const match = more.match(new RegExp(`--status-${name}: light-dark\\((#[a-f0-9]{6}), (#[a-f0-9]{6})\\)`));
    expect(match).toBeTruthy();
    expect(contrast(match![1]!, surfaces[0])).toBeGreaterThanOrEqual(3);
    expect(contrast(match![2]!, surfaces[1])).toBeGreaterThanOrEqual(3);
  }
});

test("the Ibis red parameters are the brand's scarlet", () => {
  const brand = JSON.parse(readFileSync(new FileURL("../../../brand/settings.json", import.meta.url), "utf8")) as { colors?: Record<string, string> } & Record<string, unknown>;
  const scarlet = JSON.stringify(brand).match(/"scarlet":\s*"(#[0-9A-Fa-f]{6})"/)![1]!, dark = JSON.stringify(brand).match(/"scarlet_dark":\s*"(#[0-9A-Fa-f]{6})"/)![1]!;
  expect([param("--ibis"), param("--ibis-dark")]).toEqual([scarlet.toLowerCase(), dark.toLowerCase()]);
});

test("people read the ellipsis character, never three full stops", () => {
  const dots = userStrings().filter(([, text]) => text.includes("..."));
  expect(dots.map(([where, text]) => `${where}: ${text}`)).toEqual([]);
});
