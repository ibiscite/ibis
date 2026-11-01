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
const defined = new Set([...tokens.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((match) => match[1]!));
const website = strip(readFileSync(new FileURL("../../../website/style.css", import.meta.url), "utf8")) + strip(readFileSync(new FileURL("../../../website/demo.css", import.meta.url), "utf8"));
const all = parts.map((name) => [name, strip(read(name))] as const);
const local = new Set([...all, ["website", website] as const].flatMap(([, css]) => [...css.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((match) => match[1]!)));
// Set from script at run time: the native accent and how far a list can scroll.
const fromScript = /^--(native-accent|scroll-fade-)/;
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
    || /border-radius:[^;]*\b[1-9]\d*px/.test(line) && !/calc\(/.test(line)
    || /font-size:\s*[\d.]+(px|rem)/.test(line)).map((line) => `${name}: ${line.trim().slice(0, 90)}`));
  expect(raw).toEqual([]);
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
const surfaces = ["#fcfcfd", "#242426"] as const;
const flatten = (value: string, mode: number) => {
  const rgb = value.match(/rgb\((\d+) (\d+) (\d+) \/ ([\d.]+)%\)/);
  if (!rgb) return value;
  const alpha = Number(rgb[4]) / 100, under = surfaces[mode]!;
  return `#${[1, 2, 3].map((i) => Math.round(Number(rgb[i]) * alpha + Number.parseInt(under.slice(2 * i - 1, 2 * i + 1), 16) * (1 - alpha)).toString(16).padStart(2, "0")).join("")}`;
};
const pair = (token: string) => {
  const match = tokens.match(new RegExp(`${token}: light-dark\\((#[0-9a-f]{6}|rgb\\([^)]*\\)), (#[0-9a-f]{6}|rgb\\([^)]*\\))\\)`));
  expect(match, token).toBeTruthy();
  return [flatten(match![1]!, 0), flatten(match![2]!, 1)] as const;
};

test.each([["--text", 4.5], ["--text-secondary", 4.5], ["--text-tertiary", 3], ["--scarlet", 3], ["--done", 3], ["--look", 3], ["--stop", 3]] as const)(
  "%s keeps %s:1 on the surface in light and dark", (token, ratio) => {
    const surface = pair("--surface"), value = pair(token);
    expect(contrast(value[0], surface[0])).toBeGreaterThanOrEqual(ratio);
    expect(contrast(value[1], surface[1])).toBeGreaterThanOrEqual(ratio);
  });

const colour = "(#[0-9a-f]{3,6})";
const roles = [...tokens.matchAll(new RegExp(`:root\\[data-accent="([a-z]+)"\\] \\{ --ac: light-dark\\(${colour}, ${colour}\\); --ac-selected: light-dark\\(${colour}, ${colour}\\); --ac-text: light-dark\\(${colour}, ${colour}\\);( --ac-on: light-dark\\(${colour}, ${colour}\\);)? \\}`, "g"))];
const white = "#ffffff";
const full = (hex: string) => hex.length === 4 ? `#${hex[1]}${hex[1]}${hex[2]}${hex[2]}${hex[3]}${hex[3]}` : hex;

test("all nine accents have their three roles", () => expect(roles.map((role) => role[1])).toHaveLength(9));

test.each(roles.map((role) => [role[1]!, role] as const))("%s keeps its roles readable in both modes", (_, role) => {
  [0, 1].forEach((mode) => {
    const accent = role[2 + mode]!, selected = role[4 + mode]!, text = role[6 + mode]!;
    const on = role[9 + mode] ? full(role[9 + mode]!) : white;
    expect(contrast(on, selected), `selection text, mode ${mode}`).toBeGreaterThanOrEqual(4.5);
    expect(contrast(text, surfaces[mode]!), `accent text, mode ${mode}`).toBeGreaterThanOrEqual(4.5);
    expect(contrast(accent, surfaces[mode]!), `accent as a control, mode ${mode}`).toBeGreaterThanOrEqual(3);
  });
});
