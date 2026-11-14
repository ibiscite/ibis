#!/usr/bin/env node
// Reads engine-produced scenarios on stdin; no downloads or installed packages.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const fixture = new URL("../fixtures/odf-scan/", import.meta.url);
const records = [
  { id: 11, libraryID: 1, key: "SYNTH234", citationKey: "Synthetic2026", identity: "personal:SYNTH234" },
  { id: 12, libraryID: 1, key: "DECOY234", citationKey: "Synthetic2026Other", identity: "decoy:DECOY234" },
  { id: 21, libraryID: 2, key: "SYNTH234", citationKey: "Synthetic2026", identity: "group7:SYNTH234" },
].map(record => ({ ...record, getField(field) {
  return { ...record, title: "Invented atlas", year: "2026", firstCreator: "" }[field] ?? "";
} }));
let available = records;
const context = vm.createContext({ Zotero: {
  debug() {},
  Libraries: { userLibraryID: 1, get: id => ({ libraryType: id === 1 ? "user" : "group" }) },
  Users: { getCurrentUserID: () => null, getLocalUserKey: () => "invented" },
  Groups: {
    getGroupIDFromLibraryID(id) { assert.equal(Number(id), 2); return 7; },
    getLibraryIDFromGroupID(id) { assert.equal(Number(id), 7); return 2; },
  },
  Items: {
    getAsync: async id => available.find(item => item.id === id),
    getByLibraryAndKeyAsync: async (library, key) => available.find(item => item.libraryID === library && item.key === key),
  },
  Search: class {
    addCondition(field, operation, value) {
      assert.equal(field, "citationKey"); assert.equal(operation, "is"); this.key = value;
    }
    async search() {
      assert.equal(this.libraryID, 1);
      return available.filter(item => item.libraryID === this.libraryID && item.citationKey === this.key).map(item => item.id);
    }
  },
} });
for (const file of ["citationUtils.js", "odfConverter.js"]) {
  vm.runInContext(readFileSync(new URL(file, fixture), "utf8"), context, { filename: file });
}
const utils = context.CitationUtils;
const decode = text => text.replace(/&quot;|&lt;|&gt;|&apos;|&amp;/g,
  entity => ({ "&quot;": '"', "&lt;": "<", "&gt;": ">", "&apos;": "'", "&amp;": "&" })[entity]);
const normalize = item => ({ item: item.item, locator: item.locator ?? "", prefix: item.prefix ?? "",
  suffix: item.suffix ?? "", suppress_author: item.suppress_author ?? false });

async function convert(scenario) {
  available = scenario.available === "decoy" ? records.filter(item => item.id === 12) : records;
  let text = scenario.text;
  if (scenario.syntax === "pandoc") {
    text = await utils.pandocToMarkersText(text);
    if (scenario.available === "decoy") assert.equal(text, scenario.text, "unmatched key stays unresolved");
  }
  // Supply ordinary content.xml text; execute the retained upstream regex and
  // composeCitations method, including its adjacent-marker grouping.
  context.content = `<text:p>Before ${text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")} after.</text:p>`;
  const xml = vm.runInContext(`{
    const scan = new ODFConv(); scan.content = content; scan.rands = {};
    if (scan.content.match(rexTextAll)) scan.composeCitations();
    scan.content;
  }`, context);
  const actual = [];
  // Read the converter's output records, never parse input citation markers here.
  for (const match of xml.matchAll(/<text:reference-mark-start text:name="ZOTERO_ITEM (.*?) RND[^"]+"\/>/g)) {
    const citation = JSON.parse(decode(match[1]));
    const items = [];
    for (const item of citation.citationItems) {
      const resolved = await utils.getItemByURI(item.uris[0]);
      assert.ok(resolved, `${scenario.name}: unresolved ${item.uris[0]}`);
      items.push(normalize({ item: resolved.identity, locator: item.locator.trim(),
        prefix: decode(item.prefix).trim(), suffix: decode(item.suffix).trim(),
        suppress_author: item["suppress-author"] === true }));
    }
    actual.push(items);
  }
  return actual;
}

for (const scenario of JSON.parse(readFileSync(0, "utf8"))) {
  const expected = scenario.expected.map(group => group.map(normalize));
  const actual = await convert(scenario);
  console.log(`${scenario.name}: actual=${JSON.stringify(actual)} expected=${JSON.stringify(expected)}`);
  assert.deepEqual(actual, expected, scenario.name);
  console.log(`${scenario.name}: upstream differential passed`);
  if (scenario.identity_control) {
    const actual = await convert({ ...scenario,
      text: scenario.text.replace("zg:7:SYNTH234", "zu:0:SYNTH234") });
    // Both items exist: a wrong-library marker must resolve, then fail identity.
    assert.equal(actual[0][0].item, "personal:SYNTH234");
    assert.throws(() => assert.deepEqual(actual, expected),
      { code: "ERR_ASSERTION", actual, expected });
    console.log(`${scenario.name}: wrong-library control resolved=${actual[0][0].item} intended=${expected[0][0].item}; identity comparison rejected`);
  }
}
