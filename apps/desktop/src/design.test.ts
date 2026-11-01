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
    .filter((line) => /border-(inline|block)-(start|end):[^;]*solid/.test(line) && !/separator|--line|--stroke|transparent/.test(line))
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
const pair = (token: string) => {
  const match = tokens.match(new RegExp(`${token}: light-dark\\((#[0-9a-f]{6}), (#[0-9a-f]{6})\\)`));
  expect(match, token).toBeTruthy();
  return [match![1]!, match![2]!] as const;
};

test.each([["--text", 4.5], ["--muted", 4.5], ["--tertiary", 3], ["--scarlet", 3], ["--done", 3], ["--look", 3], ["--stop", 3]] as const)(
  "%s keeps %s:1 on the surface in light and dark", (token, ratio) => {
    const surface = pair("--surface"), value = pair(token);
    expect(contrast(value[0], surface[0])).toBeGreaterThanOrEqual(ratio);
    expect(contrast(value[1], surface[1])).toBeGreaterThanOrEqual(ratio);
  });
