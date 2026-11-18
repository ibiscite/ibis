import assert from "node:assert/strict";
import test from "node:test";
import { budgets, median, missingBudgets, overBudget, regressions, summarize } from "./perf.mjs";

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

test("a relative slowdown must repeat on the same metric", () => {
  const base = { "10k": { open_ms: 250, refresh_ms: 400 } };
  const head = { "10k": { open_ms: 315, refresh_ms: 510 } };
  const confirmation = { base, head: { "10k": { open_ms: 254, refresh_ms: 505 } } };
  assert.deepEqual(regressions(head, base, confirmation), ["10k refresh_ms: 400 → 510 (+28%)"]);
});

test("only complete machine budgets qualify as performance checks", () => {
  const felt = { typing_p95_ms: 50, first_search_ms: 100, first_cite_ms: 100, format_p50_ms: 100 };
  assert.deepEqual(missingBudgets(budgets.shared), []);
  assert.deepEqual(missingBudgets(budgets.windows), []);
  assert.ok(missingBudgets({ "10k": { rss_mib: 65 }, "100k": { rss_mib: 520 } }).includes("100k open_ms"));
  assert.ok(missingBudgets(null).includes("10k typing_p95_ms"));
  for (const metrics of Object.values(budgets.windows)) {
    for (const [metric, ceiling] of Object.entries(felt)) assert.ok(metrics[metric] <= ceiling, `windows ${metric}`);
  }
});
