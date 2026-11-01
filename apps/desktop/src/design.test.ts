// The design pipeline's rules: tokens.css is the canvas's values, parts use only
// those, and nothing a person reads falls below its contrast. Pictures of the
// result live in gallery/.
import { expect, test, vi } from "vitest";

const { readFileSync, readdirSync } = await vi.importActual<{
  readFileSync: (path: URL, encoding: string) => string;
  readdirSync: (path: URL, options: { recursive: boolean }) => string[];
}>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const root = new FileURL("./", import.meta.url);
const read = (name: string) => readFileSync(new FileURL(name, root), "utf8");
const names = readdirSync(root, { recursive: true }).filter((name) => name.endsWith(".css"));
const parts = names.filter((name) => name !== "tokens.css");
const strip = (css: string) => css.replace(/\/\*[^]*?\*\//g, "");
const tokens = strip(read("tokens.css"));
/** The tokens with the colour parameters written out, for the contrast checks. */
const param = (name: string) => tokens.match(new RegExp(`${name}: (#[0-9a-f]{6});`))![1]!;
const resolved = tokens.replaceAll("var(--ibis-dark)", param("--ibis-dark")).replaceAll("var(--ibis)", param("--ibis"))
  .replace(/(--ac: (light-dark\([^)]*\));) --ac-selected: var\(--ac\); --ac-text: var\(--ac\);/, "$1 --ac-selected: $2; --ac-text: $2;");
const defined = new Set([...tokens.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((match) => match[1]!));
const website = strip(readFileSync(new FileURL("../../../website/style.css", import.meta.url), "utf8")) + strip(readFileSync(new FileURL("../../../website/demo.css", import.meta.url), "utf8"));
const all = parts.map((name) => [name, strip(read(name))] as const);
const local = new Set([...all, ["website", website] as const].flatMap(([, css]) => [...css.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((match) => match[1]!)));
// Set from script at run time: the native accent and how far a list can scroll.
const fromScript = /^--(native-accent|scroll-fade-|choice-columns)/;
// These two state each system's own materials, so they hold values as tokens.css does.
const values = new Set(["nativeAppearance.css", "nativeAccent.css"]);

test("every token a stylesheet reads is defined", () => {
  const unknown = [...all, ["website", website] as const].flatMap(([name, css]) => [...css.matchAll(/var\((--[a-z0-9-]+)/g)]
    .filter((match) => !defined.has(match[1]!) && !local.has(match[1]!) && !fromScript.test(match[1]!)).map((match) => `${name}: ${match[1]}`));
  expect([...new Set(unknown)]).toEqual([]);
});

test("every token is read by something", () => {
  const scripts = readdirSync(root, { recursive: true }).filter((name) => name.endsWith(".tsx")).map(read);
  const used = [tokens, website, ...all.map(([, css]) => css), ...scripts].join("");
  expect([...defined].filter((token) => !token.startsWith("--website-") && !used.includes(`var(${token}`))).toEqual([]);
});

test("parts take colours, corners and type from tokens", () => {
  const raw = all.filter(([name]) => !values.has(name)).flatMap(([name, css]) => css.split("\n").filter((line) =>
    /(^|[\s:(,])#[0-9a-f]{3,8}\b|rgba?\(/i.test(line)
    || /border-radius:[^;]*\b[1-9]\d*px/.test(line)
    || /font-size:\s*[\d.]+(px|rem)/.test(line)).map((line) => `${name}: ${line.trim().slice(0, 90)}`));
  expect(raw).toEqual([]);
});

type MotionRule = { name: string; selector: string; style: CSSStyleDeclaration; reduced: boolean };
function motionGaps(sheets: readonly (readonly [string, string])[]) {
  const rules: MotionRule[] = [];
  for (const [name, css] of sheets) {
    const sheet = document.createElement("style");
    sheet.textContent = css;
    document.head.append(sheet);
    const walk = (children: CSSRuleList, reduced = false) => {
      for (const rule of children) {
        if (rule.type === CSSRule.STYLE_RULE) {
          const { selectorText, style } = rule as CSSStyleRule;
          for (const selector of selectorText.split(",")) rules.push({ name, selector: selector.trim(), style, reduced });
        } else if (rule.type !== CSSRule.KEYFRAMES_RULE && "cssRules" in rule) {
          const group = rule as CSSMediaRule;
          // Rules restricted to no-preference cannot run with Reduce Motion.
          if (/prefers-reduced-motion:\s*no-preference/.test(group.conditionText ?? "")) continue;
          walk(group.cssRules, reduced || /prefers-reduced-motion:\s*reduce\b/.test(group.conditionText ?? ""));
        }
      }
    };
    try { walk(sheet.sheet!.cssRules); } finally { sheet.remove(); }
  }
  const reducedTokens = new Set(rules.filter((rule) => rule.reduced && rule.selector === ":root")
    .flatMap(({ style }) => Array.from(style).filter((property) => property.startsWith("--"))));
  return rules.flatMap((rule, index) => rule.reduced ? [] : ["animation", "transition"].flatMap((family) => {
    const declarations = Array.from(rule.style).filter((property) =>
      property === family || new RegExp(`^${family}-(name|property|duration|delay|iteration-count)$`).test(property));
    if (!declarations.some((property) => !/^(none|0|0s|initial|unset|revert)$/.test(rule.style.getPropertyValue(property)))) return [];
    const direct = rules.slice(index + 1).some((answer) => answer.reduced && answer.name === rule.name && answer.selector === rule.selector
      && Array.from(answer.style).some((property) => property === family || property === `${family}-duration` || property === `${family}-name` || property === `${family}-property`));
    const token = declarations.some((property) => [...rule.style.getPropertyValue(property).matchAll(/var\((--[\w-]+)/g)]
      .some((match) => reducedTokens.has(match[1]!)));
    return direct || token ? [] : [`${rule.name}: ${rule.selector}: ${family}`];
  }));
}

test("every motion rule has a reduced-motion answer, including nested rules and stylesheets", () => {
  expect(motionGaps(names.map((name) => [name, strip(read(name))] as const))).toEqual([]);
});

test("motion lint rejects missing, unrelated and overridden answers inside nested groups", () => {
  const motion = "@supports (display: grid) { @media (min-width: 1px) { .fixture { transition: transform 200ms; } } }";
  const answer = "@media (prefers-reduced-motion: reduce) { .fixture { transition: none; } }";
  const gaps = ["nested/fixture.css: .fixture: transition"];
  expect(motionGaps([["nested/fixture.css", motion]])).toEqual(gaps);
  expect(motionGaps([["nested/fixture.css", motion + answer.replace(".fixture", ".unrelated")]])).toEqual(gaps);
  expect(motionGaps([["nested/fixture.css", answer + motion]])).toEqual(gaps);
  expect(motionGaps([["nested/fixture.css", motion + answer]])).toEqual([]);
});

test("spinner, switch and key-cap geometry each have one shared owner", () => {
  for (const [selector, owner] of [[".source-spinner", "ui/SourceAction.css"], [".ui-switch-track", "ui/Switch.css"], ["kbd", "ui/KeyCaps.css"]]) {
    const owners = all.filter(([, css]) => [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
      .some(([, head, body]) => (head!.trim() === selector || head!.trim().endsWith(` ${selector}`)) && /(?:min-width|width|height):/.test(body!)))
      .map(([name]) => name);
    expect(owners, selector).toEqual([owner]);
  }
  expect(read("ui/SourceAction.css")).toContain("width: var(--icon-small); height: var(--icon-small)");
  expect(tokens).toContain("--icon-small: var(--space-4)");
});

test("keyboard-focused choices suppress the global outline and keep their fill", () => {
  const choice = read("ui/ChoiceList.css");
  // The extra state selector outranks the global :focus-visible rule even
  // when the application stylesheet is loaded after this shared part.
  expect(choice).toMatch(/\.choice-screen \.choice-row\[data-focus-visible\] \{[^}]*outline: none;[^}]*box-shadow: none;/);
  expect(choice).toMatch(/\.choice-row\[data-focused\] \{ background: var\(--selected\); color: var\(--selected-text\);/);
});

test("a one-sided edge never marks state", () => {
  const edges = all.flatMap(([name, css]) => css.split("\n")
    .filter((line) => /border-(inline|block)-(start|end):[^;]*solid/.test(line) && !/separator|--separator|--stroke-field|transparent/.test(line))
    .map((line) => `${name}: ${line.trim().slice(0, 90)}`));
  expect(edges).toEqual([]);
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
/** A role's light and dark colours; translucent text is flattened onto the window surface. */
const surfaces = ["#ffffff", "#1e1e1e"] as const;
const flatten = (value: string, mode: number) => {
  const rgb = value.match(/rgb\((\d+) (\d+) (\d+) \/ ([\d.]+)%\)/);
  if (!rgb) return value;
  const alpha = Number(rgb[4]) / 100, under = surfaces[mode]!;
  return `#${[1, 2, 3].map((i) => Math.round(Number(rgb[i]) * alpha + Number.parseInt(under.slice(2 * i - 1, 2 * i + 1), 16) * (1 - alpha)).toString(16).padStart(2, "0")).join("")}`;
};
const pair = (token: string) => {
  const match = resolved.match(new RegExp(`${token}: light-dark\\((#[0-9a-f]{6}|rgb\\([^)]*\\)), (#[0-9a-f]{6}|rgb\\([^)]*\\))\\)`));
  expect(match, token).toBeTruthy();
  return [flatten(match![1]!, 0), flatten(match![2]!, 1)] as const;
};

test.each([["--text", 4.5], ["--scarlet", 3]] as const)(
  "%s keeps %s:1 on the surface in light and dark", (token, ratio) => {
    const surface = pair("--surface"), value = pair(token);
    expect(contrast(value[0], surface[0])).toBeGreaterThanOrEqual(ratio);
    expect(contrast(value[1], surface[1])).toBeGreaterThanOrEqual(ratio);
  });

const colour = "(#[0-9a-f]{3,6})";
const roles = [...resolved.matchAll(new RegExp(`:root\\[data-accent="([a-z]+)"\\] \\{ --ac: (?:light-dark\\()?${colour}(?:, ${colour}\\))?; --ac-selected: light-dark\\(${colour}, ${colour}\\); --ac-text: light-dark\\(${colour}, ${colour}\\);( --ac-on: light-dark\\(${colour}, ${colour}\\);)?(?: --ac-fill: var\\(--ac-selected\\);)? \\}`, "g"))];
const white = "#ffffff";
const full = (hex: string) => hex.length === 4 ? `#${hex[1]}${hex[1]}${hex[2]}${hex[2]}${hex[3]}${hex[3]}` : hex;

test("all nine accents have their three roles", () => expect(roles.map((role) => role[1])).toHaveLength(9));

test.each(roles.map((role) => [role[1]!, role] as const))("%s keeps its roles readable in both modes", (_, role) => {
  [0, 1].forEach((mode) => {
    const accent = role[2 + mode] ?? role[2]!, selected = role[4 + mode]!, text = role[6 + mode]!;
    const on = role[9 + mode] ? full(role[9 + mode]!) : white;
    expect(contrast(on, selected), `selection text, mode ${mode}`).toBeGreaterThanOrEqual(4.5);
    expect(contrast(text, surfaces[mode]!), `accent text, mode ${mode}`).toBeGreaterThanOrEqual(4.5);
    expect(contrast(accent, surfaces[mode]!), `accent as a control, mode ${mode}`).toBeGreaterThanOrEqual(3);
    const line = tokens.match(new RegExp(`\\[data-accent="${role[1]}"\\][^\\n]*`))![0];
    expect(contrast(on, line.includes("--ac-fill") ? selected : accent), `primary button, mode ${mode}`).toBeGreaterThanOrEqual(3);
  });
});

test("the ladders hold the values measured from AppKit on macOS 27.2", () => {
  for (const value of ["--text: light-dark(rgb(0 0 0 / 84.7%), rgb(255 255 255 / 84.7%))", "--text-secondary: light-dark(rgb(0 0 0 / 49.8%), rgb(255 255 255 / 54.9%))",
    "--text-tertiary: light-dark(rgb(0 0 0 / 25.9%), rgb(255 255 255 / 24.7%))", "--text-disabled: light-dark(rgb(0 0 0 / 24.7%), rgb(255 255 255 / 24.7%))",
    "--fill-1: light-dark(rgb(0 0 0 / 9.8%), rgb(255 255 255 / 9.8%))", "--fill-4: light-dark(rgb(0 0 0 / 2.7%), rgb(255 255 255 / 2.7%))",
    "--separator: light-dark(rgb(0 0 0 / 10%), rgb(255 255 255 / 12%))", "--surface: light-dark(#ffffff, #1e1e1e)", "--selected: light-dark(#0064e1, #0059d1)"]) expect(tokens).toContain(value);
});

test("menus and buttons use title case, as the Mac does", () => {
  const messages = read("messages.ts");
  for (const label of ["Clear Search", "Keep Window Open", "Show What Will Be Pasted", "Keyboard Shortcuts", "Remove from Citation", "Search All Zotero Libraries", "Choose Library…", "Reload Library"]) expect(messages).toContain(`"${label}"`);
  for (const label of ["Clear search", "Keep window open", "Keyboard shortcuts", "Remove from citation", "Reload library", "Choose library…"]) expect(messages).not.toContain(`"${label}"`);
});

test("a value is declared once: lengths come from the space scale, colours from one primitive", () => {
  const primitives = /^:root \{[^]*?^\}/m.exec(tokens)![0];
  const declared = [...primitives.matchAll(/(--[\w-]+):\s*([^;]+);/g)].map(([, name, value]) => [name!, value!.trim()] as const);
  const scale = new Set(declared.filter(([name]) => name.startsWith("--space-")).map(([, value]) => value));
  const literal = declared.filter(([name]) => !name.startsWith("--space-") && !name.startsWith("--type-"));
  expect(literal.filter(([, value]) => scale.has(value))).toEqual([]);
  const hexes = [...primitives.matchAll(/#[0-9a-f]{3,8}\b/gi)].map(([hex]) => hex.toLowerCase());
  expect(hexes.filter((hex, index) => hexes.indexOf(hex) !== index)).toEqual([]);
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
  expect(all.map(([, css]) => css).join("")).not.toMatch(/color: var\(--(?:done|look|stop)\)/);
  expect(read("ui/SettingsGroup.css")).toMatch(/ui-settings-row-problem[^}]+color: var\(--text\)/);
  expect(read("features/picker/CitationShelf.css")).toMatch(/\.shelf-copied \{[^}]+color: var\(--text\)/);
});

test("the Ibis red parameters are the brand's scarlet", () => {
  const brand = JSON.parse(readFileSync(new FileURL("../../../brand/settings.json", import.meta.url), "utf8")) as { colors?: Record<string, string> } & Record<string, unknown>;
  const scarlet = JSON.stringify(brand).match(/"scarlet":\s*"(#[0-9A-Fa-f]{6})"/)![1]!, dark = JSON.stringify(brand).match(/"scarlet_dark":\s*"(#[0-9A-Fa-f]{6})"/)![1]!;
  expect([param("--ibis"), param("--ibis-dark")]).toEqual([scarlet.toLowerCase(), dark.toLowerCase()]);
});
