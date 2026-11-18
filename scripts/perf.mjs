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
// docs/architecture.md › Performance checks. Timings get 2x the median and at
// least 3 ms over it on a fixed machine (one Windows session ran 2x slower
// with background work), 2.5x and at least 5 ms over it on shared runners.
// Memory barely moves between runs: 1.2x. Idle: 0.5% of a core on a fixed
// machine, 1% on shared runners. On a person's machine no budget sits above
// what they notice: typing 50 ms; first search, first citation and warm
// formatting 100 ms. `null` is a class still waiting for a quiet measurement.
export const budgets = {
  windows: {
    "10k": { open_ms: 1200, first_search_ms: 5, summon_ms: 4, typing_p95_ms: 5, first_cite_ms: 40, format_p50_ms: 100, format_warmup_ms: 420, refresh_ms: 1600, rss_mib: 55, idle_cpu_percent: 0.5 },
    "100k": { open_ms: 12000, first_search_ms: 30, summon_ms: 5, typing_p95_ms: 45, first_cite_ms: 45, format_p50_ms: 100, format_warmup_ms: 420, refresh_ms: 11500, rss_mib: 450, idle_cpu_percent: 0.5 },
  },
  // Timings wait for a quiet run; memory does not move with load.
  linux: {
    "10k": { rss_mib: 65 },
    "100k": { rss_mib: 520 },
  },
  mac: null,
  // GitHub's ubuntu-24.04 runners, the release checks' machine.
  shared: {
    "10k": { open_ms: 650, first_search_ms: 6, summon_ms: 6, typing_p95_ms: 7, first_cite_ms: 30, format_p50_ms: 75, format_warmup_ms: 290, refresh_ms: 1200, rss_mib: 65, idle_cpu_percent: 1 },
    "100k": { open_ms: 6500, first_search_ms: 30, summon_ms: 7, typing_p95_ms: 25, first_cite_ms: 35, format_p50_ms: 80, format_warmup_ms: 290, refresh_ms: 6000, rss_mib: 520, idle_cpu_percent: 1 },
  },
};

// A regression is a median slower than the base by both the ratio and the
// floor, so 1.0 ms becoming 1.3 ms is noise, not a failure.
export const ratio = 1.2;
// Maxima and the p95 of 30 formatting samples move by a third between runs of
// the same code, so regressions read medians and the 252-sample typing p95.
export const floors = {
  open_ms: 50, first_search_ms: 3, summon_ms: 2, typing_p50_ms: 2, typing_p95_ms: 3, first_cite_ms: 10,
  format_p50_ms: 10, format_warmup_ms: 30, refresh_ms: 75, rss_mib: 15,
};
const budgetMetrics = [...Object.keys(floors).filter((metric) => metric !== "typing_p50_ms"), "idle_cpu_percent"];

export function missingBudgets(limits) {
  return cases.flatMap((name) => {
    const size = name.slice("feel_".length);
    return budgetMetrics.filter((metric) => !Number.isFinite(limits?.[size]?.[metric])).map((metric) => `${size} ${metric}`);
  });
}

function requireBudgets(target) {
  const limits = budgets[target];
  const missing = missingBudgets(limits);
  if (missing.length) throw new Error(`The ${target} budgets are incomplete (${missing.join(", ")}); measure with \`node scripts/perf.mjs run\``);
  return limits;
}

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
      if (!Number.isFinite(value)) over.push(`${size} ${metric}: not measured`);
      else if (value > limit) over.push(`${size} ${metric}: ${round(value)} over ${limit}`);
    }
  }
  return over;
}

function exceedsRegression(head, base, floor) {
  return head > base * ratio && head - base > floor;
}

export function regressions(head, base, confirmation) {
  const slower = [];
  for (const [size, metrics] of Object.entries(head)) {
    for (const [metric, floor] of Object.entries(floors)) {
      const now = metrics[metric], then = base[size]?.[metric];
      if (!exceedsRegression(now, then, floor)) continue;
      if (confirmation && !exceedsRegression(confirmation.head[size]?.[metric], confirmation.base[size]?.[metric], floor)) continue;
      slower.push(`${size} ${metric}: ${round(then)} → ${round(now)} (+${Math.round((now - then) / then * 100)}%)`);
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
  const result = spawnSync("cargo", ["test", "-p", "ibis-engine", "--locked", "--release", "--test", "perf", "--no-run", "--message-format=json"],
    { cwd: directory, env, encoding: "utf8", maxBuffer: 256 * 1024 * 1024, stdio: ["ignore", "pipe", "inherit"] });
  if (result.error) throw result.error;
  const messages = result.stdout.split("\n").filter((line) => line.startsWith("{")).map((line) => JSON.parse(line));
  const artifact = messages.find((item) => item.reason === "compiler-artifact" && item.target?.name === "perf" && item.executable);
  if (result.status !== 0 || !artifact) {
    const error = messages.find((item) => item.message?.level === "error")?.message.message;
    throw new Error(`the harness does not build${error ? `: ${error}` : ""}`);
  }
  return artifact.executable;
}

export function measure(executable, name, env = {}) {
  const out = run(executable, [name, "--exact", "--ignored", "--nocapture", "--test-threads=1"], { env: { ...process.env, ...env } });
  // libtest prints `test NAME ... ` on the same line, before the report.
  const found = /PERF (\{.*\})/.exec(out);
  if (!found) throw new Error(`${name} printed no measurements`);
  const report = JSON.parse(found[1]);
  const records = Number(name.slice("feel_".length, -1)) * 1000;
  if (report.records !== records) throw new Error(`${name} reported ${report.records} records, expected ${records}`);
  for (const metric of Object.keys(floors)) {
    if (!Number.isFinite(report[metric])) throw new Error(`${name} did not measure ${metric}`);
  }
  return report;
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
  const outcome = problems ? (problems.length ? problems.map((line) => `- ${line}`).join("\n") : "Within limits.") : "Measurements only.";
  const text = `### ${title}\n\n${table(["case", "metric", ...names], rows)}\n\n${outcome}\n`;
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
  const count = at === -1 ? 3 : Number(args[at + 1]);
  if (!Number.isSafeInteger(count) || count < 1) throw new Error("--rounds requires a positive integer");
  return count;
}

/** Measure one build alone, judged against `limits` when given. */
function alone(executable, count, limits, title) {
  const reports = [];
  for (let round = 0; round < count; round++) for (const name of cases) reports.push(measure(executable, name));
  const summary = summarize(reports);
  console.log(`PERF-SUMMARY ${JSON.stringify({ platform: process.platform, load: load(), rounds: count, summary })}`);
  const over = limits ? overBudget(summary, limits) : null;
  report(title, { median: summary }, over);
  return over?.length ? 1 : 0;
}

function compareBatch(head, base, count, reverse = false) {
  const reports = { head: [], base: [] };
  for (let round = 0; round < count; round++) {
    for (const name of cases) {
      const baseFirst = Boolean(round % 2) !== reverse;
      for (const side of baseFirst ? ["base", "head"] : ["head", "base"]) {
        if (side === "head") {
          reports.head.push(measure(head, name));
          continue;
        }
        try {
          reports.base.push(measure(base, name, { IBIS_PERF_IDLE_SECONDS: "0" }));
        } catch (error) {
          return { baseError: error };
        }
      }
    }
  }
  return { base: summarize(reports.base), head: summarize(reports.head) };
}

function main(args) {
  const [command, target] = args;
  const count = rounds(args);
  if (command === "run") return alone(build(root), count, null, "Performance");
  if (command === "check") {
    if (!Object.hasOwn(budgets, target)) throw new Error(`Usage: perf.mjs check ${Object.keys(budgets).join("|")}`);
    const limits = requireBudgets(target);
    return alone(build(root), count, limits, `Performance budgets (${target})`);
  }
  if (command === "compare" && target) {
    const short = target.slice(0, 12);
    const limits = requireBudgets("shared");
    // Nothing the engine is built from changed: there is nothing to compare,
    // and the shared budgets still apply.
    const unchanged = spawnSync("git", ["diff", "--quiet", target, "--", "crates", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo"], { cwd: root }).status === 0;
    if (unchanged) return alone(build(root), count, limits, `Performance (the engine is unchanged from ${short})`);
    // Cargo gives both checkouts the same executable path in one target
    // directory: keep head's binary aside before the base build replaces it.
    const directory = mkdtempSync(join(tmpdir(), "ibis-perf-base-"));
    const head = join(directory, `head${process.platform === "win32" ? ".exe" : ""}`);
    const checkout = join(directory, "base");
    try {
      copyFileSync(build(root), head);
      run("git", ["worktree", "add", "--detach", checkout, target], { cwd: root });
      // Both sides run head's harness, so a change to the measurement itself
      // never reads as a regression.
      copyFileSync(join(root, harness), join(checkout, harness));
      const withoutBase = (error) => alone(head, count, limits, `Performance (not compared with ${short}: ${error.message})`);
      let base;
      try {
        base = build(checkout);
      } catch (error) {
        return withoutBase(error);
      }
      const summaries = compareBatch(head, base, count);
      if (summaries.baseError) return withoutBase(summaries.baseError);
      const absolute = overBudget(summaries.head, limits);
      let relative = regressions(summaries.head, summaries.base);
      let confirmation;
      let confirmedAbsolute = [];
      // A short load burst can affect several adjacent timings. Before a
      // relative failure, repeat the comparison with the opposite run order.
      if (relative.length && !absolute.length) {
        console.log(`Confirming relative regressions with reversed run order:\n${relative.join("\n")}`);
        confirmation = compareBatch(head, base, count, true);
        if (confirmation.baseError) throw confirmation.baseError;
        relative = regressions(summaries.head, summaries.base, confirmation);
        confirmedAbsolute = overBudget(confirmation.head, limits);
      }
      console.log(`PERF-SUMMARY ${JSON.stringify({ platform: process.platform, load: load(), rounds: count, ...summaries, confirmation })}`);
      const display = confirmation ? { ...summaries, "base repeat": confirmation.base, "head repeat": confirmation.head } : summaries;
      const problems = [...relative, ...absolute, ...confirmedAbsolute];
      report(`Performance against ${short}`, display, problems);
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
