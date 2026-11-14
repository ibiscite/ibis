#!/usr/bin/env node
// Blocking suites for land.sh and CI. Dependencies are prepared by the caller.
// Focus: node scripts/check.mjs rust -p ibis-engine
//        node scripts/check.mjs test -p ibis-engine TEST_FILTER
//        node scripts/check.mjs vitest FILE
import { spawnSync } from "node:child_process";
import { readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

process.chdir(fileURLToPath(new URL("..", import.meta.url)));
const files = readdirSync("scripts").sort();
const python = process.env.PYTHON || (process.platform === "win32" ? "python" : "python3");
function run(command, ...args) {
  console.log([command, ...args].join(" "));
  const result = spawnSync(command, args, { stdio: "inherit", shell: process.platform === "win32" && command.endsWith(".cmd") });
  if (result.error) console.error(result.error.message);
  if (result.status !== 0) process.exit(result.status || 1);
}
const node = (...args) => run("node", ...args);
const pnpm = (...args) => run(process.platform === "win32" ? "pnpm.cmd" : "pnpm", "-C", "apps/desktop", ...args);
const suites = {
  rust: (...args) => {
    const targets = args.length ? args : ["--workspace"];
    run("cargo", "clippy", ...targets, "--all-targets", "--locked", "--", "-D", "warnings", "-D", "clippy::unused_result_ok");
    suites.test(...targets);
  },
  test: (...args) => run("cargo", "test", ...args, "--locked"),
  vitest: (...args) => pnpm("exec", "vitest", "run", ...args, `--maxWorkers=${args.length ? 1 : 2}`),
  web: () => {
    pnpm("exec", "tsc", "--noEmit");
    pnpm("lint");
    suites.vitest();
  },
  "web-build": () => {
    pnpm("exec", "vite", "build");
    node("scripts/picker-budget.mjs", "apps/desktop/dist");
  },
  scripts: () => {
    node("--test", "--test-concurrency=2", ...files.filter((f) => f.endsWith(".test.mjs")).map((f) => `scripts/${f}`));
    for (const file of files.filter((f) => /^test_.*\.py$|\.test\.py$/.test(f))) run(python, "-B", `scripts/${file}`);
    for (const file of ["smoke-native.py", "update-e2e.py"]) run(python, "-B", `scripts/${file}`, "--self-test");
  },
  generated: () => node("scripts/gen.mjs", "--check"),
  format: () => run("cargo", "fmt", "--all", "--check"),
  guards: () => {
    for (const file of files.filter((f) => /^check-.*\.mjs$/.test(f))) node(`scripts/${file}`);
  },
  licenses: () => run("cargo", "deny", "--locked", "check", "licenses", "bans", "sources"),
  windows: () => {
    const powershell = (...args) => run("powershell.exe", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", ...args);
    powershell("-Command", "if ($PSVersionTable.PSVersion.Major -ne 5 -or $PSVersionTable.PSVersion.Minor -ne 1) { throw 'Windows PowerShell 5.1 is required' }");
    for (const file of files.filter((f) => f.endsWith(".tests.ps1"))) powershell("-File", `scripts/${file}`);
    run("pwsh", "-NoProfile", "-NonInteractive", "-Command", "exit 0");
    suites.scripts();
  },
  all: () => {
    // Cheapest first, so a slip fails in seconds.
    for (const suite of ["format", "guards", "licenses", "generated", "rust", "web", "web-build", "scripts"]) suites[suite]();
  },
};
const [suite = "all", ...args] = process.argv.slice(2);
if (!Object.hasOwn(suites, suite)) {
  console.error(`Usage: node scripts/check.mjs [${Object.keys(suites).join("|")}] [focused arguments]`);
  process.exit(2);
}
suites[suite](...args);
