// Test-only Node oracle over the exact pinned Zotero processor, styles and adapter.
import fs from "node:fs";
import assert from "node:assert/strict";
import CSL from "../../../apps/desktop/vendor/csl/citeproc.mjs";
globalThis.CSL = CSL;
const base = new URL("../../../apps/desktop/vendor/csl/", import.meta.url);
globalThis.__ibisStyles = ["apa", "chicago-author-date", "harvard-cite-them-right", "ieee", "modern-language-association"].map((style) => [style, fs.readFileSync(new URL(`styles/${style}.csl`, base), "utf8")]);
globalThis.__ibisLocale = fs.readFileSync(new URL("locales/en-US.xml", base), "utf8");
const unobservedGetCite = CSL.getCite;
await import("../src/bridge.js");
const requests = JSON.parse(fs.readFileSync(0, "utf8"));
const pristine = structuredClone(requests);
const observed = requests.map((request) => JSON.parse(__ibisFormat(request)));
// Independent transparency check: call the pinned processor without the observer.
CSL.getCite = unobservedGetCite;
const expected = pristine.map((request) => {
  let xml = new Map(__ibisStyles).get(request.style).replace(/disambiguate-add-year-suffix="true"/g, 'disambiguate-add-year-suffix="false"');
  if (request.style === "ieee") xml = xml.replace('<text variable="citation-number" prefix="[" suffix="]"/>', "").replace(' second-field-align="flush"', "");
  const items = new Map(request.items.map((item) => [item.id, item]));
  const engine = new CSL.Engine({ retrieveLocale: () => __ibisLocale, retrieveItem: (id) => items.get(id) }, xml, request.locale ?? undefined, request.locale !== null);
  engine.opt.development_extensions.throw_on_empty = true;
  if (request.keep_order) engine.citation_sort.tokens = [];
  const output = {};
  for (const format of ["text", "html", "rtf"]) {
    engine.setOutputFormat(format);
    engine.updateItems([]);
    engine.updateItems(request.items.map((item) => item.id));
    const value = request.bibliography ? engine.makeBibliography()[1].map((text) => text.replace(/\s*\n\s*/g, " ").trim()).join("\n") : engine.makeCitationCluster(request.cites);
    output[format] = format === "rtf" ? `{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0 Times New Roman;}}\\f0\n${request.bibliography ? value.split("\n").join("\\par\n") : value}\n}` : value;
  }
  return output;
});
for (let index = 0; index < expected.length; index++) {
  for (const format of ["text", "html", "rtf"]) assert.ok(observed[index][format] === expected[index][format], `observer changed canonical ${format}`);
}
process.stdout.write(JSON.stringify(expected));
