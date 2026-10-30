import assert from "node:assert/strict";
import { test } from "node:test";
import { createDemo } from "../website/demo-model.mjs";

test("key demo searches fields, selects and combines known references in order", () => {
  const demo = createDemo();
  assert.equal(demo.output(), "[@okafor2023]");
  demo.setQuery("okafor ecology 2023");
  assert.equal(demo.found().length, 1);
  demo.add("okafor2021"); demo.add("okafor2023"); demo.add("unknown");
  assert.equal(demo.output(), "[@okafor2021; @okafor2023]");
  for (const [format, expected] of Object.entries({ pandoc: "[@okafor2021; @okafor2023]", cite: "\\cite{okafor2021,okafor2023}", autocite: "\\autocites{okafor2021}{okafor2023}", parencite: "\\parencites{okafor2021}{okafor2023}", textcite: "\\textcites{okafor2021}{okafor2023}", typst: "@okafor2021 @okafor2023", keys: "okafor2021, okafor2023" })) {
    demo.state.format = format; assert.equal(demo.output(), expected, format);
  }
});

test("copied receipt survives later edits and failed copy; paste uses that receipt", () => {
  const demo = createDemo();
  assert.equal(demo.copy(), true);
  demo.setQuery("rivera"); demo.state.format = "cite";
  demo.paste(); assert.equal(demo.state.pasted, "[@okafor2023]");
  assert.equal(demo.copy(), true);
  assert.equal(demo.state.pasted, "[@okafor2023]");
  demo.paste(); assert.equal(demo.state.pasted, "\\cite{rivera2025}");
  demo.setQuery("not present ".repeat(10000));
  assert.equal(demo.copy(), false);
  demo.paste(); assert.equal(demo.state.pasted, "\\cite{rivera2025}");
});

test("close and reopen preserves explicit choices; unknown format refuses output", () => {
  const demo = createDemo(); demo.add("okafor2023"); demo.copy();
  demo.back(); demo.back(); demo.back();
  assert.equal(demo.state.open, false);
  demo.state.open = true;
  assert.deepEqual(demo.state.added, ["okafor2023"]);
  assert.equal(demo.state.copied, "[@okafor2023]");
  demo.state.format = "unknown";
  assert.equal(demo.copy(), false);
  assert.equal(demo.state.copied, "[@okafor2023]");
});
