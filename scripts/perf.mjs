#!/usr/bin/env node
// What a person feels, measured and judged (docs/architecture.md › Performance
// checks). crates/ibis-engine/tests/perf.rs measures in fresh release
// processes; this file owns every limit.
//
//   node scripts/perf.mjs run [--rounds N]            print the medians
//   node scripts/perf.mjs check CLASS [--rounds N]    fail over CLASS's budgets
//   node scripts/perf.mjs compare BASE [--rounds N]   fail on a regression against BASE
import { spawnSync } from "node:child_process";
import { appendFileSync, copyFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const harness = "crates/ibis-engine/tests/perf.rs";
export const cases = ["feel_10k", "feel_100k"];

// Absolute budgets per machine class, from the medians recorded in
// docs/architecture.md with headroom: 1.5x on a fixed machine, 2.5x on shared
// runners. A budget never sits above what a person notices (typing 50 ms,
// first search and first citation 100 ms, warm formatting 100 ms).
export const budgets = {
  mac: {},
  linux: {},
  windows: {},
  shared: {},
};

// A regression is a median slower than the base by both the ratio and the
// floor, so 1.0 ms becoming 1.3 ms is noise, not a failure.
export const ratio = 1.2;
export const floors = {
  open_ms: 50, first_search_ms: 3, summon_ms: 2, typing_p95_ms: 3, first_cite_ms: 15,
  format_p95_ms: 15, refresh_ms: 75, copy_during_refresh_max_ms: 25, rss_mib: 15,
};

export function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = sorted.length >> 1;
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
}

/** Medians per case and metric over rounds of `PERF` reports. */
export function summarize(reports) {
  const out = {};
  for (const report of reports) {
    const name = `${report.records / 1000}k`;
    out[name] ??= {};
    for (const [metric, value] of Object.entries(report)) {
      if (metric !== "records" && metric !== "typing_n") (out[name][metric] ??= []).push(value);
    }
  }
  for (const metrics of Object.values(out)) for (const metric in metrics) metrics[metric] = median(metrics[metric]);
  return out;
}

export function overBudget(summary, limits) {
  const over = [];
  for (const [size, metrics] of Object.entries(limits)) {
    for (const [metric, limit] of Object.entries(metrics)) {
      const value = summary[size]?.[metric];
      if (value === undefined) over.push(`${size} ${metric}: not measured`);
      else if (value > limit) over.push(`${size} ${metric}: ${round(value)} over ${limit}`);
    }
  }
  return over;
}

export function regressions(head, base) {
  const slower = [];
  for (const [size, metrics] of Object.entries(head)) {
    for (const [metric, floor] of Object.entries(floors)) {
      const now = metrics[metric], then = base[size]?.[metric];
      if (now === undefined || then === undefined) continue;
      if (now > then * ratio && now - then > floor) slower.push(`${size} ${metric}: ${round(then)} → ${round(now)} (+${Math.round((now / then - 1) * 100)}%)`);
    }
  }
  return slower;
}

function round(value) {
  return Math.round(value * 10) / 10;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024, ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    process.stderr.write(`${result.stdout ?? ""}${result.stderr ?? ""}`);
    throw new Error(`${command} ${args.join(" ")} exited ${result.status}`);
  }
  return result.stdout;
}

/** Build the harness in `directory` and return its executable. */
// Both sides share one target directory, so the base rebuilds only the crates
// that differ.
export function build(directory) {
  const env = { ...process.env, CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR ?? join(root, "target") };
  const out = run("cargo", ["test", "-p", "ibis-engine", "--locked", "--release", "--test", "perf", "--no-run", "--message-format=json"],
    { cwd: directory, env, stdio: ["ignore", "pipe", "inherit"] });
  const artifact = out.split("\n").filter(Boolean).map((line) => JSON.parse(line))
    .find((item) => item.reason === "compiler-artifact" && item.target?.name === "perf" && item.executable);
  if (!artifact) throw new Error("the perf harness did not build");
  return artifact.executable;
}

export function measure(executable, name, env = {}) {
  const out = run(executable, [name, "--exact", "--ignored", "--nocapture", "--test-threads=1"], { env: { ...process.env, ...env } });
  // libtest prints `test NAME ... ` on the same line, before the report.
  const found = /PERF (\{.*\})/.exec(out);
  if (!found) throw new Error(`${name} printed no measurements`);
  return JSON.parse(found[1]);
}

function table(columns, rows) {
  return [`| ${columns.join(" | ")} |`, `| ${columns.map(() => "---").join(" | ")} |`, ...rows.map((row) => `| ${row.join(" | ")} |`)].join("\n");
}

function report(title, summaries, problems) {
  const names = Object.keys(summaries);
  const rows = [];
  for (const size of Object.keys(summaries[names[0]])) {
    for (const metric of Object.keys(summaries[names[0]][size])) rows.push([size, metric, ...names.map((name) => String(round(summaries[name][size]?.[metric] ?? NaN)))]);
  }
  const text = `### ${title}\n\n${table(["case", "metric", ...names], rows)}\n\n${problems.length ? problems.map((line) => `- ${line}`).join("\n") : "Within limits."}\n`;
  process.stdout.write(text);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, text);
}

function load() {
  const os = process.platform;
  if (os === "win32") return "n/a";
  return run("uptime", []).trim().replace(/.*load averages?:\s*/, "");
}

function rounds(args) {
  const at = args.indexOf("--rounds");
  return at === -1 ? 3 : Number(args[at + 1]);
}

function main(args) {
  const [command, target] = args;
  const count = rounds(args);
  if (command === "run" || command === "check") {
    if (command === "check" && !budgets[target]) throw new Error(`Usage: perf.mjs check ${Object.keys(budgets).join("|")}`);
    const executable = build(root);
    const reports = [];
    for (let round = 0; round < count; round++) for (const name of cases) reports.push(measure(executable, name));
    const summary = summarize(reports);
    console.log(`PERF-SUMMARY ${JSON.stringify({ platform: process.platform, load: load(), rounds: count, summary })}`);
    const over = command === "check" ? overBudget(summary, budgets[target]) : [];
    report(command === "check" ? `Performance budgets (${target})` : "Performance", { median: summary }, over);
    return over.length ? 1 : 0;
  }
  if (command === "compare" && target) {
    // Both sides run head's harness, so a change to the measurement itself
    // never reads as a regression. A base it cannot build is skipped, visibly.
    const head = build(root);
    const directory = mkdtempSync(join(tmpdir(), "ibis-perf-base-"));
    try {
      run("git", ["worktree", "add", "--detach", directory, target], { cwd: root });
      copyFileSync(join(root, harness), join(directory, harness));
      let base;
      try {
        base = build(directory);
      } catch (error) {
        const text = `### Performance against ${target.slice(0, 12)}\n\nSkipped: the base does not build with this harness (${error.message}).\n`;
        process.stdout.write(text);
        if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, text);
        return 0;
      }
      // Interleaved ABBA rounds share whatever the machine is doing; the base
      // skips the idle wait, which is judged absolutely below.
      const reports = { head: [], base: [] };
      for (let round = 0; round < count; round++) {
        for (const name of cases) {
          const order = round % 2 ? ["base", "head"] : ["head", "base"];
          for (const side of order) {
            reports[side].push(measure(side === "head" ? head : base, name, side === "base" ? { IBIS_PERF_IDLE_SECONDS: "0" } : {}));
          }
        }
      }
      const summaries = { base: summarize(reports.base), head: summarize(reports.head) };
      console.log(`PERF-SUMMARY ${JSON.stringify({ platform: process.platform, load: load(), rounds: count, ...summaries })}`);
      const problems = [...regressions(summaries.head, summaries.base), ...overBudget(summaries.head, budgets.shared)];
      report(`Performance against ${target.slice(0, 12)}`, summaries, problems);
      return problems.length ? 1 : 0;
    } finally {
      spawnSync("git", ["worktree", "remove", "--force", directory], { cwd: root });
      rmSync(directory, { recursive: true, force: true });
    }
  }
  throw new Error("Usage: node scripts/perf.mjs run|check CLASS|compare BASE [--rounds N]");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    process.exitCode = main(process.argv.slice(2));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
