import { readdirSync, readFileSync } from "node:fs";
import { iconProblems } from "./check.mjs";

const directory = new URL("../brand/icons/", import.meta.url);
let failed = false;
const files = readdirSync(directory).sort();
if (files.length === 0) throw Error("Missing Ibis icon set");
for (const name of files) {
  const problems = name.endsWith(".svg") ? iconProblems(readFileSync(new URL(name, directory), "utf8")) : ["SVG source required"];
  if (problems.length) { failed = true; console.error(`${name}: ${problems.join("; ")}`); }
}
if (failed) process.exit(1);
console.log(`${files.length} Ibis icons: grid, live area, single path and stroke passed.`);
