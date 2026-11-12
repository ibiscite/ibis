import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { changedFiles, checkPlan, runChecks } from "./check.mjs";

const files = (...names) => names.map((filename) => ({ filename, status: "modified" }));
const inventory = ["land.test.mjs", "check.test.mjs", "website.test.mjs", "website-copy.test.mjs", "other.test.mjs"];
const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === "path") ?? "PATH";
const copyChecker = (scripts) => {
  for (const name of ["check.mjs", "ci-paths.mjs"]) {
    writeFileSync(join(scripts, name), readFileSync(new URL(`./${name}`, import.meta.url)));
  }
};

test("the local plan checks native targets, generated files and related website tests", () => {
  const steps = checkPlan(files("crates/ibis-core/src/lib.rs", "website/index.html"), { inventory });
  assert.ok(steps.some(({ command, args }) => command === "cargo" && args[0] === "fmt"));
  for (const step of steps.filter(({ args }) => args[0] === "clippy")) {
    assert.ok(step.args.includes("--all-targets"));
    assert.deepEqual(step.args.slice(-4), ["-D", "warnings", "-D", "clippy::unused_result_ok"]);
  }
  assert.ok(steps.some(({ args }) => args.join(" ") === "scripts/gen.mjs --check"));
  const tests = checkPlan(files("website/index.html"), { inventory }).find(({ name }) => name === "Script tests").args;
  assert.ok(tests.includes("scripts/website.test.mjs") && tests.includes("scripts/website-copy.test.mjs"));
  assert.ok(!tests.includes("scripts/other.test.mjs"));
  assert.ok(!steps.some(({ name }) => /Minimum Rust|WebKit|install/.test(name)), "heavy checks and installs belong to CI");
});

test("script-only checks discover Node and Python tests and run suites without contention", (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-script-discovery-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const scripts = join(root, "scripts"), bin = join(root, "bin");
  mkdirSync(scripts); mkdirSync(bin);
  copyChecker(scripts);
  for (const name of ["new.test.mjs", "new.test.cjs"]) {
    const imports = name.endsWith(".cjs")
      ? 'const { mkdirSync, rmdirSync } = require("node:fs"); const { test } = require("node:test");'
      : 'import { mkdirSync, rmdirSync } from "node:fs"; import { test } from "node:test";';
    writeFileSync(join(scripts, name), `${imports}
test("a suite owns its subprocess budget", async () => {
  mkdirSync("suite-active");
  try {
    await new Promise((resolve) => setTimeout(resolve, 250));
    console.log(${JSON.stringify(name)});
  } finally { rmdirSync("suite-active"); }
});`);
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

test("interface and script changes select their supported outcomes without native suites", () => {
  const front = checkPlan(files("apps/desktop/src/App.tsx"), { inventory });
  for (const command of ["typecheck", "lint", "test"]) assert.ok(front.some(({ args }) => args.includes(command)), command);
  assert.ok(!front.some(({ command, args }) => command === "cargo" && ["test", "clippy"].includes(args[0])));
  const scripts = checkPlan(files("scripts/land.sh", "scripts/check.test.mjs"), { inventory });
  assert.deepEqual(scripts.find(({ name }) => name === "Script tests").args, ["--test", "--test-concurrency=1", ...inventory.slice().sort().map((name) => `scripts/${name}`)]);
});

test("changed files include committed, dirty and untracked files; unreadable input selects everything", () => {
  const answers = (tracked, untracked = "") => (args) => ({ "merge-base": "abc\n", diff: tracked, "ls-files": untracked })[args[0]];
  assert.deepEqual(changedFiles(answers("M\0docs/a.md\0R100\0old.rs\0new.rs\0", "new.txt\0")), [
    { filename: "new.txt", status: "added" },
    { filename: "docs/a.md", previous_filename: null, status: "modified" },
    { filename: "new.rs", previous_filename: "old.rs", status: "renamed" },
  ]);
  assert.equal(changedFiles(() => null), null);
  assert.ok(checkPlan(null, { inventory }).some(({ name, args }) => name === "Test" && args.includes("--workspace")));
});

test("preflight absence reports unavailable without executing the check; a later failure stays failed", () => {
  const step = { name: "Lint", command: "cargo", args: ["clippy"], probes: [["cargo", ["clippy", "--version"], "clippy is not installed"]] };
  const calls = [], reports = [];
  const unavailable = runChecks([step], (command, args) => { calls.push([command, args]); return { status: 1 }; }, (line) => reports.push(line));
  assert.equal(unavailable, 2);
  assert.equal(calls.length, 1);
  assert.match(reports.join("\n"), /unavailable.*clippy is not installed/);
  let count = 0;
  assert.equal(runChecks([step], () => ({ status: ++count === 1 ? 0 : 2 }), (line) => reports.push(line)), 1);
  assert.match(reports.at(-1), /failed/);
  assert.equal(count, 2, "a failed command is never re-probed or reinterpreted");
  assert.equal(runChecks([step], () => ({ signal: "SIGTERM" }), (line) => reports.push(line)), 75);
  assert.match(reports.at(-1), /infrastructure/);
});

test("each result is explicit and infrastructure never counts as a pass", () => {
  const step = { name: "Test", command: "node", args: [], probes: [] };
  for (const [result, code, word] of [[{ status: 0 }, 0, "passed"], [{ status: 1 }, 1, "failed"], [{ status: 75 }, 75, "infrastructure"], [{ signal: "SIGTERM" }, 75, "infrastructure"]]) {
    const reports = [];
    assert.equal(runChecks([step], () => result, (line) => reports.push(line)), code);
    assert.match(reports.at(-1), new RegExp(word));
  }
});

test("missing brand fonts report unavailable before rendering starts", () => {
  const reports = [], calls = [];
  const status = runChecks(checkPlan([], { brandOnly: true, inventory }), (command, args) => {
    calls.push([command, args]);
    return { status: args[0] === "-c" ? 1 : 0 };
  }, (line) => reports.push(line));
  assert.equal(status, 2);
  assert.match(reports.join("\n"), /unavailable.*Inter Display/);
  assert.ok(!calls.some(([, args]) => args.includes("brand/build.py")));
});
