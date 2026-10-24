#!/usr/bin/env node
// Design tokens are the only source of colours and sizes (docs/design.md, #1033).
// Outside apps/desktop/src/tokens.css, a declaration may not use a raw colour
// (hex, rgb(), hsl() and similar, or a named colour; system colours such as
// Canvas, transparent and currentColor are fine) or a px length other than a
// 0 or ±1px hairline. Comments, strings, url() and at-rule conditions such as
// @media breakpoints (where var() cannot work) are not checked; var()
// fallbacks and custom properties defined outside tokens.css are.
//   node scripts/style-lint.mjs
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

export const ROOTS = ["apps/desktop/src", "website"];
export const TOKEN_FILES = ["apps/desktop/src/tokens.css"];
// Build output, which copies tokens.css; its sources are checked instead.
const GENERATED = ["website/dist"];
const NAMED = new Set(("aliceblue antiquewhite aqua aquamarine azure beige bisque black blanchedalmond blue blueviolet brown burlywood cadetblue chartreuse "
  + "chocolate coral cornflowerblue cornsilk crimson cyan darkblue darkcyan darkgoldenrod darkgray darkgreen darkgrey darkkhaki darkmagenta darkolivegreen "
  + "darkorange darkorchid darkred darksalmon darkseagreen darkslateblue darkslategray darkslategrey darkturquoise darkviolet deeppink deepskyblue dimgray "
  + "dimgrey dodgerblue firebrick floralwhite forestgreen fuchsia gainsboro ghostwhite gold goldenrod gray green greenyellow grey honeydew hotpink indianred "
  + "indigo ivory khaki lavender lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan lightgoldenrodyellow lightgray lightgreen lightgrey "
  + "lightpink lightsalmon lightseagreen lightskyblue lightslategray lightslategrey lightsteelblue lightyellow lime limegreen linen magenta maroon "
  + "mediumaquamarine mediumblue mediumorchid mediumpurple mediumseagreen mediumslateblue mediumspringgreen mediumturquoise mediumvioletred midnightblue "
  + "mintcream mistyrose moccasin navajowhite navy oldlace olive olivedrab orange orangered orchid palegoldenrod palegreen paleturquoise palevioletred "
  + "papayawhip peachpuff peru pink plum powderblue purple rebeccapurple red rosybrown royalblue saddlebrown salmon sandybrown seagreen seashell sienna "
  + "silver skyblue slateblue slategray slategrey snow springgreen steelblue tan teal thistle tomato turquoise violet wheat white whitesmoke yellow yellowgreen").split(" "));
const COLOR_FUNCTION = /(?<![\w-])(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)\(/gi;

/** Takes a balanced function call starting at `start` (its name) and returns its text. */
function call(value, start) {
  let depth = 0;
  for (let i = value.indexOf("(", start); i < value.length; i++) {
    if (value[i] === "(") depth++;
    else if (value[i] === ")" && --depth === 0) return value.slice(start, i + 1);
  }
  return value.slice(start);
}

/** The raw colours and px lengths in one stylesheet, normalised (lower case, single spaces). */
export function rawValues(css) {
  const found = [];
  // One left-to-right pass: whichever of a comment or a string starts first
  // wins, so quotes in comments and comment marks in strings stay inert.
  const text = css.replace(/\/\*[\s\S]*?(?:\*\/|$)|"(?:\\[\s\S]|[^"\\])*"?|'(?:\\[\s\S]|[^'\\])*'?/g, (token) => token[0] === "/" ? " " : '""')
    .replace(/url\([^)]*\)/gi, "url()");
  // Declarations sit between { ; and } ; a segment ending in { is a selector or an at-rule condition.
  for (const match of text.matchAll(/[^{};]*[{};]?/g)) {
    const segment = match[0];
    if (segment.endsWith("{")) continue;
    const colon = segment.indexOf(":");
    if (colon < 0) continue;
    const value = segment.slice(colon + 1).replace(/[};]$/, "").replace(/\s+/g, " ").trim().toLowerCase();
    for (const fn of value.matchAll(COLOR_FUNCTION)) found.push(call(value, fn.index));
    for (const hex of value.matchAll(/#[0-9a-f]{3,8}(?![\w-])/g)) found.push(hex[0]);
    for (const word of value.matchAll(/(?<![\w#.-])[a-z]+(?![\w(-])/g)) if (NAMED.has(word[0])) found.push(word[0]);
    for (const px of value.matchAll(/(?<![\w.#+-])[+-]?(\d*\.?\d+(?:e[+-]?\d+)?)px(?![\w-])/g)) if (![0, 1].includes(Number(px[1]))) found.push(px[0]);
  }
  return found;
}

/** Problems in the stylesheets under `repo`, compared with the allowlist {file: {reason, values: {value: count}}}. */
export function lint(repo, allowlist, roots = ROOTS) {
  const counts = {};
  const walk = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const file = join(directory, entry.name);
      const name = relative(repo, file).split(sep).join("/");
      if (entry.isDirectory()) { if (entry.name !== "node_modules" && !GENERATED.includes(name)) walk(file); continue; }
      if (!entry.name.endsWith(".css") || TOKEN_FILES.includes(name)) continue;
      for (const value of rawValues(readFileSync(file, "utf8"))) {
        counts[name] ??= {};
        counts[name][value] = (counts[name][value] ?? 0) + 1;
      }
    }
  };
  for (const root of roots) if (existsSync(join(repo, root))) walk(join(repo, root));
  const problems = [];
  for (const [file, values] of Object.entries(counts)) {
    for (const [value, count] of Object.entries(values)) {
      const allowed = allowlist[file]?.values?.[value] ?? 0;
      if (count > allowed) problems.push(`${file}: raw ${value}${count - allowed > 1 ? ` (${count - allowed} new)` : ""}; use a token from apps/desktop/src/tokens.css, or add one there (rules: docs/design.md).`);
    }
  }
  for (const [file, { values }] of Object.entries(allowlist)) {
    for (const [value, allowed] of Object.entries(values)) {
      if ((counts[file]?.[value] ?? 0) < allowed) problems.push(`${file}: ${value} is allowed ${allowed} time(s) but used less; lower its count in scripts/style-lint.mjs.`);
    }
  }
  return problems.sort();
}

// Existing raw values, to be moved into tokens by #1033. Counts only shrink.
const reason = "#1033";
export const ALLOWLIST = {
  "apps/desktop/src/features/picker/CitationShelf.css": { reason, values: { "-2px": 1 } },
  "apps/desktop/src/nativeAppearance.css": { reason, values: {
    "rgb(249 249 249 / 92%)": 1, "rgb(40 40 40 / 92%)": 1, "#982d16": 1, "#ff9d87": 1, "#135f29": 1, "#6fca81": 1,
    "#734b00": 1, "#f0c25a": 1, "#9b273b": 1, "#ff9c9c": 1, "30px": 2 } },
  "apps/desktop/src/notice.css": { reason, values: { "8px": 1, "#3a3a3e": 2, "#ffffff": 1, "#ffb39f": 1, "#8ff0a4": 1, "2px": 1 } },
  "apps/desktop/src/styles.css": { reason, values: { "24px": 1 } },
  "apps/desktop/src/ui/Menu.css": { reason, values: {
    "28px": 1, "rgb(246 246 248 / 94%)": 1, "rgb(40 40 44 / 94%)": 1, "#f9f9f9": 1, "#2c2c2c": 1, "rgb(0 0 0 / 5.6%)": 1,
    "rgb(255 255 255 / 8%)": 1, "#fff": 1, "#38383c": 1, "rgb(0 0 6 / 8%)": 1, "rgb(255 255 255 / 10%)": 1, "30px": 2,
    "32px": 2, "-8px": 1, "8px": 1, "16px": 3, "4px": 1, "10px": 3, "6px": 1, "18px": 1 } },
  "apps/desktop/src/ui/ToolbarMenu.css": { reason, values: { "264px": 1, "8px": 1, "4px": 2, "34px": 2, "10px": 3, "30px": 1 } },
};

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const problems = lint(resolve(fileURLToPath(import.meta.url), "../.."), ALLOWLIST);
  if (problems.length) {
    console.error(`Raw style values outside the design tokens:\n  ${problems.join("\n  ")}`);
    process.exitCode = 1;
  }
}
