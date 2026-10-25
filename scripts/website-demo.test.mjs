import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import { createDemo, splitLocatorQuery } from "../website/demo-model.mjs";

test("demo adds in order and captures copied output independently of later edits", () => {
  const demo = createDemo();
  assert.equal(demo.output(), "[@okafor2023]");
  demo.add("okafor2021"); demo.add("okafor2023");
  assert.equal(demo.output(), "[@okafor2021; @okafor2023]");
  demo.copy(); demo.state.format = "cite";
  assert.equal(demo.state.open, true, "website example stays available after copying");
  assert.equal(demo.output(), "\\cite{okafor2021,okafor2023}");
  demo.paste();
  assert.equal(demo.state.pasted, "[@okafor2021; @okafor2023]");
  demo.add("okafor2021");
  assert.equal(demo.output(), "\\cite{okafor2023}");
  demo.copy();
  assert.equal(demo.state.pasted, "[@okafor2021; @okafor2023]", "copying again preserves the previous paste");
  demo.paste(); assert.equal(demo.state.pasted, "\\cite{okafor2023}");
  demo.copy(); assert.equal(demo.state.pasted, demo.state.copied, "same-value copies preserve the receipt");
});
test("demo searches across fields and steps back without discarding added references", () => {
  const demo = createDemo();
  demo.state.query = "okafor ecology 2023";
  assert.equal(demo.found().length, 1);
  demo.add("okafor2023");
  demo.back(); assert.equal(demo.state.preview, false);
  demo.back(); assert.equal(demo.state.query, "");
  demo.back(); assert.equal(demo.state.open, false);
  assert.deepEqual(demo.state.added, ["okafor2023"]);
});
test("empty and huge unmatched input never copies an invalid reference", () => {
  const demo = createDemo();
  demo.state.query = "not present ".repeat(10000);
  assert.equal(demo.found().length, 0);
  assert.equal(demo.output(), "");
  assert.equal(demo.copy(), false);
  assert.equal(demo.state.copied, null);
});

const locatorQueries = JSON.parse(await readFile(new URL("../fixtures/locator-queries.json", import.meta.url), "utf8"));
test("demo locator interpretation follows the shared core fixture", () => {
  for (const { query, search, locator } of locatorQueries) {
    const parsed = splitLocatorQuery(query);
    assert.equal(parsed?.search ?? query, search, query);
    assert.deepEqual(parsed?.locator ?? null, locator, query);
  }
});
test("locator tails stay bounded and reject ambiguous or malformed values", () => {
  for (const query of ["title S 12", "title n 50", "title p IIV", "title p 12-", "title, 2020", "title p 9999999999", `title pp ${"1, ".repeat(30)}2`, `title p ${" ".repeat(65)}12`]) {
    assert.equal(splitLocatorQuery(query), null, query);
  }
  for (const [tail, label] of [["¶¶4", "paragraph"], ["chapter IV", "chapter"], ["pgs 3, 7 & 9a", "page"], ["notes 1ff", "note"], ["sections 2 — 4", "section"]]) {
    assert.equal(splitLocatorQuery(`title ${tail}`).locator.label, label, tail);
  }
  assert.equal(splitLocatorQuery(`${"研究 ".repeat(10000)}p. 12`).locator.value, "12");
});
