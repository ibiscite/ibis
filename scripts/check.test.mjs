import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const checker = fileURLToPath(new URL("./check.mjs", import.meta.url));
const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === "path") ?? "PATH";

test("brand-only checks rebuild without starting Rust or changing source files", { skip: process.platform === "win32" }, (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-brand-check-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, "scripts")); mkdirSync(join(root, "bin"));
  writeFileSync(join(root, "scripts/check.mjs"), readFileSync(checker));
  writeFileSync(join(root, "bin/python3"), '#!/bin/sh\nprintf "%s\\n" "$*"\nexit 0\n', { mode: 0o755 });
  const result = spawnSync(process.execPath, [join(root, "scripts/check.mjs"), "--brand-assets"], {
    cwd: root, env: { ...process.env, [pathKey]: `${join(root, "bin")}${delimiter}${process.env[pathKey] ?? ""}` }, encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /brand\/build\.py --check/);
  assert.doesNotMatch(result.stdout, /== Format:|== Desktop:|== Workflow/);
});

test("script-only discovery covers Node and Python names without starting build tools", (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-script-discovery-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const scripts = join(root, "scripts"), bin = join(root, "bin");
  mkdirSync(scripts); mkdirSync(bin);
  writeFileSync(join(scripts, "check.mjs"), readFileSync(checker));
  for (const name of ["new.test.mjs", "new.test.cjs"]) {
    writeFileSync(join(scripts, name), `console.log(${JSON.stringify(name)});`);
  }
  for (const name of ["test_new.py", "new.test.py", "unrelated.py"]) writeFileSync(join(scripts, name), "");
  const python = process.platform === "win32" ? "python.cmd" : "python3";
  const wrapper = process.platform === "win32"
    ? '@echo off\r\nif "%~1"=="--version" exit /b 0\r\necho PYTHON_TEST %*\r\nexit /b 0\r\n'
    : '#!/bin/sh\nif [ "$1" = "--version" ]; then exit 0; fi\nprintf "PYTHON_TEST %s %s\\n" "$1" "$2"\n';
  writeFileSync(join(bin, python), wrapper, { mode: 0o755 });
  const env = { ...process.env, [pathKey]: `${bin}${delimiter}${process.env[pathKey] ?? ""}` };
  delete env.NODE_TEST_CONTEXT; // The synthetic child runs its own Node test runner.
  const run = (flag) => spawnSync(process.execPath, [join(scripts, "check.mjs"), flag], { cwd: root, env, encoding: "utf8", timeout: 15_000 });
  for (const flag of ["--script-tests", "--platform-script-tests"]) {
    const result = run(flag);
    assert.equal(result.status, 0, result.stdout + result.stderr);
    for (const name of ["new.test.mjs", "new.test.cjs", "test_new.py", "new.test.py"]) assert.match(result.stdout, new RegExp(name));
    assert.doesNotMatch(result.stdout, /unrelated\.py|== Format:|== Lint:|== Desktop:|== Workflow/);
  }
  writeFileSync(join(scripts, "new.test.cjs"), 'throw Error("synthetic script failure");');
  assert.equal(run("--platform-script-tests").status, 1);
  writeFileSync(join(scripts, "new.test.cjs"), "");
  writeFileSync(join(bin, python), process.platform === "win32" ? wrapper.replace(/exit \/b 0\r\n$/, 'exit /b 7\r\n') : wrapper + 'exit 7\n', { mode: 0o755 });
  assert.equal(run("--platform-script-tests").status, 1);
  writeFileSync(join(bin, python), process.platform === "win32" ? '@exit /b 7\r\n' : '#!/bin/sh\nexit 7\n', { mode: 0o755 });
  assert.equal(run("--platform-script-tests").status, 2, "unavailable Python must report incomplete");
});

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
  return (args) => spawnSync(process.execPath, [checker, ...args], { cwd: root, env, encoding: "utf8", timeout: 15_000 });
}

test("unknown arguments are refused before checks", (t) => {
  const run = fixture(t);
  for (const args of [["--unknown"], ["--script-tests", "extra"], ["--expect-commit", "a".repeat(40)]]) {
    const result = run(args);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Usage: node scripts\/check\.mjs/);
    assert.doesNotMatch(result.stdout, /CHECK_STEP_STARTED/);
  }
});

test("ordinary checks start with the first step", (t) => {
  const result = fixture(t)([]);
  assert.equal(result.status, 1); // The deliberately failed first step.
  assert.match(result.stdout, /== Workflow expressions:/);
  assert.match(result.stdout, /CHECK_STEP_STARTED/);
  assert.doesNotMatch(result.stdout, /== Format:|== Desktop:/);
});
