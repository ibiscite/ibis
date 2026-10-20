#!/usr/bin/env node
// One command for contributors and CI-equivalent local checks:
//   node scripts/check.mjs
// For final-commit evidence: node scripts/check.mjs --expect-commit FULL_SHA
// Runs formatting, lints, tests and the dependency policy. Never edits files.
// Exit codes: 0 all passed, 1 a check failed, 2 incomplete (a tool is missing).
import { spawnSync } from "node:child_process";

const args = process.argv.slice(2);
if (args.length > 0) {
  if (args.length !== 2 || args[0] !== "--expect-commit" || args[1].length !== 40 || !/^[a-f0-9]{40}$/i.test(args[1])) {
    console.error("Usage: node scripts/check.mjs [--expect-commit FULL_SHA]");
    process.exit(1);
  }
  verifyCommit(args[1].toLowerCase());
}

const desktop = { cwd: "apps/desktop" };
const steps = [
  ["Workflow expressions", "actionlint", ["-shellcheck=", "-pyflakes="]],
  ["Source verification tests", "node", ["--test", "scripts/check.test.mjs"]],
  ["Fuzz reporter tests", "node", ["--test", "scripts/fuzz.test.mjs"]],
  ["Evidence publication tests", "node", ["--test", "scripts/evidence.test.mjs"]],
  ["Update feed tests", "node", ["--test", "scripts/update-feed.test.mjs"]],
  ["Hygiene tests", "node", ["--test", "scripts/hygiene.test.mjs"]],
  ["Release notes tests", "node", ["--test", "scripts/release-notes.test.mjs"]],
  ["Format", "cargo", ["fmt", "--all", "--check"]],
  ["Lint", "cargo", ["clippy", "--all-targets", "--locked", "--", "-D", "warnings"]],
  ["Test", "cargo", ["test", "--locked"]],
  ["Dependency policy", "cargo", ["deny", "--locked", "check"]],
  ["Desktop: install", "pnpm", ["install", "--frozen-lockfile"], desktop],
  ["Desktop: UI import boundary tests", "node", ["--test", "scripts/ui-imports.test.mjs"]],
  ["Desktop: UI import boundary", "node", ["scripts/ui-imports.mjs"]],
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

function verifyCommit(expected) {
  const refuse = (message) => {
    console.error(`Verification refused: ${message}`);
    process.exit(1);
  };
  if (["GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR", "GIT_INDEX_FILE"].some((name) => process.env[name])) {
    refuse("Git source overrides are not supported for guarded checks.");
  }
  const git = (args) => {
    // Capture diagnostics: Git errors and porcelain filenames can be private.
    // Never start a configured monitor: killing Git does not stop its helpers.
    const result = spawnSync("git", ["-c", "core.fsmonitor=false", ...args], {
      encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], shell: false,
      timeout: 5_000, killSignal: "SIGKILL",
    });
    if (result.status !== 0) refuse("Git could not verify the source checkout.");
    return result.stdout.trim();
  };
  if (git(["rev-parse", "--is-inside-work-tree"]) !== "true" || git(["rev-parse", "--show-prefix"]) !== "") {
    refuse("run from the repository root.");
  }
  const head = git(["rev-parse", "--verify", "HEAD^{commit}"]);
  if (head !== expected) refuse("expected commit does not match repository HEAD.");
  if (git(["ls-files", "-v", "-z"]).split("\0").some((entry) => /^[a-zS] /.test(entry))) {
    refuse("index flags can hide tracked source changes.");
  }
  if (git(["status", "--porcelain=v1", "--untracked-files=all", "--ignore-submodules=none"]) !== "") {
    refuse("source checkout has uncommitted changes.");
  }
  console.log(`Verified source commit ${head} (clean checkout).`);
}

function hasCommand(cmd, args) {
  const probe = spawnSync(cmd, args, { stdio: "ignore", shell });
  return probe.status === 0;
}
