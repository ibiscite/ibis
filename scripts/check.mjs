#!/usr/bin/env node
// One command for contributors and CI-equivalent local checks:
//   node scripts/check.mjs
// Runs formatting, lints, tests and the dependency policy. Never edits files.
import { spawnSync } from "node:child_process";

const steps = [
  ["Format", "cargo", ["fmt", "--all", "--check"]],
  ["Lint", "cargo", ["clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"]],
  ["Test", "cargo", ["test", "--workspace", "--locked"]],
  ["Dependency policy", "cargo", ["deny", "check"]],
];

for (const [name, cmd, args] of steps) {
  console.log(`\n== ${name}: ${cmd} ${args.join(" ")}`);
  const result = spawnSync(cmd, args, { stdio: "inherit", shell: process.platform === "win32" });
  if (result.error?.code === "ENOENT" && args[0] === "deny") {
    console.log("cargo-deny is not installed; skipping (CI runs it). Install: cargo install --locked cargo-deny");
    continue;
  }
  if (result.status !== 0) {
    console.error(`\n✗ ${name} failed. Fix the problem above, then run this script again.`);
    process.exit(result.status ?? 1);
  }
}
console.log("\n✓ All checks passed.");
