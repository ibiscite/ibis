#!/usr/bin/env node
// One command to trust before pushing:
//   node scripts/check.mjs         what CI would run for this branch's changes
//   node scripts/check.mjs --all   every routine local check
//   node scripts/check.mjs --plan  the check classes selected by changed paths
// Runs formatting, lints, tests and the dependency policy. Never edits files.
// Exit codes: 0 passed, 1 failed, 2 unavailable, 75 infrastructure.
import { spawnSync } from "node:child_process";
import { readdirSync, realpathSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { classify, parseNameStatus } from "./ci-paths.mjs";

/** Lucide outlines stay on the 24 unit grid, with room for the source stroke.
 * Curves use bounded control points too: no hidden transform can move them. */
export function iconProblems(svg) {
  const problems = [];
  const expected = { xmlns: "http://www.w3.org/2000/svg", width: "24", height: "24", viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", "stroke-width": "2", "stroke-linecap": "round", "stroke-linejoin": "round" };
  const root = svg.match(/^<svg\s+([^>]+)><path d="([^"]+)"\s*\/><\/svg>\s*$/);
  if (!root) return ["one svg and one untransformed path required"];
  // Every attribute counts, however it is quoted: nothing unread can move the drawing.
  const attribute = /([\w:-]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/g;
  const attributes = [...root[1].matchAll(attribute)].map(([, key, double, single]) => [key, double ?? single]);
  const unread = root[1].replace(attribute, "").trim();
  if (unread || attributes.length !== Object.keys(expected).length || new Set(attributes.map(([key]) => key)).size !== attributes.length || attributes.some(([key, value]) => expected[key] !== value)) problems.push("24 unit grid, no fill, 2 unit source stroke, round caps and joins required");
  const tokens = root[2].match(/[a-z]|[-+]?(?:\d*\.\d+|\d+\.?\d*)/gi) ?? [];
  if (root[2].replace(/[a-z]|[-+]?(?:\d*\.\d+|\d+\.?\d*)|[\s,]/gi, "")) problems.push("invalid path data");
  const counts = { M: 2, L: 2, H: 1, V: 1, C: 6, S: 4, Q: 4 };
  let index = 0, command = "", x = 0, y = 0, startX = 0, startY = 0;
  let controlX = 0, controlY = 0, previous = "";
  const within = (a, b) => a >= 1 && a <= 23 && b >= 1 && b <= 23;
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
      if (upper === "S" && ["C", "S"].includes(previous) && !within(2 * x - controlX, 2 * y - controlY)) problems.push("path leaves room for less than half the source stroke");
      for (let i = 0; i < count; i += 2) {
        x = values[i] + (relative ? originX : 0); y = values[i + 1] + (relative ? originY : 0);
        if (!within(x, y)) problems.push("path leaves room for less than half the source stroke");
      }
      if (["C", "S"].includes(upper)) {
        controlX = values[count - 4] + (relative ? originX : 0); controlY = values[count - 3] + (relative ? originY : 0);
      }
    }
    if (!within(x, y)) problems.push("path leaves room for less than half the source stroke");
    if (upper === "M") { startX = x; startY = y; command = relative ? "l" : "L"; }
    previous = upper;
  }
  return [...new Set(problems)];
}

/** This branch's changes against origin/main, committed or not, in the shape
 * of GitHub's file list. Anything unclear returns null: CI then runs everything. */
export function changedFiles(run = git) {
  const base = run(["merge-base", "origin/main", "HEAD"])?.trim();
  const tracked = base && run(["diff", "--name-status", "-M", "-z", base]);
  const untracked = run(["ls-files", "--others", "--exclude-standard", "-z"]);
  if (tracked == null || tracked === false || untracked == null) return null;
  return [...untracked.split("\0").filter(Boolean).map((filename) => ({ filename, status: "added" })), ...parseNameStatus(tracked)];
}

/** The same path classification used by CI. */
export function plan(files = changedFiles()) {
  return Object.entries(classify("pull_request", files?.length, files)).filter(([, value]) => value).map(([name]) => name);
}

function git(args) {
  const result = spawnSync("git", args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  return result.status === 0 ? result.stdout : null;
}

/** One executable local gate; land.sh delegates every check to this plan. */
export function checkPlan(files, { all = false, scriptsOnly = false, brandOnly = false, inventory = readdirSync(new URL(".", import.meta.url)).sort() } = {}) {
  const selected = classify(all ? "all" : "pull_request", files?.length, files);
  const names = files?.flatMap(({ filename, previous_filename }) => [filename, previous_filename].filter(Boolean)) ?? [];
  const step = (name, command, args, options = {}, probes) => ({ name, command, args, options,
    probes: probes ?? [[command, command === "actionlint" ? ["-version"] : ["--version"], `${command} is not installed or cannot start`]],
  });
  const python = process.platform === "win32" ? "python" : "python3";
  const brand = step("Brand assets", python, ["brand/build.py", "--check"], {}, [
    [python, ["--version"], "Python is not installed or cannot start"],
    ["rsvg-convert", ["--version"], "librsvg's rsvg-convert is not installed"],
    ["fc-match", ["--version"], "fontconfig is not installed"],
    [python, ["-c", "import subprocess, sys; sys.exit('Inter Display' not in subprocess.check_output(['fc-match', '-f', '%{family}', 'Inter Display'], text=True).split(','))"], "Inter Display is not installed"],
  ]);
  if (brandOnly) return [brand];
  const related = inventory.slice().sort().filter((name) => /\.test\.(mjs|cjs)$/.test(name)).filter((name) =>
    scriptsOnly || all || !files || selected.scripts || names.some((path) => path.startsWith("website/") && name.startsWith("website")));
  const scriptSteps = [
    ...inventory.filter((name) => /^check-[a-z-]+\.mjs$/.test(name) && (!scriptsOnly || name !== "check-agent-identity.mjs"))
      .map((name) => step(`Repository guard: ${name}`, "node", [`scripts/${name}`])),
    ...(related.length ? [step("Script tests", "node", ["--test", ...related.map((name) => `scripts/${name}`)])] : []),
    ...inventory.filter((name) => /^test_.*\.py$|\.test\.py$/.test(name))
      .filter((name) => scriptsOnly || all || !files || names.some((path) => path === `scripts/${name}` || path.replace(/\.py$/, ".test.py") === `scripts/${name}`))
      .map((name) => step(`Python script tests: ${name}`, python, ["-B", `scripts/${name}`])),
  ];
  if (scriptsOnly) return scriptSteps;
  const cargo = (name, args, native = false) => step(name, "cargo", args, {}, [
    ["cargo", ["--version"], "Cargo is not installed or cannot start"],
    ...["fmt", "clippy"].includes(args[0]) ? [["cargo", [args[0], "--version"], `cargo ${args[0]} is not installed`]] : [],
    ...native && process.platform === "linux" ? [["pkg-config", ["--exists", "webkit2gtk-4.1", "libsoup-3.0", "gtk+-3.0"], "Linux desktop WebView development libraries are unavailable"]] : [],
  ]);
  const desktop = { cwd: "apps/desktop" };
  const pnpm = (name, args, tool) => step(name, "pnpm", args, desktop, [
    ["pnpm", ["--version"], "pnpm is not installed or cannot start"],
    ["pnpm", ["exec", tool, "--version"], `desktop dependencies are unavailable (${tool}); install the locked dependencies first`, desktop],
  ]);
  return [
    step("Workflow expressions", "actionlint", ["-shellcheck=", "-pyflakes="]),
    cargo("Format", ["fmt", "--all", "--check"]),
    ...selected.core ? [cargo("Lint", ["clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings", "-D", "clippy::unused_result_ok"], true),
      cargo("Test", ["test", "--workspace", "--locked"], true)] : [],
    ...selected.native ? [brand,
      ...!selected.core ? [cargo("Desktop: lint app", ["clippy", "-p", "ibis-desktop", "-p", "ibis-desktop-macros", "--all-targets", "--locked", "--", "-D", "warnings", "-D", "clippy::unused_result_ok"], true),
        cargo("Desktop: app tests", ["test", "-p", "ibis-desktop", "-p", "ibis-desktop-macros", "--locked"], true)] : []] : [],
    ...selected.web ? [pnpm("Desktop: type check", ["typecheck"], "tsc"), pnpm("Desktop: Biome", ["lint"], "biome"),
      pnpm("Desktop: interface tests", ["test", "--maxWorkers=1"], "vitest")] : [],
    ...scriptSteps,
    // Freshness is checked on every landing, including documentation changes.
    step("Generated files", "node", ["scripts/gen.mjs", "--check"], {}, cargo("", [], true).probes),
    step("Dependency policy", "cargo", ["deny", "--locked", "check", "licenses", "bans", "sources"], {}, [
      ["cargo", ["deny", "--version"], "cargo-deny is not installed"],
    ]),
  ];
}

/** Probe before execution. A check failure is never reinterpreted as absence. */
export function runChecks(steps, run = spawnSync, report = console.log) {
  let unavailable = false;
  const shellFor = (command) => process.platform === "win32" && command === "pnpm";
  for (const { name, command, args, options = {}, probes = [] } of steps) {
    let reason;
    for (const [tool, probeArgs, missing, probeOptions = {}] of probes) {
      const probe = run(tool, probeArgs, { stdio: "ignore", shell: shellFor(tool), timeout: 30_000, ...probeOptions });
      if (probe.signal || probe.status === 75 || probe.error && probe.error.code !== "ENOENT") {
        report(`${name}: infrastructure — prerequisite probe could not complete`); return 75;
      }
      if (probe.status !== 0) { reason = missing; break; }
    }
    if (reason) { report(`${name}: unavailable — ${reason}`); unavailable = true; continue; }
    report(`== ${name}: ${command} ${args.join(" ")}`);
    const result = run(command, args, { stdio: "inherit", shell: shellFor(command), ...options });
    if (result.status === 0) { report(`${name}: passed`); continue; }
    const infrastructure = result.status === 75 || result.error || result.signal;
    report(`${name}: ${infrastructure ? "infrastructure — command interrupted or could not execute" : "failed"}`);
    return infrastructure ? 75 : 1;
  }
  if (unavailable) report("Gate incomplete: unavailable checks must run before landing.");
  else report("All checks passed.");
  return unavailable ? 2 : 0;
}

function main() {
  const [flag, ...extra] = process.argv.slice(2);
  if (extra.length || flag && !["--all", "--plan", "--script-tests", "--platform-script-tests", "--brand-assets"].includes(flag)) {
    console.error("Usage: node scripts/check.mjs [--all | --plan | --script-tests | --platform-script-tests | --brand-assets]");
    process.exitCode = 1; return;
  }
  const files = flag === "--all" || ["--script-tests", "--platform-script-tests", "--brand-assets"].includes(flag) ? null : changedFiles();
  if (flag === "--plan") { console.log(plan(files).join(" ")); return; }
  process.exitCode = runChecks(checkPlan(files, { all: flag === "--all", scriptsOnly: ["--script-tests", "--platform-script-tests"].includes(flag), brandOnly: flag === "--brand-assets" }));
}

if (process.argv[1] && realpathSync(resolve(process.argv[1])) === realpathSync(fileURLToPath(import.meta.url))) main();
