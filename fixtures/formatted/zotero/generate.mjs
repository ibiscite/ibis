// Formatted goldens for Zotero data (#1258). Run from any directory with Node 22+.
// Takes Zotero's own CSL for the invented `formatted-*` items in
// fixtures/zotero/mapping/cases.json (pinned itemToCSLJSON) and formats it with
// the pinned citeproc-js through crates/ibis-csl/tests/node-oracle.mjs. Never
// runs Ibis's Rust mapper or formatter, Zotero, or a real library.
import { execFileSync } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const here = path.dirname(fileURLToPath(import.meta.url));
const cases = JSON.parse(await readFile(path.join(here, "../../zotero/mapping/cases.json"), "utf8"))
  .filter(({ name }) => name.startsWith("formatted-"));
const items = cases.map(({ item }) => item);
const csl = cases.map(({ expected }, index) => ({ ...expected, id: `ibis-${index}` }));
const cites = csl.map(({ id }) => ({ id, prefix: "", suffix: "", "suppress-author": false, "author-only": false }));
const styles = ["apa", "chicago-author-date", "harvard-cite-them-right", "ieee", "modern-language-association"];
const requests = styles.flatMap((style) => ["normal", "bibliography-only"].map((form) => ({ style, form })));
const outputs = JSON.parse(execFileSync(process.execPath, [path.join(here, "../../../crates/ibis-csl/tests/node-oracle.mjs")], {
  input: JSON.stringify(requests.map(({ style, form }) => ({
    style, items: csl, cites, bibliography: form === "bibliography-only" || style === "ieee", keep_order: false, locale: "en-US",
  }))),
}));
const output = `${JSON.stringify({
  items,
  cases: requests.map((request, index) => ({ ...request, ...outputs[index] })),
}, null, 2)}\n`;
const outputPath = path.join(here, "expected.json");
if (process.argv.includes("--check")) {
  if (await readFile(outputPath, "utf8") !== output) throw new Error("Zotero formatted fixture drift");
} else {
  await writeFile(outputPath, output);
}
console.log(`${items.length} invented Zotero items; ${requests.length} formatted cases`);
