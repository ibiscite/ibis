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

test("demo freezes each added locator and changes it only through explicit actions", () => {
  const demo = createDemo();
  demo.setQuery("okafor 2023 p. 12");
  assert.equal(demo.found()[0].key, "okafor2023");
  demo.add("okafor2023");
  demo.setQuery("nakamura ch. III"); demo.add("nakamura2024");
  assert.equal(demo.output(), "[@okafor2023, p. 12; @nakamura2024, chap. III]");
  demo.setQuery("okafor 2023 p. 19");
  assert.equal(demo.output(), "[@okafor2023, p. 12; @nakamura2024, chap. III]");
  demo.applyLocator("okafor2023");
  assert.equal(demo.output(), "[@okafor2023, p. 19; @nakamura2024, chap. III]");
  demo.removeLocator("okafor2023");
  assert.equal(demo.output(), "[@okafor2023; @nakamura2024, chap. III]");
  assert.deepEqual(demo.state.added, ["okafor2023", "nakamura2024"]);
  demo.copy(); demo.add("okafor2023"); demo.add("okafor2023");
  assert.equal(demo.output(), "[@nakamura2024, chap. III; @okafor2023, p. 19]");
  demo.paste(); assert.equal(demo.state.pasted, "[@okafor2023; @nakamura2024, chap. III]");
});
test("search as typed preserves the query and editing restores interpretation", () => {
  const demo = createDemo();
  demo.setQuery("okafor p. 12");
  assert.equal(demo.interpretation().typed, "p. 12");
  assert.equal(demo.output(), "[@okafor2023, p. 12]");
  demo.searchAsTyped();
  assert.equal(demo.state.query, "okafor p. 12"); assert.equal(demo.interpretation(), null);
  assert.equal(demo.found().length, 0); assert.equal(demo.copy(), false);
  demo.setQuery("okafor p. 12");
  assert.equal(demo.interpretation().locator.value, "12");
});
test("every demo key format carries frozen contexts or warns when unsupported", () => {
  const demo = createDemo();
  demo.setQuery("okafor 2023 pp. 12–14"); demo.add("okafor2023");
  demo.setQuery("nakamura ch. III"); demo.add("nakamura2024");
  const outputs = {
    pandoc: "[@okafor2023, pp. 12-14; @nakamura2024, chap. III]",
    autocite: "\\autocites[12--14]{okafor2023}[chap.~III]{nakamura2024}",
    parencite: "\\parencites[12--14]{okafor2023}[chap.~III]{nakamura2024}",
    textcite: "\\textcites[12--14]{okafor2023}[chap.~III]{nakamura2024}",
    cite: "\\cite[pp.~12--14]{okafor2023}, \\cite[chap.~III]{nakamura2024}",
    typst: "@okafor2023[pp. 12–14] @nakamura2024[chap. III]", keys: "okafor2023, nakamura2024",
  };
  for (const [format, output] of Object.entries(outputs)) {
    demo.state.format = format;
    assert.equal(demo.output(), output, format);
    assert.equal(demo.rendered().warnings.length, ["keys", "cite"].includes(format) ? 1 : 0, format);
  }
  demo.setQuery("okafor pp 3, 7 & 9a"); demo.applyLocator("okafor2023");
  demo.state.format = "pandoc"; assert.match(demo.output(), /7 \\& 9a/);
  demo.state.format = "autocite"; assert.match(demo.output(), /7 \\& 9a/);
});

test("demo refuses invalid rendered control text and recovers after locator removal", () => {
  const demo = createDemo();
  demo.setQuery("okafor pp 3,\n7"); demo.add("okafor2023");
  assert.equal(demo.output(), ""); assert.equal(demo.copy(), false);
  assert.deepEqual(demo.rendered().warnings, ["text contains a control character"]);
  demo.removeLocator("okafor2023");
  assert.equal(demo.output(), "[@okafor2023]"); assert.equal(demo.copy(), true);
});

test("locator byte bounds and Unicode whitespace match the core", () => {
  const tail = `pp ${"1, ".repeat(20)}1`;
  assert.equal(Buffer.byteLength(tail), 64);
  assert.equal(splitLocatorQuery(`title ${tail}`).locator.value, tail.slice(3));
  assert.equal(splitLocatorQuery(`title ${tail}1`), null);
  assert.equal(splitLocatorQuery("title\u0085p 12").locator.value, "12");
  assert.equal(splitLocatorQuery("title\ufeffp 12"), null);
});


test("an unmatched search keeps chosen references from the loaded demo library", () => {
  const demo = createDemo();
  demo.add("okafor2023");
  demo.setQuery("unmatched synthetic search");
  assert.equal(demo.found().length, 0);
  assert.equal(demo.output(), "[@okafor2023]");
  assert.equal(demo.copy(), true);
});
