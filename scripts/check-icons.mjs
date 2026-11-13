import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/** Lucide outlines stay on the 24 unit grid, with room for the source stroke.
 * Curves use bounded control points too: no hidden transform can move them. */
export function iconProblems(svg) {
  const problems = [];
  const expected = { xmlns: "http://www.w3.org/2000/svg", width: "24", height: "24", viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", "stroke-width": "2", "stroke-linecap": "round", "stroke-linejoin": "round" };
  const root = svg.match(/^<svg\s+([^>]+)><path d="([^"]+)"\s*\/><\/svg>\s*$/);
  if (!root) return ["one svg and one untransformed path required"];
  // Every attribute counts, however it is quoted: nothing unread can move the drawing.
  const attribute = /([\w:-]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/g;
  const attributes = [...root[1].matchAll(attribute)].map(([, key, double, single]) => [key, double ?? single]);
  const unread = root[1].replace(attribute, "").trim();
  if (unread || attributes.length !== Object.keys(expected).length || new Set(attributes.map(([key]) => key)).size !== attributes.length || attributes.some(([key, value]) => expected[key] !== value)) problems.push("24 unit grid, no fill, 2 unit source stroke, round caps and joins required");
  const tokens = root[2].match(/[a-z]|[-+]?(?:\d*\.\d+|\d+\.?\d*)/gi) ?? [];
  if (root[2].replace(/[a-z]|[-+]?(?:\d*\.\d+|\d+\.?\d*)|[\s,]/gi, "")) problems.push("invalid path data");
  const counts = { M: 2, L: 2, H: 1, V: 1, C: 6, S: 4, Q: 4 };
  let index = 0, command = "", x = 0, y = 0, startX = 0, startY = 0;
  let controlX = 0, controlY = 0, previous = "";
  const within = (a, b) => a >= 1 && a <= 23 && b >= 1 && b <= 23;
  if (tokens[0] !== "M") problems.push("path must start with absolute Move");
  while (index < tokens.length) {
    if (/^[a-z]$/i.test(tokens[index])) command = tokens[index++];
    const upper = command.toUpperCase();
    if (upper === "Z") { x = startX; y = startY; command = ""; previous = "Z"; continue; }
    const count = counts[upper];
    if (!count) { problems.push("unsupported path command"); break; }
    const values = tokens.slice(index, index + count).map(Number);
    if (values.length !== count || values.some((v) => !Number.isFinite(v))) { problems.push("incomplete path command"); break; }
    index += count;
    const relative = command !== upper;
    if (upper === "H") x = values[0] + (relative ? x : 0);
    else if (upper === "V") y = values[0] + (relative ? y : 0);
    else {
      const originX = x, originY = y;
      if (upper === "S" && ["C", "S"].includes(previous) && !within(2 * x - controlX, 2 * y - controlY)) problems.push("path leaves room for less than half the source stroke");
      for (let i = 0; i < count; i += 2) {
        x = values[i] + (relative ? originX : 0); y = values[i + 1] + (relative ? originY : 0);
        if (!within(x, y)) problems.push("path leaves room for less than half the source stroke");
      }
      if (["C", "S"].includes(upper)) {
        controlX = values[count - 4] + (relative ? originX : 0); controlY = values[count - 3] + (relative ? originY : 0);
      }
    }
    if (!within(x, y)) problems.push("path leaves room for less than half the source stroke");
    if (upper === "M") { startX = x; startY = y; command = relative ? "l" : "L"; }
    previous = upper;
  }
  return [...new Set(problems)];
}

function main() {
  const directory = new URL("../brand/icons/", import.meta.url);
  let failed = false;
  const entries = readdirSync(directory);
  if (!entries.includes("LICENSE-lucide")) throw Error("Missing Lucide licence and notices");
  const files = entries.filter((name) => name !== "LICENSE-lucide").sort();
  if (files.length === 0) throw Error("Missing Ibis icon set");
  for (const name of files) {
    const problems = name.endsWith(".svg") ? iconProblems(readFileSync(new URL(name, directory), "utf8")) : ["SVG source required"];
    if (problems.length) { failed = true; console.error(`${name}: ${problems.join("; ")}`); }
  }
  if (failed) process.exit(1);
  console.log(`${files.length} Ibis icons: grid, stroke clearance, single path and stroke passed.`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
