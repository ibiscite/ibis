#!/usr/bin/env node
// One command for contributors and CI-equivalent local checks:
//   node scripts/check.mjs
// Runs formatting, lints, tests and the dependency policy. Never edits files.
// Exit codes: 0 all passed, 1 a check failed, 2 incomplete (a tool is missing).
import { spawnSync } from "node:child_process";

const desktop = { cwd: "apps/desktop" };
const steps = [
  ["Format", "cargo", ["fmt", "--all", "--check"]],
  ["Lint", "cargo", ["clippy", "--all-targets", "--locked", "--", "-D", "warnings"]],
  ["Test", "cargo", ["test", "--locked"]],
  ["Dependency policy", "cargo", ["deny", "check"]],
  ["Desktop: install", "pnpm", ["install", "--frozen-lockfile"], desktop],
  ["Desktop: type check", "pnpm", ["typecheck"], desktop],
  ["Desktop: interface tests", "pnpm", ["test"], desktop],
  ["Desktop: lint app", "cargo", ["clippy", "-p", "ibis-desktop", "--all-targets", "--locked", "--", "-D", "warnings"]],
];

const skipped = [];
for (const [name, cmd, args, options = {}] of steps) {
  console.log(`\n== ${name}: ${cmd} ${args.join(" ")}`);
  const result = spawnSync(cmd, args, { stdio: "inherit", shell: process.platform === "win32", ...options });
  const missingTool =
    result.error?.code === "ENOENT" || (args[0] === "deny" && result.status !== 0 && !hasCargoDeny());
  if (missingTool) {
    console.log(`${name} could not run: the tool is not installed.`);
    skipped.push(name);
    continue;
  }
  if (result.status !== 0) {
    console.error(`\n✗ ${name} failed. Fix the problem above, then run this script again.`);
    process.exit(result.status ?? 1);
  }
}
if (skipped.length > 0) {
  console.error(`\n! INCOMPLETE: ${skipped.join(", ")} did not run. Install the missing tools (cargo-deny: cargo install --locked cargo-deny; pnpm: https://pnpm.io/installation).`);
  process.exit(2);
}
console.log("\n✓ All checks passed.");

function hasCargoDeny() {
  const probe = spawnSync("cargo", ["deny", "--version"], { stdio: "ignore", shell: process.platform === "win32" });
  return probe.status === 0;
}
