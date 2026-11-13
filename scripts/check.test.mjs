import assert from "node:assert/strict";
import { test } from "node:test";
import { changedFiles, checkPlan, runChecks } from "./check.mjs";

const files = (...names) => names.map((filename) => ({ filename, status: "modified" }));
const inventory = ["land.test.mjs", "check.test.mjs", "website.test.mjs", "website-copy.test.mjs", "other.test.mjs"];

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

test("interface and script changes select their supported outcomes without native suites", () => {
  const front = checkPlan(files("apps/desktop/src/App.tsx"), { inventory });
  for (const command of ["typecheck", "lint", "test"]) assert.ok(front.some(({ args }) => args.includes(command)), command);
  assert.ok(!front.some(({ command, args }) => command === "cargo" && ["test", "clippy"].includes(args[0])));
  const scripts = checkPlan(files("scripts/land.sh", "scripts/check.test.mjs"), { inventory });
  assert.deepEqual(scripts.find(({ name }) => name === "Script tests").args, ["--test", ...inventory.slice().sort().map((name) => `scripts/${name}`)]);
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
