// Test-only Node oracle over the exact pinned Zotero processor, styles and adapter.
import fs from "node:fs";
import assert from "node:assert/strict";
import CSL from "../vendor/citeproc.mjs";
globalThis.CSL = CSL;
const base = new URL("../vendor/", import.meta.url);
globalThis.__ibisStyles = ["apa", "chicago-author-date", "harvard-cite-them-right", "ieee", "modern-language-association"].map((style) => [style, fs.readFileSync(new URL(`styles/${style}.csl`, base), "utf8")]);
globalThis.__ibisLocale = fs.readFileSync(new URL("locales/en-US.xml", base), "utf8");
const patchCases = ["numeric", "author-date", "label"].map((format) => [format, `<style xmlns="http://purl.org/net/xbiblio/csl"><info><category citation-format='${format}'/></info><macro name="outside"><text variable="citation-number"/></macro><citation><layout><text variable="citation-number"/></layout></citation><bibliography second-field-align='margin'><sort><key variable="citation-number"/></sort><layout><group><text suffix=')' variable='citation-number' prefix='('/><text variable="title"/><text variable="citation-number"></text></group></layout></bibliography></style>`]);
__ibisStyles.push(...patchCases);
const unobservedGetCite = CSL.getCite;
await import("../src/bridge.js");
// Inspect exactly what the bridge hands the processor, without rendering data.
const Engine = CSL.Engine;
try {
  for (const [style] of patchCases) {
    let tree;
    CSL.Engine = function (_, xml) { tree = typeof xml === "string" ? CSL.parseXml(xml) : xml; throw "captured"; };
    assert.throws(() => __ibisFormat({ style, numeric: style === "numeric", items: [], locale: null, keep_order: false }), (error) => error === "captured");
    const bibliography = tree.children.find((node) => node.name === "bibliography");
    assert.equal(bibliography.attrs["second-field-align"], style === "numeric" ? undefined : "margin");
    const group = bibliography.children.find((node) => node.name === "layout").children[0];
    assert.equal(group.children.filter((node) => node.attrs.variable === "citation-number").length, style === "numeric" ? 0 : 2);
    assert.equal(tree.children.find((node) => node.name === "citation").children[0].children[0].attrs.variable, "citation-number");
    assert.equal(tree.children.find((node) => node.name === "macro").children[0].attrs.variable, "citation-number");
    assert.equal(bibliography.children[0].children[0].attrs.variable, "citation-number");
  }
} finally { CSL.Engine = Engine; }
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
  const output = JSON.parse(__ibisFormat({ ...request, numeric: request.style === "ieee" }));
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
