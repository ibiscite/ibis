#!/usr/bin/env node
// Copy design parameters chosen on the canvas into the one source of truth.
// Usage: node apps/desktop/canvas/apply-params.mjs --radius=10px --unit=.25rem --type-base=.8125rem --ibis=#c23a1c --ibis-dark=#f2552f
// tokens.css holds every parameter; the two reds also live in brand/settings.json
// (run `python3 brand/build.py` afterwards so the brand exports follow).
import { readFileSync, writeFileSync } from "node:fs";

const names = ["unit", "radius", "type-base", "ibis", "ibis-dark"];
const tokensPath = new URL("../src/tokens.css", import.meta.url);
const brandPath = new URL("../../../brand/settings.json", import.meta.url);
const valid = /^(#[0-9a-fA-F]{6}|-?\d*\.?\d+(px|rem))$/;

const params = Object.fromEntries(process.argv.slice(2).map((arg) => {
  const [, name, value] = arg.match(/^--([a-z-]+)=(.+)$/) ?? [];
  if (!names.includes(name) || !valid.test(value)) throw new Error(`Unknown parameter or value: ${arg} (parameters: ${names.join(", ")})`);
  return [name, value];
}));

let tokens = readFileSync(tokensPath, "utf8");
const block = tokens.match(/\/\* Parameters:[^]*?\n\}/)?.[0];
if (!block) throw new Error("tokens.css has no Parameters block");
let next = block;
for (const [name, value] of Object.entries(params)) {
  const pattern = new RegExp(`(--${name}: )[^;]+;`);
  if (!pattern.test(next)) throw new Error(`tokens.css Parameters block has no --${name}`);
  next = next.replace(pattern, `$1${value.toLowerCase()};`);
}
writeFileSync(tokensPath, tokens.replace(block, next));

const reds = { ibis: "scarlet", "ibis-dark": "scarlet_dark" };
if (params.ibis || params["ibis-dark"]) {
  let brand = readFileSync(brandPath, "utf8");
  for (const [name, key] of Object.entries(reds)) {
    if (params[name]) brand = brand.replace(new RegExp(`("${key}": )"#[0-9A-Fa-f]{6}"`), `$1"${params[name].toUpperCase()}"`);
  }
  writeFileSync(brandPath, brand);
}
console.log(`Applied ${Object.keys(params).length} parameter(s).${params.ibis || params["ibis-dark"] ? " Now run: python3 brand/build.py" : ""}`);
