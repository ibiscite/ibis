import assert from "node:assert/strict";
import { test } from "node:test";
import { docProblems, baselineErrors } from "./check-docs-drift.mjs";
const files = ["AGENTS.md", "CONTRIBUTING.md", "docs/guide.md", "docs/next page.md", "scripts/check.mjs", "crates/core/Cargo.toml", "apps/desktop/src/ui/Button.tsx"];
const check = (text, doc = "docs/guide.md") => docProblems(doc, text, files, ["ibis-core"]);

test("checks inline repository paths and document-relative links", () => {
  assert.deepEqual(check("`scripts/check.mjs` [guide](../AGENTS.md#rules) [space](next%20page.md)"), []);
  assert.equal(check("`scripts/missing.mjs` [gone](gone.md)").length, 2);
  assert.deepEqual(check("[web](https://example.invalid/x) [local](#part) `https://example.invalid/x`"), []);
  assert.deepEqual(check("`apps/desktop/src/ui/` `crates/*/Cargo.toml`"), []);
  assert.equal(check("`apps/desktop/src/missing.tsx`").length, 1);
});

test("node and cargo commands name existing scripts and packages without running prose", () => {
  assert.deepEqual(check("```sh\nnode scripts/check.mjs\ncargo +nightly test -p ibis-core\n```"), []);
  assert.equal(check("`node scripts/missing.mjs` `cargo test -p ibis-missing`").length, 2);
  assert.equal(check("node scripts/資料.mjs\r\ncargo test --package=missing").length, 2);
  assert.deepEqual(check("`node scripts/<name>.mjs` `cargo test -p <crate>`"), []);
});

test("empty, large, renamed, Windows and Unicode references are deterministic", () => {
  assert.deepEqual(check(""), []);
  assert.deepEqual(check("`scripts\\check.mjs`\r\n"), []);
  assert.equal(check("`scripts/資料.mjs`").length, 1);
  assert.equal(check("[relative](../AGENTS.md)", "docs/nested/moved.md").length, 1);
  assert.equal(check("ordinary text\n".repeat(100000) + "[bad](missing.md)").length, 1);
});

test("baseline admits only recorded current-main cases and must shed stale entries", () => {
  const problems = check("[old](missing.md)");
  const baseline = Object.fromEntries(problems.map((problem) => [problem, 1]));
  assert.deepEqual(baselineErrors(problems, baseline), []);
  assert.ok(baselineErrors([...problems, ...problems], baseline).length);
  assert.ok(baselineErrors([], baseline).length);
  assert.ok(baselineErrors(check("[new](another.md)"), baseline).length);
});


test("paths with spaces and every package flag are checked, while branch and user paths are not repository paths", () => {
  assert.equal(check("`docs/missing page.md`").length, 1);
  assert.equal(check("`cargo test -p ibis-core -p missing`").length, 1);
  assert.deepEqual(check("`origin/main` `feat/22-example` `%APPDATA%/example/` `~/.config/example/` `IBIS_LIBRARY=/path/to/file.bib`"), []);
});
