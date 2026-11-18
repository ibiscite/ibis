import assert from "node:assert/strict";
import test from "node:test";
import { sha256 } from "./csl-bundle.mjs";
import { fixtureHashes } from "./csl-suite.mjs";

test("fixtures are hashed in name order and only .txt files count", () => {
  const files = { "name_B.txt": "b", "affix_A.txt": "a", "README.md": "notes" };
  assert.deepEqual(Object.entries(fixtureHashes(Object.keys(files), (name) => Buffer.from(files[name]))), [
    ["affix_A.txt", sha256(Buffer.from("a"))],
    ["name_B.txt", sha256(Buffer.from("b"))],
  ]);
});

test("a fixture name that could leave its folder is refused", () => {
  for (const name of ["../escape.txt", ".hidden.txt", "a b.txt"]) {
    assert.throws(() => fixtureHashes([name], () => Buffer.from("")), /not a fixture name/);
  }
});
