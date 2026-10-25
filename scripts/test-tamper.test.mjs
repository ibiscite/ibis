import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { test } from "node:test";
import { testDiff, testTamper } from "./test-tamper.mjs";

const check = (filename, patch, body = "") => testTamper([{ filename, patch }], body);

test("requires a reason for deleted assertions and disabled tests", () => {
  for (const line of ["-assert_eq!(actual, expected);", "-  expect(actual).toBe(expected);", "-self.assertEqual(a, b)", "+#[ignore]", "+test.skip('case', () => {});", "+describe.only('case', () => {});", "+todo!();"]) {
    const result = check("crates/example/tests/guard.rs", `@@ -1 +1 @@\n${line}`);
    assert.match(result[0], /Test changes:.*explain/i, line);
  }
  assert.deepEqual(check("scripts/example.test.mjs", "-assert.equal(a, b)", "Test changes: replace the old contract assertion."), []);
});

test("expected data edits include both sides of renames", () => {
  for (const filename of ["fixtures/expected.txt", "crates/example/proptest-regressions/input.txt", "crates/example/snapshots/case.snap", "fixtures\\例\\expected.json"]) {
    assert.equal(check(filename, "+expected output").length, 1, filename);
    assert.equal(check(filename, "").length, 1, "rename without patch still changes expected data");
  }
});

test("empty, harmless and large diffs stay deterministic", () => {
  assert.deepEqual(testTamper([], ""), []);
  assert.deepEqual(check("src/example.ts", "+const skip = 1;"), []);
  assert.deepEqual(check("scripts/example.test.mjs", "-const assertMessage = 1;\n+test('case', () => {});"), []);
  const patch = "+// harmless\n".repeat(100000) + "+it.only('case', () => {});";
  assert.equal(check("tests/資料.test.ts", patch).length, 1);
  assert.equal(check("tests/資料.test.ts", "+it.skip('case', () => {});\r\n").length, 1);
});

test("an explanation must be visible and contain a reason", () => {
  for (const body of ["Test changes:", "Test changes:   ", "Test changes: .", "Test changes: ----", "<!-- Test changes: hidden -->", "```\nTest changes: example\n```", "Test changes: <!-- hidden -->"]) {
    assert.equal(check("tests/case.test.ts", "+it.skip('case', () => {});", body).length, 1, body);
  }
  assert.deepEqual(check("tests/case.test.ts", "+it.skip('case', () => {});", "Test changes: remove a redundant test.\r\n"), []);
});


test("shallow merge checkout checks only PR changes and catches renamed expected data", (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-tamper-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const source = join(root, "source"), shallow = join(root, "shallow");
  mkdirSync(source);
  const env = { ...process.env };
  for (const key of Object.keys(env)) if (key.startsWith("GIT_")) delete env[key];
  const run = (cwd, ...args) => {
    const result = spawnSync("git", ["-c", "core.hooksPath=", "-c", "commit.gpgsign=false", "-c", "user.name=Synthetic", "-c", "user.email=synthetic@example.invalid", ...args], { cwd, env, encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout;
  };
  run(source, "init", "-q", "-b", "main");
  mkdirSync(join(source, "fixtures"));
  writeFileSync(join(source, "fixtures/資料 expected.txt"), "synthetic\n");
  writeFileSync(join(source, ":(literal)case.test.mjs"), "assert.equal(1, 1);\n");
  run(source, "add", "."); run(source, "commit", "-qm", "base");
  run(source, "switch", "-qc", "change");
  run(source, "mv", "fixtures/資料 expected.txt", "moved.txt");
  run(source, "--literal-pathspecs", "rm", "-q", "--", ":(literal)case.test.mjs");
  run(source, "commit", "-qm", "rename");
  const head = run(source, "rev-parse", "HEAD").trim();
  run(source, "switch", "-q", "main");
  writeFileSync(join(source, "new.test.mjs"), "assert.equal(1, 1);\n");
  run(source, "add", "."); run(source, "commit", "-qm", "main assertion");
  const base = run(source, "rev-parse", "HEAD").trim();
  run(source, "merge", "--no-ff", "-qm", "merge", "change");
  run(root, "clone", "-q", "--depth=1", pathToFileURL(source).href, shallow);
  const event = { pull_request: { base: { sha: base }, head: { sha: head } } };
  const calls = [];
  const files = testDiff(event, (args) => { calls.push(args); return run(shallow, ...args); });
  assert.ok(calls.some((args) => args[0] === "fetch"), "missing parent is obtained from the existing origin");
  assert.deepEqual(files.map((file) => file.filename), [":(literal)case.test.mjs", "fixtures/資料 expected.txt"]);
  const cwd = process.cwd();
  try {
    process.chdir(source);
    const literalFile = testDiff(event).find((file) => file.filename.startsWith(":"));
    assert.match(literalFile.patch, /-assert.equal/);
    assert.equal(testTamper([literalFile]).length, 1);
  } finally { process.chdir(cwd); }
  assert.equal(testTamper(files).length, 1);
  assert.throws(() => testDiff({ pull_request: { base: { sha: "bad" } } }), /valid PR base\/head/);
  assert.throws(() => testDiff(event, () => "parent " + base + "\n"), /merge checkout/);
  assert.throws(() => testDiff(null, () => { throw Error("unavailable"); }), /unavailable/);
  assert.deepEqual(testDiff(null, (args) => args[0] === "merge-base" ? base : ""), []);
});


test("matcher-only deletion still removes an assertion", () => {
  for (const matcher of [".toEqual(expected)", ".not.toBe(expected)", ".rejects.toThrow()", ".resolves.toEqual(expected)"]) {
    assert.equal(check("tests/case.test.ts", `@@ -2 +2 @@\n-${matcher};\n+;`).length, 1, matcher);
  }
});
