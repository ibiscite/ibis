import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("./ci-result.sh", import.meta.url));
const check = (env, expected) => {
  const result = spawnSync("bash", [script], { encoding: "utf8", timeout: 5000,
    env: { ...process.env, EVENT: "push", RESULTS: "success success success success success", QUALIFICATION: "skipped skipped skipped skipped", ...env } });
  assert.equal(result.status, expected, `${JSON.stringify(env)}: ${result.stdout}${result.stderr}`);
};

test("confirmation requires all three-system suites; daily qualification cannot skip or fail", () => {
  for (const EVENT of ["push", "schedule", "workflow_dispatch"]) {
    const env = { EVENT, QUALIFICATION: EVENT === "push" ? "skipped skipped skipped skipped" : "success success success success" };
    check(env, 0);
    for (const state of ["failure", "cancelled", "skipped", "unavailable", ""]) {
      for (let i = 0; i < 5; i++) {
        const results = Array(5).fill("success"); results[i] = state;
        check({ ...env, RESULTS: results.join(" ") }, 1);
      }
      if (EVENT !== "push") for (let i = 0; i < 4; i++) {
        const results = Array(4).fill("success"); results[i] = state;
        check({ ...env, QUALIFICATION: results.join(" ") }, 1);
      }
    }
  }
  check({ EVENT: "pull_request" }, 1);
  check({ RESULTS: "success success success success success success" }, 1);
});
