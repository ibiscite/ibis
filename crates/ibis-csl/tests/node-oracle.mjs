// Test-only Node oracle over the exact pinned Zotero processor, styles and adapter.
import fs from "node:fs";
import assert from "node:assert/strict";
import CSL from "../vendor/citeproc.mjs";
globalThis.CSL = CSL;
const base = new URL("../vendor/", import.meta.url);
globalThis.__ibisStyles = ["apa", "chicago-author-date", "harvard-cite-them-right", "ieee", "modern-language-association"].map((style) => [style, fs.readFileSync(new URL(`styles/${style}.csl`, base), "utf8")]);
globalThis.__ibisLocale = fs.readFileSync(new URL("locales/en-US.xml", base), "utf8");
const unobservedGetCite = CSL.getCite;
await import("../src/bridge.js");
// CSL inline strings use the same documented whitespace contract before either
// processor. Keep this independent regex oracle separate from Rust's traversal.
const requests = JSON.parse(fs.readFileSync(0, "utf8"), (_, value) => typeof value === "string" ? value.replace(/[\r\n\u0085\u2028\u2029]+/gu, " ").replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u0084\u0086-\u009f]/gu, " ") : value);
const pristine = structuredClone(requests);
const updateItems = CSL.Engine.prototype.updateItems;
let registrations = 0;
CSL.Engine.prototype.updateItems = function (ids, ...args) {
  if (ids.length) registrations++;
  return updateItems.call(this, ids, ...args);
};
const observed = requests.map((request) => {
  const before = registrations;
  const output = JSON.parse(__ibisFormat(request));
  assert.equal(registrations - before, 1, "one item-registration pass per request");
  return output;
});
CSL.Engine.prototype.updateItems = updateItems;
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
