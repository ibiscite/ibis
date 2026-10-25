import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
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
  assert.equal(check("`資料/missing file.mjs`").length, 1);
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


test("Markdown delimiter variants, quoted commands and continued commands remain checked", () => {
  assert.equal(check("``scripts/missing.mjs``").length, 1);
  assert.equal(check("[missing][target]\n[target]: <missing.md>").length, 1);
  assert.equal(check("cargo test \\\n  -p missing").length, 1);
  assert.equal(check("cargo test -p 'missing'").length, 1);
  assert.equal(check('node "scripts/missing.mjs"').length, 1);
  assert.deepEqual(docProblems("docs/guide.md", "`ui/*.tsx` `scripts/check?.mjs`", [...files, "scripts/check1.mjs"], []), []);
});


test("hostile glob matching is bounded", () => {
  const code = `import {docProblems} from ${JSON.stringify(new URL("./check-docs-drift.mjs", import.meta.url).href)}; const result = docProblems("docs/guide.md", "\u0060scripts/" + "*".repeat(40) + "missing.mjs\u0060", ["scripts/" + "a".repeat(40) + ".mjs"], []); if(result.length !== 1) process.exitCode = 1;`;
  const result = spawnSync(process.execPath, ["--input-type=module", "-e", code], { timeout: 1500, killSignal: "SIGKILL", encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr || result.error?.message);
});


test("hostile Markdown delimiter runs are bounded", () => {
  const code = `import {docProblems} from ${JSON.stringify(new URL("./check-docs-drift.mjs", import.meta.url).href)}; for (const point of [96, 91, 10]) docProblems("docs/guide.md", String.fromCharCode(point).repeat(100000), [], []);`;
  const result = spawnSync(process.execPath, ["--input-type=module", "-e", code], { timeout: 1500, killSignal: "SIGKILL", encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr || result.error?.message);
});


test("invalid baseline counts cannot silence broken references", () => {
  const problems = check("[bad](missing.md)");
  for (const baseline of [null, [], { [problems[0]]: "not-a-number" }, { [problems[0]]: 0 }, { bogus: 1 }, { [problems[0]]: Number.MAX_SAFE_INTEGER + 1 }]) assert.match(baselineErrors(problems, baseline)[0], /baseline.*positive.*integer/i);
});


test("CRLF fences close before subsequent inline references", () => {
  const lines = ["```sh", "node scripts/check.mjs", "```", "`scripts/missing.mjs`"];
  assert.equal(check(lines.join("\r\n")).length, 1);
  assert.deepEqual(check(lines.join("\r\n")), check(lines.join("\n")));
});
