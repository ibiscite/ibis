#!/usr/bin/env node
// What a person feels, measured and judged (docs/architecture.md › Performance
// checks). crates/ibis-engine/tests/perf.rs measures in fresh release
// processes; this file owns every limit.
//
//   node scripts/perf.mjs run [--rounds N]            print the medians
//   node scripts/perf.mjs check CLASS [--rounds N]    fail over CLASS's budgets
//   node scripts/perf.mjs compare BASE [--rounds N]   fail on a regression against BASE
import { spawnSync } from "node:child_process";
import { appendFileSync, copyFileSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const harness = "crates/ibis-engine/tests/perf.rs";
export const cases = ["feel_10k", "feel_100k"];

// Absolute budgets per machine class, from the medians recorded in
// docs/architecture.md › Performance checks. On a fixed machine: timings 1.5x
// the median and at least 2 ms over it, memory 1.2x, idle 0.5% of a core. On
// shared runners: timings 2.5x the Windows median and at least 5 ms over it,
// memory 1.5x the Linux median, idle 1%. A budget never sits above what a
// person notices: typing 50 ms, first search, first citation and warm
// formatting 100 ms. `null` is a class still waiting for a quiet measurement.
export const budgets = {
  windows: {
    "10k": { open_ms: 900, first_search_ms: 4, summon_ms: 3, typing_p95_ms: 4, first_cite_ms: 30, format_p50_ms: 80, refresh_ms: 1200, rss_mib: 55, idle_cpu_percent: 0.5 },
    "100k": { open_ms: 9000, first_search_ms: 25, summon_ms: 4, typing_p95_ms: 35, first_cite_ms: 35, format_p50_ms: 85, refresh_ms: 8500, rss_mib: 450, idle_cpu_percent: 0.5 },
  },
  // Timings wait for a quiet run; memory does not move with load.
  linux: {
    "10k": { rss_mib: 65 },
    "100k": { rss_mib: 520 },
  },
  mac: null,
  shared: {
    "10k": { open_ms: 1500, first_search_ms: 8, summon_ms: 6, typing_p95_ms: 8, first_cite_ms: 50, format_p50_ms: 130, refresh_ms: 2000, rss_mib: 80, idle_cpu_percent: 1 },
    "100k": { open_ms: 15000, first_search_ms: 40, summon_ms: 8, typing_p95_ms: 60, first_cite_ms: 55, format_p50_ms: 140, refresh_ms: 14000, rss_mib: 650, idle_cpu_percent: 1 },
  },
};

// A regression is a median slower than the base by both the ratio and the
// floor, so 1.0 ms becoming 1.3 ms is noise, not a failure.
export const ratio = 1.2;
// Maxima and the p95 of 30 formatting samples move by a third between runs of
// the same code, so regressions read medians and the 252-sample typing p95.
export const floors = {
  open_ms: 50, first_search_ms: 3, summon_ms: 2, typing_p50_ms: 2, typing_p95_ms: 3, first_cite_ms: 10,
  format_p50_ms: 10, refresh_ms: 75, rss_mib: 15,
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
    if (command === "check" && !(target in budgets)) throw new Error(`Usage: perf.mjs check ${Object.keys(budgets).join("|")}`);
    if (command === "check" && !budgets[target]) throw new Error(`The ${target} budgets wait for a quiet measurement: run \`perf.mjs run\` and record it`);
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
    // Cargo gives both checkouts the same executable path in one target
    // directory: keep head's binary aside before the base build replaces it.
    const directory = mkdtempSync(join(tmpdir(), "ibis-perf-base-"));
    const head = join(directory, `head${process.platform === "win32" ? ".exe" : ""}`);
    copyFileSync(build(root), head);
    const checkout = join(directory, "base");
    try {
      run("git", ["worktree", "add", "--detach", checkout, target], { cwd: root });
      copyFileSync(join(root, harness), join(checkout, harness));
      let base;
      try {
        base = build(checkout);
      } catch (error) {
        const text = `### Performance against ${target.slice(0, 12)}\n\nSkipped: the base does not build with this harness (${error.message}).\n`;
        process.stdout.write(text);
        if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, text);
        return 0;
      }
      // A change that leaves the engine byte for byte the same has nothing to
      // compare; it still meets the shared budgets.
      if (readFileSync(base).equals(readFileSync(head))) {
        const reports = [];
        for (let round = 0; round < count; round++) for (const name of cases) reports.push(measure(head, name));
        const summary = summarize(reports);
        const over = overBudget(summary, budgets.shared);
        report(`Performance (the engine is unchanged from ${target.slice(0, 12)})`, { median: summary }, over);
        return over.length ? 1 : 0;
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
      spawnSync("git", ["worktree", "remove", "--force", checkout], { cwd: root });
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
