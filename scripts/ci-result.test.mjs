import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("./ci-result.sh", import.meta.url));
const check = (env, expected) => {
  const result = spawnSync("bash", [script], { encoding: "utf8", timeout: 5000,
    env: { ...process.env, EVENT: "push", RESULTS: Array(5).fill("success").join(" "), QUALIFICATION: "skipped skipped skipped skipped", ...env } });
  assert.equal(result.status, expected, `${JSON.stringify(env)}: ${result.stdout}${result.stderr}`);
};

test("every job must pass; daily qualification cannot skip or fail", () => {
  for (const EVENT of ["push", "pull_request", "schedule", "workflow_dispatch"]) {
    const daily = ["schedule", "workflow_dispatch"].includes(EVENT);
    const env = { EVENT, QUALIFICATION: !daily ? "skipped skipped skipped skipped" : "success success success success" };
    check(env, 0);
    for (const state of ["failure", "cancelled", "skipped", "unavailable", ""]) {
      for (let i = 0; i < 5; i++) {
        const results = Array(5).fill("success"); results[i] = state;
        check({ ...env, RESULTS: results.join(" ") }, 1);
      }
      if (daily) for (let i = 0; i < 4; i++) {
        const results = Array(4).fill("success"); results[i] = state;
        check({ ...env, QUALIFICATION: results.join(" ") }, 1);
      }
    }
  }
  check({ EVENT: "merge_group" }, 1);
  check({ RESULTS: Array(6).fill("success").join(" ") }, 1);
});
