import assert from "node:assert/strict";
import test from "node:test";
import { lint } from "./pr-lint.mjs";
import { compareSweeps, pinTitle, readSweep, report, styleNotes } from "./style-sweep.mjs";

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

test("text with an empty part is listed for a person to check", () => {
  const next = sweep({ hole: { citations: ["(, 2024, p.)"], bibliography: "Fine." }, fine: output("(Taylor, 2026)") });
  const result = compareSweeps(new Map(), next, []);
  assert.deepEqual(result.suspicious, ["hole"]);
  assert.match(report(result, new Map(), next), /looks incomplete, to check by hand:\*\* `hole`/);
});

test("moved pins name the pull request, and unmoved pins open none", () => {
  const pins = { styles: { tag: "v1.0.16" }, locales: { tag: "v0.0.106" }, zotero: { commit: "a".repeat(40) } };
  assert.equal(pinTitle(pins, pins), null);
  const moved = { styles: { tag: "v1.0.22" }, locales: { tag: "v0.0.107" }, zotero: { commit: "b".repeat(40) } };
  const title = pinTitle(pins, moved);
  assert.equal(title, "fix: update to CSL styles v1.0.22, locales v0.0.107, citeproc bbbbbbb");
  assert.deepEqual(lint({ title, branch: "styles/newest" }).errors, []);
  assert.equal(pinTitle(pins, { ...pins, locales: { tag: "v0.0.107" } }), "fix: update to locales v0.0.107");
});

test("the release notes name each style whose output changed, or say none did", () => {
  const previous = sweep({ same: output("x"), moved: output("x"), gone: output("x") });
  const next = sweep({ same: output("x"), moved: output("y"), fresh: output("z") });
  const notes = styleNotes(compareSweeps(previous, next, []), "v0.3.1");
  assert.match(notes, /^\*\*Citation styles\*\*\n\n- Output changed since v0\.3\.1 in 1 style: `moved`\n- New: `fresh`\n- Removed or renamed[^\n]*: `gone`\n$/);
  assert.equal(styleNotes(compareSweeps(previous, previous, []), "v0.3.1"), "**Citation styles**\n\nEvery style writes what it wrote in v0.3.1.\n");
});
