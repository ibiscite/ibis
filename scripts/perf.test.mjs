import assert from "node:assert/strict";
import test from "node:test";
import { budgets, cases, median, overBudget, regressions, summarize } from "./perf.mjs";

const report = (records, values) => ({ records, typing_n: 252, ...values });

test("medians per case ignore the sample count and the record count", () => {
  const summary = summarize([
    report(10000, { open_ms: 30, typing_p95_ms: 1 }),
    report(10000, { open_ms: 90, typing_p95_ms: 3 }),
    report(10000, { open_ms: 40, typing_p95_ms: 2 }),
    report(100000, { open_ms: 400 }),
    report(100000, { open_ms: 600 }),
  ]);
  assert.deepEqual(summary, { "10k": { open_ms: 40, typing_p95_ms: 2 }, "100k": { open_ms: 500 } });
  assert.equal(median([3, 1, 2, 4]), 2.5);
});

test("a budget fails when exceeded or when its metric was not measured", () => {
  const limits = { "10k": { open_ms: 50, idle_cpu_percent: 0.5 } };
  assert.deepEqual(overBudget({ "10k": { open_ms: 50, idle_cpu_percent: 0.1 } }, limits), []);
  assert.deepEqual(overBudget({ "10k": { open_ms: 51 } }, limits), ["10k open_ms: 51 over 50", "10k idle_cpu_percent: not measured"]);
});

test("a regression needs both the ratio and the floor", () => {
  const base = { "10k": { typing_p95_ms: 1, open_ms: 300 }, "100k": { refresh_ms: 3000 } };
  // 1 ms → 1.6 ms is +60% but under the 3 ms floor; 300 → 340 is +13%.
  assert.deepEqual(regressions({ "10k": { typing_p95_ms: 1.6, open_ms: 340 }, "100k": { refresh_ms: 3500 } }, base), []);
  assert.deepEqual(regressions({ "10k": { typing_p95_ms: 5, open_ms: 300 }, "100k": { refresh_ms: 3700 } }, base),
    ["10k typing_p95_ms: 1 → 5 (+400%)", "100k refresh_ms: 3000 → 3700 (+23%)"]);
});

test("every machine class budgets both cases", () => {
  for (const [name, limits] of Object.entries(budgets)) {
    assert.deepEqual(Object.keys(limits), cases.map((test) => test.replace("feel_", "")), name);
  }
});
