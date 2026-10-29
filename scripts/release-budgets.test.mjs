import assert from "node:assert/strict";
import test from "node:test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { percentiles, releaseBudgets } from "./release-budgets.mjs";

const revision = "a".repeat(40);
function evidence() {
  return { schema: 1, revision, platform: "macOS", machine: "synthetic-release-machine",
    measurements: [10000, 100000].flatMap((references) => ["cold", "warm"].map((mode) => ({
      mode, references, settle_seconds: 60,
      samples: Array.from({ length: 30 }, () => ({ open_ms: 100, typing_ms: 50, idle_cpu_percent: 0.2, wakeups_per_second: 1, memory_bytes: 100_000_000 })),
    }))) };
}

test("release evidence reports nearest-rank p50/p95/p99 on the named machine and exact revision", () => {
  assert.deepEqual(percentiles(Array.from({ length: 100 }, (_, i) => i + 1)), { p50: 50, p95: 95, p99: 99 });
  const report = releaseBudgets(evidence(), revision);
  assert.equal(report.status, "passed");
  assert.equal(report.machine, "synthetic-release-machine");
  assert.equal(report.measurements.length, 4);
  assert.deepEqual(report.measurements[0].summary.open_ms, { p50: 100, p95: 100, p99: 100 });
});

test("fixed latency, CPU, wakeup and whole-process-tree memory limits fail", () => {
  for (const [metric, value] of [["open_ms", 100.001], ["typing_ms", 50.001], ["idle_cpu_percent", 0.201], ["wakeups_per_second", 1.001], ["memory_bytes", 200 * 1024 * 1024 + 1]]) {
    const report = evidence();
    for (const sample of report.measurements[0].samples) sample[metric] = value;
    assert.throws(() => releaseBudgets(report, revision), /exceeds/);
  }
  const boundary = evidence();
  boundary.measurements[0].samples[0].memory_bytes = 200 * 1024 * 1024;
  assert.doesNotThrow(() => releaseBudgets(boundary, revision));
  const report = evidence();
  report.measurements[2].samples[0].memory_bytes = 400_000_000;
  assert.throws(() => releaseBudgets(report, revision), /exceeds/);
});

test("missing, stale, partial, duplicate, malformed and unsettled evidence fails closed", () => {
  for (const change of [
    (r) => { r.revision = "b".repeat(40); },
    (r) => { r.machine = ""; },
    (r) => { r.platform = "unknown"; },
    (r) => { r.measurements.pop(); },
    (r) => { r.measurements[0] = r.measurements[1]; },
    (r) => { r.measurements[0].settle_seconds = 59; },
    (r) => { r.measurements[0].samples.pop(); },
    (r) => { r.measurements[0].samples[0].memory_bytes = 0; },
    (r) => { r.measurements[0].samples[0].memory_bytes = 0.5; },
    (r) => { r.measurements[0].samples[0].open_ms = NaN; },
    (r) => { r.measurements[0].samples[0].open_ms = true; },
    (r) => { r.measurements[0].samples[0].open_ms = -1; },
    (r) => { r.measurements[0].samples[0].open_ms = Infinity; },
    (r) => { r.private_path = "synthetic"; },
  ]) {
    const report = evidence();
    change(report);
    assert.throws(() => releaseBudgets(report, revision));
  }
});

test("release validator CLI exits unsuccessfully for missing, stale and over-budget evidence", (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-release-budget-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const path = join(root, "synthetic.json");
  const script = fileURLToPath(new URL("./release-budgets.mjs", import.meta.url));
  const run = (args) => spawnSync(process.execPath, [script, ...args], { encoding: "utf8" });
  assert.equal(run([]).status, 1);
  writeFileSync(path, JSON.stringify(evidence()));
  assert.equal(run([path, revision]).status, 0);
  assert.equal(run([path, "b".repeat(40)]).status, 1);
  const bad = evidence();
  bad.measurements[0].samples[0].wakeups_per_second = 2;
  writeFileSync(path, JSON.stringify(bad));
  assert.equal(run([path, revision]).status, 1);
  writeFileSync(path, "{");
  assert.equal(run([path, revision]).status, 1);
});
