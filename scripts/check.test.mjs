import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const checker = fileURLToPath(new URL("./check.mjs", import.meta.url));
const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === "path") ?? "PATH";

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), "ibis-check-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const bin = join(root, "tools");
  mkdirSync(bin);
  // Stop at the first normal step; no Rust, package install or app test runs.
  writeFileSync(join(bin, process.platform === "win32" ? "actionlint.cmd" : "actionlint"),
    process.platform === "win32"
      ? '@echo off\r\nif "%~1"=="-version" exit /b 0\r\necho CHECK_STEP_STARTED\r\nexit /b 7\r\n'
      : '#!/bin/sh\nif [ "$1" = "-version" ]; then exit 0; fi\nprintf "CHECK_STEP_STARTED\\n"\nexit 7\n',
    { mode: 0o755 });
  const env = { ...process.env, [pathKey]: `${bin}${delimiter}${process.env[pathKey] ?? ""}` };
  // A Git hook's inherited selectors must never redirect synthetic setup.
  for (const key of Object.keys(env)) if (key.toUpperCase().startsWith("GIT_")) delete env[key];
  function git(cwd, ...args) {
    const result = spawnSync("git", ["-c", `core.hooksPath=${join(root, "no-hooks")}`, ...args], { cwd, env, encoding: "utf8" });
    assert.equal(result.status, 0, "synthetic Git setup must succeed");
    return result.stdout.trim();
  }
  function repository(name) {
    const cwd = join(root, name);
    mkdirSync(cwd);
    git(cwd, "init", "-q", "--object-format=sha1");
    writeFileSync(join(cwd, "tracked.txt"), `${name}\n`);
    writeFileSync(join(cwd, ".gitignore"), "ignored/\n");
    git(cwd, "add", ".");
    git(cwd, "-c", "user.name=Synthetic", "-c", "user.email=synthetic@example.invalid", "-c", "commit.gpgsign=false", "commit", "-qm", "Synthetic checkout");
    return { cwd, sha: git(cwd, "rev-parse", "HEAD") };
  }
  const source = repository("source repo");
  return {
    ...source, root, bin, env, git, repository,
    run: (args, cwd = source.cwd, runEnv = env) => spawnSync(process.execPath, [checker, ...args], {
      cwd, env: runEnv, encoding: "utf8", timeout: 15_000,
    }),
  };
}

function refused(result, message) {
  assert.equal(result.status, 1);
  assert.match(result.stderr, message);
  assert.equal(result.stdout, "");
  assert.equal(result.stderr.trim().split("\n").length, 1);
  assert.doesNotMatch(result.stderr, /fatal:|private-資料|ibis-check-/);
}

function proceeded(result) {
  assert.equal(result.status, 1); // The deliberately failed first normal step.
  assert.match(result.stdout, /== Workflow expressions:/);
  assert.match(result.stdout, /CHECK_STEP_STARTED/);
  assert.doesNotMatch(result.stdout, /== Format:|== Desktop:/);
}

test("guard accepts the exact clean commit and ignored generated files", (t) => {
  const f = fixture(t);
  mkdirSync(join(f.cwd, "ignored"));
  writeFileSync(join(f.cwd, "ignored", "generated.txt"), "synthetic output");
  const result = f.run(["--expect-commit", f.sha.toUpperCase()]);
  proceeded(result);
  assert.match(result.stdout, new RegExp(`Verified source commit ${f.sha}`));
});

test("another repository's working directory cannot verify the requested commit", (t) => {
  const f = fixture(t);
  const other = f.repository("other repo");
  assert.notEqual(other.sha, f.sha);
  refused(f.run(["--expect-commit", f.sha], other.cwd), /expected commit does not match/);
});

test("guard requires the repository root even when a subdirectory has the same HEAD", (t) => {
  const f = fixture(t);
  const nested = join(f.cwd, "nested");
  mkdirSync(nested);
  refused(f.run(["--expect-commit", f.sha], nested), /repository root/);
});

test("inherited Git selectors cannot verify another checkout or index", (t) => {
  const f = fixture(t);
  const other = f.repository("other repo");
  for (const overrides of [
    { GIT_DIR: join(f.cwd, ".git"), GIT_WORK_TREE: f.cwd },
    { GIT_COMMON_DIR: join(f.cwd, ".git") },
    { GIT_INDEX_FILE: join(f.cwd, ".git", "index") },
  ]) {
    refused(f.run(["--expect-commit", f.sha], other.cwd, { ...f.env, ...overrides }), /Git source overrides are not supported/);
  }
});

for (const kind of ["tracked", "staged", "untracked"]) {
  test(`guard refuses ${kind} source changes without exposing filenames`, (t) => {
    const f = fixture(t);
    const file = kind === "untracked" ? "private-資料.txt" : "tracked.txt";
    writeFileSync(join(f.cwd, file), "changed synthetic source\n");
    if (kind === "staged") f.git(f.cwd, "add", file);
    refused(f.run(["--expect-commit", f.sha]), /source checkout has uncommitted changes/);
  });
}

for (const flag of ["--assume-unchanged", "--skip-worktree"]) {
  test(`guard refuses tracked changes hidden by ${flag}`, (t) => {
    const f = fixture(t);
    f.git(f.cwd, "update-index", flag, "tracked.txt");
    writeFileSync(join(f.cwd, "tracked.txt"), "hidden synthetic change\n");
    assert.equal(f.git(f.cwd, "status", "--porcelain"), "");
    refused(f.run(["--expect-commit", f.sha]), /index flags can hide tracked source changes/);
  });
}

test("guard refuses a missing repository or missing Git without raw diagnostics", (t) => {
  const f = fixture(t);
  refused(f.run(["--expect-commit", f.sha], f.root), /Git could not verify/);
  refused(f.run(["--expect-commit", f.sha], f.cwd, { ...f.env, [pathKey]: f.bin }), /Git could not verify/);
});

test("unknown, missing, abbreviated and extra arguments are refused before checks", (t) => {
  const f = fixture(t);
  for (const args of [
    ["--unknown"], ["--expect-commit"], ["--expect-commit", f.sha.slice(0, 12)],
    ["--expect-commit", "x".repeat(40)], ["--expect-commit", "a".repeat(41)],
    ["--expect-commit", `${f.sha}\n`],
    ["--expect-commit", f.sha, "extra"], [`--expect-commit=${f.sha}`],
    ["--expect-commit", f.sha, "--expect-commit", f.sha],
  ]) refused(f.run(args), /Usage: node scripts\/check\.mjs/);
});

test("ordinary checks still start without a guard, including a dirty checkout", (t) => {
  const f = fixture(t);
  writeFileSync(join(f.cwd, "tracked.txt"), "work in progress\n");
  const result = f.run([]);
  proceeded(result);
  assert.doesNotMatch(result.stdout, /Verified source commit/);
});
