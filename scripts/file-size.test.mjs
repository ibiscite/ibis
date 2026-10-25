import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { sizeErrors, lineCount, sourceFile } from "./check-file-size.mjs";
const text = (n) => "synthetic\n".repeat(n);

test("caps new files at 600 lines and existing large files at their baseline", () => {
  assert.deepEqual(sizeErrors({ "src/a.ts": text(600) }, {}, {}), []);
  assert.match(sizeErrors({ "src/a.ts": text(601) }, {}, {})[0], /split.*600/i);
  assert.deepEqual(sizeErrors({ "src/a.ts": text(700) }, { "src/a.ts": 700 }, { "src/a.ts": 700 }), []);
  assert.match(sizeErrors({ "src/a.ts": text(701) }, { "src/a.ts": 700 }, { "src/a.ts": 700 })[0], /split|grow/i);
});

test("shrinking lowers the baseline and inflation cannot buy more lines", () => {
  const old = { "src/a.ts": 700 };
  assert.match(sizeErrors({ "src/a.ts": text(650) }, old, old)[0], /baseline.*650/i);
  assert.deepEqual(sizeErrors({ "src/a.ts": text(650) }, { "src/a.ts": 650 }, old), []);
  assert.ok(sizeErrors({ "src/a.ts": text(701) }, { "src/a.ts": 701 }, old).length);
  assert.ok(sizeErrors({ "src/new.ts": text(700) }, { "src/new.ts": 700 }, old).length);
  assert.ok(sizeErrors({}, old, old).length, "deleted or renamed files must lose their old baseline");
  assert.deepEqual(sizeErrors({ "src/a.ts": text(599) }, {}, old), []);
});

test("line counts handle empty, huge, CRLF, Unicode and trailing newlines", () => {
  assert.equal(lineCount(""), 0);
  assert.equal(lineCount("one\r\ntwo\r\n"), 2);
  assert.equal(lineCount("one\ntwo"), 2);
  assert.equal(lineCount(text(100000)), 100000);
  assert.deepEqual(sizeErrors({ "src\\資料.ts": text(700) }, { "src/資料.ts": 700 }, { "src/資料.ts": 700 }), []);
  assert.ok(sizeErrors({ "src/renamed.ts": text(700) }, { "src/renamed.ts": 700 }, { "src/old.ts": 700 }).length);
});

test("only source and test files count, with explicit generated/vendor exclusions", () => {
  for (const name of ["apps/desktop/src/bindings.ts", "vendor/parser.js", "apps/desktop/src/vendor/parser.js", "target/build/output.rs", "docs/design.md"]) assert.equal(sourceFile(name), false, name);
  for (const name of ["crates/core/tests/a.rs", "scripts/a.test.mjs", "apps/desktop/src/ui/a.tsx", "website/index.html"]) assert.equal(sourceFile(name), true, name);
});


test("CLI enforces and writes reductions against the committed base", (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-size-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, "scripts")); mkdirSync(join(root, "src"));
  const env = { ...process.env };
  delete env.GITHUB_EVENT_PATH;
  for (const key of Object.keys(env)) if (key.startsWith("GIT_")) delete env[key];
  const git = (...args) => {
    const result = spawnSync("git", ["-c", "core.hooksPath=", "-c", "commit.gpgsign=false", "-c", "user.name=Synthetic", "-c", "user.email=synthetic@example.invalid", ...args], { cwd: root, env, encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr); return result.stdout.trim();
  };
  const baseline = join(root, "scripts/file-size-baseline.json"), source = join(root, "src/資料.ts");
  const record = (n) => writeFileSync(baseline, JSON.stringify({ "src/資料.ts": n }));
  const run = (...args) => spawnSync(process.execPath, [fileURLToPath(new URL("./check-file-size.mjs", import.meta.url)), ...args], { cwd: root, env, encoding: "utf8" });
  git("init", "-q", "-b", "main"); record(700); writeFileSync(source, text(700));
  git("add", "."); git("commit", "-qm", "synthetic base"); git("update-ref", "refs/remotes/origin/main", git("rev-parse", "HEAD"));
  const oldSha = git("rev-parse", "HEAD");
  assert.equal(run().status, 0);
  writeFileSync(source, text(650));
  assert.match(run().stderr, /lower the baseline to 650/);
  assert.equal(run("--write-baseline").status, 0);
  assert.equal(JSON.parse(readFileSync(baseline))["src/資料.ts"], 650);
  git("add", "."); git("commit", "-qm", "lower baseline");
  git("switch", "-qc", "change"); git("remote", "add", "origin", root);
  const eventFile = join(root, "event.json");
  writeFileSync(eventFile, JSON.stringify({ pull_request: { base: { sha: oldSha } } }));
  writeFileSync(source, text(680)); record(680);
  const stale = spawnSync(process.execPath, [fileURLToPath(new URL("./check-file-size.mjs", import.meta.url))], { cwd: root, env: { ...env, GITHUB_EVENT_PATH: eventFile }, encoding: "utf8" });
  assert.equal(stale.status, 1, "stale PR base must not resurrect main's old limit");
  writeFileSync(source, text(701)); record(701);
  assert.match(run().stderr, /exceptions cannot be added or grow/);
  assert.equal(run("--write-baseline").status, 1, "write mode also refuses inflation");
});
