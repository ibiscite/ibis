import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readdirSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { check } from "./check.mjs";

function laptop(t, { missing, powershellStatus = 0 } = {}) {
  const calls = [], errors = [], python = process.env.PYTHON || "python";
  t.mock.method(console, "log", () => {});
  t.mock.method(console, "error", (message) => errors.push(message));
  return { calls, errors, run: (suite = "windows") => check(suite, [], {
    platform: "win32",
    spawn(command, args) {
      calls.push([command, ...args]);
      // The SSH launcher's PATH has only these runtimes: no Bash or PowerShell 7.
      if (command === missing || !["node", python, "powershell.exe"].includes(command)) {
        return { status: null, error: Object.assign(new Error(`spawn ${command} ENOENT`), { code: "ENOENT" }) };
      }
      return { status: command === "powershell.exe" && args.includes("-File") ? powershellStatus : 0 };
    },
  }) };
}

test("Windows suites need neither pwsh nor Bash and select their declared Node coverage", (t) => {
  const f = laptop(t);
  assert.equal(f.run(), 0, f.errors.join("\n"));
  const powershell = f.calls.filter(([command]) => command === "powershell.exe");
  assert.match(powershell[0].at(-1), /PSVersionTable.PSVersion.Major -ne 5.*Minor -ne 1/);
  assert.deepEqual(powershell.slice(1).map((call) => call.at(-1)),
    readdirSync(new URL(".", import.meta.url)).filter((file) => file.endsWith(".tests.ps1")).sort().map((file) => `scripts/${file}`));
  const node = f.calls.find(([command]) => command === "node");
  assert.deepEqual(node, ["node", "--test", "--test-concurrency=2",
    "scripts/check.test.mjs", "scripts/preview-packages.test.mjs", "scripts/windows-command-path.test.mjs"]);
  // Run the selected Node files too, with Windows branches and an empty PATH.
  // This file is already running; spawning it again would recurse.
  const result = spawnSync(process.execPath, [
    "--import", 'data:text/javascript,Object.defineProperty(process,"platform",{value:"win32"})',
    ...node.slice(1).filter((file) => file !== "scripts/check.test.mjs"),
  ], { cwd: fileURLToPath(new URL("..", import.meta.url)), encoding: "utf8", timeout: 15000,
    env: { ...Object.fromEntries(Object.entries(process.env).filter(([key]) => key.toLowerCase() !== "path")), PATH: "" } });
  assert.equal(result.status, 0, result.error?.message ?? result.stdout + result.stderr);
  assert.ok(f.calls.some((call) => call.includes("scripts/test_native_evidence.py")));
  assert.equal(f.calls.filter((call) => call.includes("--self-test")).length, 2);
  f.calls.length = 0;
  assert.equal(f.run("scripts"), 0);
  assert.deepEqual(f.calls[0], node);
  assert.ok(f.calls.every(([command]) => command !== "powershell.exe"));
});

test("missing Windows PowerShell fails naming powershell.exe before running any tests", (t) => {
  const f = laptop(t, { missing: "powershell.exe" });
  assert.equal(f.run(), 1);
  assert.match(f.errors.join("\n"), /Required runtime powershell.exe:.*ENOENT/);
  assert.equal(f.calls.length, 1);
});

test("PowerShell 7 is required only when its CI suite is selected", (t) => {
  const f = laptop(t);
  assert.equal(f.run("powershell7"), 1);
  assert.match(f.errors.join("\n"), /Required runtime pwsh:.*ENOENT/);
  assert.equal(f.calls.length, 1);
});

test("PowerShell test failures stop the suite and preserve the failure status", (t) => {
  const f = laptop(t, { powershellStatus: 73 });
  assert.equal(f.run(), 73);
  assert.ok(f.calls.at(-1).includes("-File"));
  assert.ok(f.calls.every(([command]) => command === "powershell.exe"));
});
