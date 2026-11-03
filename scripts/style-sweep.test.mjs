import assert from "node:assert/strict";
import test from "node:test";
import { compareSweeps, readSweep, report } from "./style-sweep.mjs";

const output = (text) => ({ citations: [text], bibliography: `${text}.` });
const sweep = (entries) => new Map(Object.entries(entries));

test("a render reads back by style", () => {
  const read = readSweep(`{"style":"a","output":${JSON.stringify(output("x"))}}\n{"style":"b","output":${JSON.stringify(output("y"))}}\n`);
  assert.deepEqual([...read.keys()], ["a", "b"]);
  assert.deepEqual(read.get("b"), output("y"));
});

test("only a changed output counts as changed, and it is named", () => {
  const previous = sweep({ same: output("x"), moved: output("x"), gone: output("x") });
  const next = sweep({ same: output("x"), moved: output("y"), fresh: output("z") });
  const result = compareSweeps(previous, next, []);
  assert.deepEqual(result.changed, ["moved"]);
  assert.deepEqual(result.removed, ["gone"]);
  assert.deepEqual(result.added, ["fresh"]);
  const text = report(result, previous, next);
  assert.match(text, /Output changed in 1 style\./);
  assert.match(text, /- x\n\+ y/);
});

test("a refusal is not a failure, but an unlisted failure is new", () => {
  const broken = { citations: [{ failed: "formatter unavailable" }], bibliography: "x" };
  const refused = { citations: [{ refused: true }], bibliography: "x" };
  const next = sweep({ broken, known: broken, refused });
  const result = compareSweeps(new Map(), next, ["known"]);
  assert.deepEqual(result.failures, ["broken", "known"]);
  assert.deepEqual(result.newFailures, ["broken"]);
});

test("a fixed exception is reported so the list only shrinks", () => {
  const result = compareSweeps(new Map(), sweep({ healed: output("x") }), ["healed"]);
  assert.deepEqual(result.fixed, ["healed"]);
  assert.match(report(result, new Map(), new Map()), /remove from the exceptions:\*\* `healed`/);
});

test("a very long list stays readable", () => {
  const styles = Array.from({ length: 250 }, (_, index) => `s${index}`);
  const previous = new Map(styles.map((style) => [style, output("x")]));
  const next = new Map(styles.map((style) => [style, output("y")]));
  assert.match(report(compareSweeps(previous, next, []), previous, next), /and 50 more/);
});
