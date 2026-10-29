// Validate measurements captured on a named release machine. Collection uses
// native profilers; a CI VM's debug/accessibility smoke run is not this evidence.
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const limits = { open_ms: 100, typing_ms: 50, idle_cpu_percent: 0.2, wakeups_per_second: 1 };
const metrics = [...Object.keys(limits), "memory_bytes"];
function require(condition, message) {
  if (!condition) throw new Error(message);
}
function fields(value, names) {
  require(value && typeof value === "object" && !Array.isArray(value)
    && Object.keys(value).sort().join() === [...names].sort().join(), "Invalid evidence fields");
}
function number(value) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}
export function percentiles(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const at = (p) => sorted[Math.ceil(sorted.length * p) - 1];
  return { p50: at(0.5), p95: at(0.95), p99: at(0.99) };
}

export function releaseBudgets(report, revision) {
  fields(report, ["schema", "revision", "platform", "machine", "measurements"]);
  require(report.schema === 1 && /^[a-f0-9]{40}$/.test(report.revision), "Invalid release revision");
  require(report.revision === revision, "Evidence must match the release revision");
  require(["macOS", "Windows", "Linux"].includes(report.platform), "Invalid release platform");
  require(typeof report.machine === "string" && /^[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}$/.test(report.machine), "Use a stable release machine name");
  require(Array.isArray(report.measurements) && report.measurements.length === 4, "Need cold and warm runs at 10k and 100k");
  const seen = new Set();
  const measurements = report.measurements.map((run) => {
    fields(run, ["mode", "references", "settle_seconds", "samples"]);
    require(["cold", "warm"].includes(run.mode) && [10000, 100000].includes(run.references), "Invalid measurement case");
    const key = `${run.mode}-${run.references}`;
    require(!seen.has(key), "Duplicate measurement case");
    seen.add(key);
    require(number(run.settle_seconds) && run.settle_seconds >= 60, "Settle for at least 60 seconds");
    require(Array.isArray(run.samples) && run.samples.length >= 30, "Need at least 30 samples per case");
    for (const sample of run.samples) {
      fields(sample, metrics);
      require(metrics.every((metric) => number(sample[metric]))
        && Number.isSafeInteger(sample.memory_bytes) && sample.memory_bytes > 0, "Invalid metric");
    }
    const summary = Object.fromEntries(metrics.map((metric) => [metric, percentiles(run.samples.map((s) => s[metric]))]));
    for (const [metric, limit] of Object.entries(limits)) {
      // Latency is explicitly a p95 budget. Idle CPU and wakeups are ceilings.
      const value = metric === "open_ms" || metric === "typing_ms" ? summary[metric].p95 : Math.max(...run.samples.map((s) => s[metric]));
      require(value <= limit, `${key}: ${metric} exceeds ${limit}`);
    }
    const memoryLimit = run.references === 10000 ? 200 * 1024 * 1024 : 400_000_000;
    require(run.samples.every((s) => run.references === 10000 ? s.memory_bytes <= memoryLimit : s.memory_bytes < memoryLimit), `${key}: memory_bytes exceeds ${memoryLimit}`);
    return { mode: run.mode, references: run.references, samples: run.samples.length, summary };
  });
  return { status: "passed", revision, platform: report.platform, machine: report.machine, measurements };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    require(process.argv.length === 4, "Usage: node scripts/release-budgets.mjs evidence.json RELEASE_SHA");
    console.log(JSON.stringify(releaseBudgets(JSON.parse(readFileSync(process.argv[2], "utf8")), process.argv[3]), null, 2));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
