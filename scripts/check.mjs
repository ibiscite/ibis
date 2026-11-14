#!/usr/bin/env node
// Blocking suites for land.sh and CI. Dependencies are prepared by the caller.
// Focus: node scripts/check.mjs rust -p ibis-engine
//        node scripts/check.mjs test -p ibis-engine TEST_FILTER
//        node scripts/check.mjs vitest FILE
import { spawnSync } from "node:child_process";
import { readdirSync, realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";

export function check(suite = "all", args = [], { platform = process.platform, spawn = spawnSync } = {}) {
  const root = fileURLToPath(new URL("..", import.meta.url));
  const files = readdirSync(new URL(".", import.meta.url)).sort();
  const powershellArgs = ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass"];
  // Commands declare a runtime; only selected commands resolve it for this platform.
  const runtimes = {
    cargo: { default: ["cargo"] },
    node: { default: ["node"] },
    pnpm: { default: ["pnpm", "-C", "apps/desktop"], win32: ["pnpm.cmd", "-C", "apps/desktop"] },
    python: { default: [process.env.PYTHON || "python3", "-B"], win32: [process.env.PYTHON || "python", "-B"] },
    powershell: { win32: ["powershell.exe", ...powershellArgs] },
    powershell7: { default: ["pwsh", ...powershellArgs] },
  };
  // The Windows gate's Node coverage. Other script tests can require POSIX tools.
  const nodeTests = {
    win32: ["check.test.mjs", "preview-packages.test.mjs", "windows-command-path.test.mjs"],
    default: files.filter((f) => f.endsWith(".test.mjs")),
  };
  const suites = {
    rust: (...args) => {
      const targets = args.length ? args : ["--workspace"];
      return [["cargo", "clippy", ...targets, "--all-targets", "--locked", "--", "-D", "warnings", "-D", "clippy::unused_result_ok"], ...suites.test(...targets)];
    },
    test: (...args) => [["cargo", "test", ...args, "--locked"]],
    vitest: (...args) => [["pnpm", "exec", "vitest", "run", ...args, `--maxWorkers=${args.length ? 1 : 2}`]],
    lint: () => [["pnpm", "exec", "tsc", "--noEmit"], ["pnpm", "lint"]],
    web: () => [...suites.lint(), ...suites.vitest()],
    "web-build": () => [["pnpm", "exec", "vite", "build"], ["node", "scripts/picker-budget.mjs", "apps/desktop/dist"]],
    scripts: () => [
      ["node", "--test", "--test-concurrency=2", ...(nodeTests[platform] ?? nodeTests.default).map((f) => `scripts/${f}`)],
      ...files.filter((f) => /^test_.*\.py$|\.test\.py$/.test(f)).map((f) => ["python", `scripts/${f}`]),
      ...["smoke-native.py", "update-e2e.py"].map((f) => ["python", `scripts/${f}`, "--self-test"]),
    ],
    generated: () => [["node", "scripts/gen.mjs", "--check"]],
    format: () => [["cargo", "fmt", "--all", "--check"]],
    guards: () => files.filter((f) => /^check-.*\.mjs$/.test(f)).map((f) => ["node", `scripts/${f}`]),
    licenses: () => [["cargo", "deny", "--locked", "check", "licenses", "bans", "sources"]],
    powershell7: () => files.filter((f) => f.endsWith(".tests.ps1")).map((f) => ["powershell7", "-File", `scripts/${f}`]),
    // cfg(windows) code compiles only here.
    "windows-rust": () => [["cargo", "check", "-p", "ibis-desktop", "--all-targets", "--locked"]],
    windows: () => [
      ["powershell", "-Command", "if ($PSVersionTable.PSVersion.Major -ne 5 -or $PSVersionTable.PSVersion.Minor -ne 1) { throw 'Windows PowerShell 5.1 is required' }"],
      ...files.filter((f) => f.endsWith(".tests.ps1")).map((f) => ["powershell", "-File", `scripts/${f}`]),
      ...suites.scripts(),
    ],
    all: () =>
      // Cheapest first, so a slip fails in seconds.
      ["format", "lint", "guards", "licenses", "generated", "rust", "vitest", "web-build", "scripts"].flatMap((suite) => suites[suite]()),
  };
  if (!Object.hasOwn(suites, suite)) {
    console.error(`Usage: node scripts/check.mjs [${Object.keys(suites).join("|")}] [focused arguments]`);
    return 2;
  }
  for (const [runtime, ...commandArgs] of suites[suite](...args)) {
    const command = runtimes[runtime][platform] ?? runtimes[runtime].default;
    if (!command) {
      console.error(`Runtime ${runtime} is not supported on ${platform}`);
      return 1;
    }
    const [executable, ...prefix] = command;
    console.log([...command, ...commandArgs].join(" "));
    const result = spawn(executable, [...prefix, ...commandArgs], { cwd: root, stdio: "inherit", shell: platform === "win32" && executable.endsWith(".cmd") });
    if (result.error) console.error(`Required runtime ${executable}: ${result.error.message}`);
    if (result.status !== 0) return result.status || 1;
  }
  return 0;
}

// Compare real paths: a symlinked invocation (macOS /tmp, /var) must still run the checks.
if (process.argv[1] && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url))) {
  const [suite, ...args] = process.argv.slice(2);
  process.exitCode = check(suite, args);
}
