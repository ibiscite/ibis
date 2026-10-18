#!/usr/bin/env node
// One command for contributors and CI-equivalent local checks:
//   node scripts/check.mjs
// Runs formatting, lints, tests and the dependency policy. Never edits files.
// Exit codes: 0 all passed, 1 a check failed, 2 incomplete (a tool is missing).
import { spawnSync } from "node:child_process";

const desktop = { cwd: "apps/desktop" };
const steps = [
  ["Workflow expressions", "actionlint", ["-shellcheck=", "-pyflakes="]],
  ["Fuzz reporter tests", "node", ["--test", "scripts/fuzz.test.mjs"]],
  ["Evidence publication tests", "node", ["--test", "scripts/evidence.test.mjs"]],
  ["Format", "cargo", ["fmt", "--all", "--check"]],
  ["Lint", "cargo", ["clippy", "--all-targets", "--locked", "--", "-D", "warnings"]],
  ["Test", "cargo", ["test", "--locked"]],
  ["Dependency policy", "cargo", ["deny", "--locked", "check"]],
  ["Desktop: install", "pnpm", ["install", "--frozen-lockfile"], desktop],
  // All desktop unit tests, including the generated-contract freshness check.
  ["Desktop: app tests", "cargo", ["test", "-p", "ibis-desktop", "--locked"]],
  ["Desktop: type check", "pnpm", ["typecheck"], desktop],
  ["Desktop: interface tests", "pnpm", ["test"], desktop],
  ["Desktop: lint app", "cargo", ["clippy", "-p", "ibis-desktop", "--all-targets", "--locked", "--", "-D", "warnings"]],
];

// On Windows commands run through a shell so that pnpm.cmd resolves; a missing
// tool then fails like any other command instead of raising ENOENT. So when a
// step fails, ask the tool for its version: if that fails too, the tool is
// missing and the run is incomplete rather than failed.
const shell = process.platform === "win32";
const skipped = [];
for (const [name, cmd, args, options = {}] of steps) {
  console.log(`\n== ${name}: ${cmd} ${args.join(" ")}`);
  const result = spawnSync(cmd, args, { stdio: "inherit", shell, ...options });
  if (result.status === 0) continue;
  const probe = cmd === "actionlint" ? ["-version"] : args[0] === "deny" ? ["deny", "--version"] : ["--version"];
  if (result.error?.code === "ENOENT" || !hasCommand(cmd, probe)) {
    console.log(`${name} could not run: the tool is not installed.`);
    skipped.push(name);
    continue;
  }
  // Always 1, so that a tool's own status (for example 2) never reads as incomplete.
  console.error(`\n✗ ${name} failed. Fix the problem above, then run this script again.`);
  process.exit(1);
}
if (skipped.length > 0) {
  console.error(`\n! INCOMPLETE: ${skipped.join(", ")} did not run. Install the missing tools (cargo-deny: cargo install --locked cargo-deny; pnpm: https://pnpm.io/installation; actionlint: https://github.com/rhysd/actionlint/blob/main/docs/install.md).`);
  process.exit(2);
}
console.log("\n✓ All checks passed.");

function hasCommand(cmd, args) {
  const probe = spawnSync(cmd, args, { stdio: "ignore", shell });
  return probe.status === 0;
}
