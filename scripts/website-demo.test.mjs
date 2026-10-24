import assert from "node:assert/strict";
import { test } from "node:test";
import { createDemo } from "../website/demo-model.mjs";

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
