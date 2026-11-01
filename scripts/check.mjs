#!/usr/bin/env node
// One command for contributors and CI-equivalent local checks:
//   node scripts/check.mjs
// Runs formatting, lints, tests and the dependency policy. Never edits files.
// Exit codes: 0 all passed, 1 a check failed, 2 incomplete (a tool is missing).
import { spawnSync } from "node:child_process";
import { readdirSync, realpathSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Outline drawings stay on the 20 pt grid, inside the 16 pt live area.
 * Curves use bounded control points too: no hidden transform can move them. */
export function iconProblems(svg) {
  const problems = [];
  const expected = { xmlns: "http://www.w3.org/2000/svg", width: "20", height: "20", viewBox: "0 0 20 20", fill: "none", stroke: "currentColor", "stroke-width": "1.5", "stroke-linecap": "round", "stroke-linejoin": "round" };
  const root = svg.match(/^<svg\s+([^>]+)><path d="([^"]+)"\s*\/><\/svg>\s*$/);
  if (!root) return ["one svg and one untransformed path required"];
  const attributes = [...root[1].matchAll(/([\w:-]+)="([^"]*)"/g)];
  if (attributes.length !== Object.keys(expected).length || attributes.some(([, key, value]) => expected[key] !== value)) problems.push("20 pt grid, no fill, 1.5 pt stroke, round caps and joins required");
  const tokens = root[2].match(/[a-z]|[-+]?(?:\d*\.\d+|\d+\.?\d*)/gi) ?? [];
  if (root[2].replace(/[a-z]|[-+]?(?:\d*\.\d+|\d+\.?\d*)|[\s,]/gi, "")) problems.push("invalid path data");
  const counts = { M: 2, L: 2, H: 1, V: 1, C: 6, S: 4, Q: 4 };
  let index = 0, command = "", x = 0, y = 0, startX = 0, startY = 0;
  let controlX = 0, controlY = 0, previous = "";
  const within = (a, b) => a >= 2.75 && a <= 17.25 && b >= 2.75 && b <= 17.25;
  if (tokens[0] !== "M") problems.push("path must start with absolute Move");
  while (index < tokens.length) {
    if (/^[a-z]$/i.test(tokens[index])) command = tokens[index++];
    const upper = command.toUpperCase();
    if (upper === "Z") { x = startX; y = startY; command = ""; previous = "Z"; continue; }
    const count = counts[upper];
    if (!count) { problems.push("unsupported path command"); break; }
    const values = tokens.slice(index, index + count).map(Number);
    if (values.length !== count || values.some((v) => !Number.isFinite(v))) { problems.push("incomplete path command"); break; }
    index += count;
    const relative = command !== upper;
    if (upper === "H") x = values[0] + (relative ? x : 0);
    else if (upper === "V") y = values[0] + (relative ? y : 0);
    else {
      const originX = x, originY = y;
      if (upper === "S" && ["C", "S"].includes(previous) && !within(2 * x - controlX, 2 * y - controlY)) problems.push("stroke leaves the 16 pt live area");
      for (let i = 0; i < count; i += 2) {
        x = values[i] + (relative ? originX : 0); y = values[i + 1] + (relative ? originY : 0);
        if (!within(x, y)) problems.push("stroke leaves the 16 pt live area");
      }
      if (["C", "S"].includes(upper)) {
        controlX = values[count - 4] + (relative ? originX : 0); controlY = values[count - 3] + (relative ? originY : 0);
      }
    }
    if (!within(x, y)) problems.push("stroke leaves the 16 pt live area");
    if (upper === "M") { startX = x; startY = y; command = relative ? "l" : "L"; }
    previous = upper;
  }
  return [...new Set(problems)];
}

function main() {
  const args = process.argv.slice(2);
  const scriptsOnly = args.length === 1 && ["--script-tests", "--platform-script-tests"].includes(args[0]);
  const brandOnly = args.length === 1 && args[0] === "--brand-assets";
  if (args.length > 0 && !scriptsOnly && !brandOnly) {
    console.error("Usage: node scripts/check.mjs [--script-tests | --platform-script-tests | --brand-assets]");
    process.exit(1);
  }

  const desktop = { cwd: "apps/desktop" };
  const scriptFiles = readdirSync(new URL(".", import.meta.url)).sort();
  const scriptTests = scriptFiles
    .filter((name) => /\.test\.(mjs|cjs)$/.test(name))
    .map((name) => `scripts/${name}`);
  const scriptSteps = [
    // The identity guard needs the Web job's full history.
    ...scriptFiles.filter((name) => /^check-[a-z-]+\.mjs$/.test(name))
      .filter((name) => args[0] !== "--platform-script-tests" || name !== "check-agent-identity.mjs")
      .map((name) => [`Repository guard: ${name}`, "node", [`scripts/${name}`]]),
    ["Script tests", "node", ["--test", ...scriptTests]],
    ...scriptFiles.filter((name) => /^test_.*\.py$|\.test\.py$/.test(name)).map((name) => [
      `Python script tests: ${name}`, process.platform === "win32" ? "python" : "python3", ["-B", `scripts/${name}`],
    ]),
  ];
  const brandSteps = [["Brand assets", process.platform === "win32" ? "python" : "python3", ["brand/build.py", "--check"]]];
  // Verified against https://clippy.rust-lang.org/stable/index.html:
  // unused_result_ok (restriction): discarded .ok(); enabled below (zero hits).
  // let_underscore_must_use / let_underscore_untyped (restriction): many current
  // deliberate discards; do not enable wholesale without resolving those cases.
  const steps = [
    ["Workflow expressions", "actionlint", ["-shellcheck=", "-pyflakes="]],
    ...brandSteps,
    ["Format", "cargo", ["fmt", "--all", "--check"]],
    ["Lint", "cargo", ["clippy", "--all-targets", "--locked", "--", "-D", "warnings", "-D", "clippy::unused_result_ok"]],
    ["Test", "cargo", ["test", "--locked"]],
    ["Dependency policy", "cargo", ["deny", "--locked", "check"]],
    ["Desktop: install", "pnpm", ["install", "--frozen-lockfile"], desktop],
    ...scriptSteps,
    // All desktop unit tests, including the generated-contract freshness check.
    ["Desktop: app tests", "cargo", ["test", "-p", "ibis-desktop", "--locked"]],
    ["Desktop: type check", "pnpm", ["typecheck"], desktop],
    ["Desktop: hook dependencies", "pnpm", ["lint"], desktop],
    ["Desktop: interface tests", "pnpm", ["test"], desktop],
    ["Desktop: lint app", "cargo", ["clippy", "-p", "ibis-desktop", "--all-targets", "--locked", "--", "-D", "warnings", "-D", "clippy::unused_result_ok"]],
  ];

  // On Windows commands run through a shell so that pnpm.cmd resolves; a missing
  // tool then fails like any other command instead of raising ENOENT. So when a
  // step fails, ask the tool for its version: if that fails too, the tool is
  // missing and the run is incomplete rather than failed.
  const shell = process.platform === "win32";
  const skipped = [];
  for (const [name, cmd, args, options = {}] of scriptsOnly ? scriptSteps : brandOnly ? brandSteps : steps) {
    console.log(`\n== ${name}: ${cmd} ${args.join(" ")}`);
    const result = spawnSync(cmd, args, { stdio: "inherit", shell, ...options });
    if (result.status === 0) continue;
    const probe = cmd === "actionlint" ? ["-version"] : args[0] === "deny" ? ["deny", "--version"] : ["--version"];
    if (result.error?.code === "ENOENT" || !hasCommand(cmd, probe) || name === "Brand assets" && result.status === 2) {
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

}

if (process.argv[1] && realpathSync(resolve(process.argv[1])) === realpathSync(fileURLToPath(import.meta.url))) main();

function hasCommand(cmd, args) {
  const probe = spawnSync(cmd, args, { stdio: "ignore", shell: process.platform === "win32" });
  return probe.status === 0;
}
