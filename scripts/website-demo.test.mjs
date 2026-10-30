import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { createDemo, references } from "../website/demo-model.mjs";

test("the website uses exactly the desktop's bundled invented references", async () => {
  const samples = JSON.parse(await readFile(new URL("../apps/desktop/src-tauri/resources/samples.csljson", import.meta.url), "utf8"));
  assert.equal(samples.length, 3);
  assert.deepEqual(references, samples.map(sample => ({
    key: sample["citation-key"], kind: sample.type === "book" ? "book" : "article",
    authors: sample.author.map(author => author.family).join(", "),
    year: sample.issued["date-parts"][0][0], title: sample.title,
    where: sample["container-title"] || sample.publisher,
  })));
});

test("key demo searches fields, selects and combines known samples in order", () => {
  const demo = createDemo();
  assert.equal(demo.output(), "[@vale2024lanterns] [Sample]");
  demo.setQuery("vale landscapes 2024");
  assert.equal(demo.found().length, 1);
  demo.add("fen2023clouds"); demo.add("vale2024lanterns"); demo.add("unknown");
  for (const [format, expected] of Object.entries({ pandoc: "[@fen2023clouds; @vale2024lanterns]", cite: "\\cite{fen2023clouds,vale2024lanterns}", autocite: "\\autocites{fen2023clouds}{vale2024lanterns}", parencite: "\\parencites{fen2023clouds}{vale2024lanterns}", textcite: "\\textcites{fen2023clouds}{vale2024lanterns}", typst: "@fen2023clouds @vale2024lanterns", keys: "fen2023clouds, vale2024lanterns" })) {
    demo.state.format = format; assert.equal(demo.output(), `${expected} [Sample]`, format);
  }
});

test("empty and no-match searches preserve a prepared citation; no selection refuses copy", () => {
  const demo = createDemo();
  demo.setQuery("");
  assert.equal(demo.found().length, 3);
  demo.add("vale2024lanterns");
  demo.setQuery("no invented reference matches this");
  assert.deepEqual(demo.found(), []);
  assert.equal(demo.output(), "[@vale2024lanterns] [Sample]");
  assert.equal(demo.copy(), true);
  demo.add("vale2024lanterns");
  assert.equal(demo.output(), "");
  assert.equal(demo.copy(), false);
  assert.equal(demo.state.copied, "[@vale2024lanterns] [Sample]");
});

test("copied receipt survives later edits and failed copy; paste uses that receipt", () => {
  const demo = createDemo();
  assert.equal(demo.copy(), true);
  demo.setQuery("clouds"); demo.state.format = "cite";
  demo.paste(); assert.equal(demo.state.pasted, "[@vale2024lanterns] [Sample]");
  assert.equal(demo.copy(), true);
  assert.equal(demo.state.pasted, "[@vale2024lanterns] [Sample]");
  demo.paste(); assert.equal(demo.state.pasted, "\\cite{fen2023clouds} [Sample]");
  demo.setQuery("not present ".repeat(10000));
  assert.equal(demo.copy(), false);
  demo.paste(); assert.equal(demo.state.pasted, "\\cite{fen2023clouds} [Sample]");
});

test("close and reopen preserves choices; reload starts the same practice task afresh", () => {
  const demo = createDemo(); demo.add("vale2024lanterns"); demo.copy();
  demo.back(); demo.back(); demo.back();
  assert.equal(demo.state.open, false);
  demo.state.open = true;
  assert.deepEqual(demo.state.added, ["vale2024lanterns"]);
  assert.equal(demo.state.copied, "[@vale2024lanterns] [Sample]");
  demo.state.format = "unknown";
  assert.equal(demo.copy(), false);
  assert.equal(demo.state.copied, "[@vale2024lanterns] [Sample]");
  const reloaded = createDemo();
  assert.equal(reloaded.state.query, "lantern");
  assert.deepEqual(reloaded.state.added, []);
  assert.equal(reloaded.state.copied, null);
  assert.equal(reloaded.state.pasted, null);
  assert.equal(reloaded.output(), "[@vale2024lanterns] [Sample]");
});
