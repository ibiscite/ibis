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
  for (const body of ["Test changes:", "Test changes:   ", "Test changes: .", "Test changes: ----", "Test changes: <br>", "\tTest changes: example", "<!-- Test changes: hidden -->", "```\nTest changes: example\n```", "Test changes: <!-- hidden -->"]) {
    assert.equal(check("tests/case.test.ts", "+it.skip('case', () => {});", body).length, 1, body);
  }
  assert.deepEqual(check("tests/case.test.ts", "+it.skip('case', () => {});", "Test changes: remove a redundant test.\r\n"), []);
});


test("shallow merge checkout checks only PR changes and catches renamed expected data", (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-tamper-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const source = join(root, "source"), shallow = join(root, "shallow");
  // Windows forbids colons in filenames; POSIX exercises Git pathspec magic.
  const tricky = process.platform === "win32" ? "literal-case.test.mjs" : ":(literal)case.test.mjs";
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
  writeFileSync(join(source, tricky), "assert.equal(1, 1);\n");
  run(source, "add", "."); run(source, "commit", "-qm", "base");
  const initial = run(source, "rev-parse", "HEAD").trim();
  run(source, "switch", "-qc", "change");
  run(source, "mv", "fixtures/資料 expected.txt", "moved.txt");
  run(source, "--literal-pathspecs", "rm", "-q", "--", tricky);
  run(source, "commit", "-qm", "rename");
  const head = run(source, "rev-parse", "HEAD").trim();
  run(source, "switch", "-q", "main");
  writeFileSync(join(source, "new.test.mjs"), "assert.equal(1, 1);\n");
  run(source, "add", "."); run(source, "commit", "-qm", "main assertion");
  const base = run(source, "rev-parse", "HEAD").trim();
  run(source, "merge", "--no-ff", "-qm", "merge", "change");
  run(root, "clone", "-q", "--depth=1", pathToFileURL(source).href, shallow);
  const event = { pull_request: { base: { sha: initial }, head: { sha: head }, merge_commit_sha: run(source, "rev-parse", "HEAD").trim() } };
  const calls = [];
  const files = testDiff(event, (args) => { calls.push(args); return run(shallow, ...args); }, event.pull_request.merge_commit_sha);
  assert.ok(calls.some((args) => args[0] === "fetch"), "missing parent is obtained from the existing origin");
  assert.deepEqual(files.map((file) => file.filename), [tricky, "fixtures/資料 expected.txt"].sort());
  const cwd = process.cwd();
  try {
    process.chdir(source);
    const literalFile = testDiff(event, undefined, event.pull_request.merge_commit_sha).find((file) => file.filename === tricky);
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


test("runner option forms and todo declarations need an explanation", () => {
  for (const line of ["+test.todo('case');", "+test('case', { skip: true }, () => {});", "+test('case', { only: true }, () => {});"]) assert.equal(check("scripts/case.test.mjs", line).length, 1, line);
});

test("inline Rust test assertions are guarded without treating runtime assertions as tests", () => {
  const before = "fn runtime() {\n  assert!(ready);\n}\n#[cfg(test)]\nmod tests {\n #[test]\n fn case() {\n  assert_eq!(actual, expected);\n }\n}\n";
  assert.deepEqual(testTamper([{ filename: "src/lib.rs", patch: "@@ -2 +2 @@\n-  assert!(ready);\n+  run();", before, after: before }]), []);
  assert.equal(testTamper([{ filename: "src/lib.rs", patch: "@@ -8 +8 @@\n-  assert_eq!(actual, expected);\n+  run();", before, after: before }]).length, 1);
});


test("nested Rust comments cannot hide an assertion from its test region", () => {
  const before = "#[test]\nfn case() {\n /* outer /* inner */ } */\n assert_eq!(1, 2);\n}\n";
  const file = { filename: "src/lib.rs", before, after: before.replace("assert_eq!(1, 2)", "()"), patch: "@@ -4 +4 @@\n- assert_eq!(1, 2);\n+ ();" };
  assert.equal(testTamper([file]).length, 1);
});
