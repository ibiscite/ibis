import { expect, test, vi } from "vitest";

// Vitest can load these existing runtime builtins without a Node-types dependency.
const { readFileSync, readdirSync } = await vi.importActual<{
  readFileSync: (path: URL, encoding: string) => string;
  readdirSync: (path: URL) => string[];
}>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");

const here = new FileURL("./", import.meta.url);
const sheets = readdirSync(here).filter((name) => name.endsWith(".css")).sort();
const css = (name: string) => readFileSync(new FileURL(name, here), "utf8");

/** Nothing animates longer than this (docs/design.md, "Never"). */
const LONGEST_MS = 280;
/** Reduced motion keeps fades; anything that moves must be this short. */
const REDUCED_MS = 1;
/** Properties whose change is a fade or tint rather than movement. */
const FADES = new Set(["opacity", "color", "background-color", "background", "border-color", "outline-color"]);

const REDUCED = /@media[^{]*prefers-reduced-motion:\s*reduce[^{]*\{/g;

/** The text of each block that opens at a match of `opener`, braces balanced. */
function blocks(text: string, opener: RegExp): string[] {
  const found: string[] = [];
  for (const match of text.matchAll(opener)) {
    let depth = 1;
    let end = (match.index ?? 0) + match[0].length;
    const start = end;
    while (end < text.length && depth > 0) {
      if (text[end] === "{") depth += 1;
      if (text[end] === "}") depth -= 1;
      end += 1;
    }
    found.push(text.slice(start, end - 1));
  }
  return found;
}

function withoutReduced(text: string): string {
  return blocks(text, REDUCED).reduce((rest, block) => rest.replace(block, ""), text);
}

function custom(text: string): Map<string, string> {
  return new Map([...text.matchAll(/(--[\w-]+)\s*:\s*([^;}]+)/g)].map((m) => [m[1] ?? "", (m[2] ?? "").trim()]));
}

/** Motion tokens as written, and as they are with reduced motion. */
function tokens(text: string) {
  const reducedText = blocks(text, REDUCED).join("\n");
  const normal = custom(withoutReduced(text));
  return { normal, reduced: new Map([...normal, ...custom(reducedText)]) };
}

function milliseconds(value: string, vars: Map<string, string>, depth = 0): number[] {
  const resolved = value.replace(/var\((--[\w-]+)(?:\s*,\s*([^)]+))?\)/g, (_, name: string, fallback?: string) => {
    const found = vars.get(name) ?? fallback;
    if (found === undefined) throw new Error(`unknown motion token ${name}`);
    return depth < 8 ? found : "";
  });
  if (resolved.includes("var(")) return milliseconds(resolved, vars, depth + 1);
  return [...resolved.matchAll(/(?<![\w.-])(\d*\.?\d+)(ms|s)\b/g)].map((m) => Number(m[1]) * (m[2] === "s" ? 1000 : 1));
}

type Motion = { sheet: string; kind: "transition" | "animation"; property: string; value: string; reducedOnly: boolean };

/** Every transition or animation, one entry per comma-separated item. */
function motions(sheet: string, text: string): Motion[] {
  const reducedText = blocks(text, REDUCED).join("\n");
  const found: Motion[] = [];
  const scan = (body: string, reducedOnly: boolean) => {
    for (const m of body.matchAll(/(?<![\w-])(transition|animation)(-duration|-delay)?\s*:\s*([^;}]+)/g)) {
      const kind = m[1] as Motion["kind"];
      for (const item of (m[3] ?? "").split(/,(?![^(]*\))/)) {
        const property = kind === "transition" && !m[2] ? item.trim().split(/\s+/)[0] ?? "all" : "all";
        found.push({ sheet, kind, property, value: item.trim(), reducedOnly });
      }
    }
  };
  scan(withoutReduced(text), false);
  scan(reducedText, true);
  return found;
}

function keyframeProperties(text: string, name: string): string[] {
  return blocks(text, new RegExp(`@keyframes\\s+${name}\\s*\\{`, "g"))
    .flatMap((body) => [...body.matchAll(/([\w-]+)\s*:/g)].map((m) => m[1] ?? ""));
}

/** Problems with the given style sheets, as readable sentences. */
function problems(files: [string, string][]): string[] {
  const all = files.map(([, text]) => text).join("\n");
  const { normal, reduced } = tokens(all);
  const found: string[] = [];
  for (const [sheet, text] of files) {
    for (const motion of motions(sheet, text)) {
      if (/^(none|0s?|0ms)$/.test(motion.value)) continue;
      const total = milliseconds(motion.value, motion.reducedOnly ? reduced : normal).reduce((a, b) => a + b, 0);
      if (total > LONGEST_MS) found.push(`${sheet}: "${motion.value}" lasts ${total} ms`);
      const animationName = motion.kind === "animation" ? motion.value.split(/\s+/).find((word) => /^[a-z][\w-]*$/i.test(word) && !/^(ease|linear|infinite|forwards|backwards|both|alternate|reverse|normal|running|paused)/.test(word)) : undefined;
      const fades = motion.kind === "transition"
        ? FADES.has(motion.property)
        : animationName !== undefined && keyframeProperties(all, animationName).every((p) => FADES.has(p));
      if (fades) continue;
      const reducedTotal = milliseconds(motion.value, reduced).reduce((a, b) => a + b, 0);
      if (reducedTotal > REDUCED_MS) found.push(`${sheet}: "${motion.value}" still moves with reduced motion`);
    }
  }
  return found;
}

test("the design's motion tokens stay within the 280 ms limit", () => {
  const { normal, reduced } = tokens(css("tokens.css"));
  for (const name of ["--motion-feedback", "--motion-panel", "--motion-layout"]) {
    expect(milliseconds(`var(${name})`, normal)[0]).toBeLessThanOrEqual(LONGEST_MS);
  }
  // Panels and layout move; with reduced motion they change at once.
  expect(milliseconds("var(--motion-panel)", reduced)).toEqual([REDUCED_MS]);
  expect(milliseconds("var(--motion-layout)", reduced)).toEqual([REDUCED_MS]);
});

test("no transition or animation is longer than 280 ms or moves with reduced motion", () => {
  expect(sheets).toContain("styles.css");
  expect(problems(sheets.map((name) => [name, css(name)]))).toEqual([]);
});

test("the motion check catches long and moving transitions", () => {
  const tokenSheet = css("tokens.css");
  const check = (rule: string) => problems([["tokens.css", tokenSheet], ["test.css", rule]]);
  expect(check(".a { transition: opacity var(--motion-feedback) var(--motion-ease); }")).toEqual([]);
  expect(check(".a { transition: transform var(--motion-layout) ease; }")).toEqual([]);
  expect(check(".a { transition: opacity 300ms; }")).toHaveLength(1);
  expect(check(".a { transition: opacity 200ms 100ms; }")).toHaveLength(1);
  expect(check(".a { transition: opacity 120ms, transform 0.3s; }")).toHaveLength(2);
  expect(check(".a { transition: transform 120ms; }")).toEqual(['test.css: "transform 120ms" still moves with reduced motion']);
  expect(check(".a { transition-duration: var(--motion-feedback); }")).toHaveLength(1);
  expect(check("@keyframes slide { to { transform: none; } } .a { animation: slide var(--motion-panel); }")).toEqual([]);
  expect(check("@keyframes slide { to { transform: none; } } .a { animation: slide 200ms; }")).toHaveLength(1);
  expect(check("@keyframes appear { to { opacity: 1; } } .a { animation: appear 120ms ease; }")).toEqual([]);
  expect(check("@media (prefers-reduced-motion: reduce) { .a { transition: transform 50ms; } }")).toHaveLength(1);
});
