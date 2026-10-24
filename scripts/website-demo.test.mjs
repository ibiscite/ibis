import assert from "node:assert/strict";
import { test } from "node:test";
import { citationPieces, createDemo, formats } from "../website/demo-model.mjs";

const canonical = {
  pandoc: ["[@okafor2021]", "[@okafor2021; @okafor2023]"],
  autocite: ["\\autocite{okafor2021}", "\\autocites{okafor2021}{okafor2023}"],
  parencite: ["\\parencite{okafor2021}", "\\parencites{okafor2021}{okafor2023}"],
  textcite: ["\\textcite{okafor2021}", "\\textcites{okafor2021}{okafor2023}"],
  cite: ["\\cite{okafor2021}", "\\cite{okafor2021,okafor2023}"],
  typst: ["@okafor2021", "@okafor2021 @okafor2023"],
  keys: ["okafor2021", "okafor2021, okafor2023"],
};

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
test("every format keeps its canonical output while each owned piece cites exactly one reference", () => {
  assert.deepEqual(Object.keys(canonical), Object.keys(formats));
  const demo = createDemo();
  demo.add("okafor2021"); demo.add("okafor2023");
  for (const [format, [one, two]] of Object.entries(canonical)) {
    demo.state.format = format;
    assert.equal(demo.output(), two, format);
    assert.equal(demo.pieces().map(piece => piece.text).join(""), two, `${format} joins the same pieces`);
    const owned = demo.pieces().filter(piece => piece.key);
    assert.deepEqual(owned.map(piece => piece.key), ["okafor2021", "okafor2023"], `${format} owns keys in added order`);
    for (const piece of owned) assert.ok(piece.text.includes(piece.key) && !piece.text.includes(piece.key === "okafor2021" ? "okafor2023" : "okafor2021"), `${format} pieces never share a key`);
    assert.ok(demo.pieces().every(piece => piece.text), `${format} has no empty literals`);
    assert.equal(citationPieces(format, ["okafor2021"]).map(piece => piece.text).join(""), one, format);
  }
  demo.state.format = "autocite";
  assert.deepEqual(demo.pieces(), [{ text: "\\autocites" }, { text: "{okafor2021}", key: "okafor2021" }, { text: "{okafor2023}", key: "okafor2023" }], "adjacent commands own their braces");
});
test("the unadded selection previews without offering removal", () => {
  const demo = createDemo();
  assert.deepEqual(demo.pieces(), [{ text: "[" }, { text: "@okafor2023" }, { text: "]" }]);
  demo.state.query = "nothing like this";
  assert.deepEqual(demo.pieces(), []);
});
test("removal targets one synthetic key and keeps the copied receipt", () => {
  const demo = createDemo();
  for (const key of ["okafor2021", "rivera2025", "okafor2023"]) demo.add(key);
  demo.copy(); demo.paste();
  demo.remove("rivera2025");
  assert.deepEqual(demo.state.added, ["okafor2021", "okafor2023"]);
  assert.equal(demo.output(), "[@okafor2021; @okafor2023]");
  assert.equal(demo.state.copied, "[@okafor2021; @rivera2025; @okafor2023]", "removal never rewrites the copied value");
  assert.equal(demo.state.pasted, demo.state.copied);
  demo.remove("rivera2025"); demo.remove("not-a-key");
  assert.deepEqual(demo.state.added, ["okafor2021", "okafor2023"], "absent and unknown keys are ignored");
  demo.remove("okafor2021"); demo.remove("okafor2023");
  assert.equal(demo.output(), "[@okafor2023]", "an empty shelf falls back to the current row");
  assert.equal(demo.state.open, true);
});
test("empty and huge unmatched input never copies an invalid reference", () => {
  const demo = createDemo();
  demo.state.query = "not present ".repeat(10000);
  assert.equal(demo.found().length, 0);
  assert.equal(demo.output(), "");
  assert.equal(demo.copy(), false);
  assert.equal(demo.state.copied, null);
});
