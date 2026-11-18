import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("./ci-result.sh", import.meta.url));
const check = (env, expected) => {
  const result = spawnSync("bash", [script], { encoding: "utf8", timeout: 5000,
    env: { ...process.env, EVENT: "push", RESULTS: Array(5).fill("success").join(" "), SELECTED: "rust:true:success pictures:true:success", WORKSPACE: "true", CSL: "success", QUALIFICATION: "skipped skipped skipped", ...env } });
  assert.equal(result.status, expected, `${JSON.stringify(env)}: ${result.stdout}${result.stderr}`);
};

test("every job must pass; daily qualification cannot skip or fail", () => {
  for (const EVENT of ["push", "pull_request", "schedule", "workflow_dispatch"]) {
    const daily = ["schedule", "workflow_dispatch"].includes(EVENT);
    const env = { EVENT, QUALIFICATION: !daily ? "skipped skipped skipped" : "success success success" };
    check(env, 0);
    for (const state of ["failure", "cancelled", "skipped", "unavailable", ""]) {
      check({ ...env, CSL: state }, 1);
      for (let i = 0; i < 5; i++) {
        const results = Array(5).fill("success"); results[i] = state;
        check({ ...env, RESULTS: results.join(" ") }, 1);
      }
      if (daily) for (let i = 0; i < 3; i++) {
        const results = Array(3).fill("success"); results[i] = state;
        check({ ...env, QUALIFICATION: results.join(" ") }, 1);
      }
    }
  }
  check({ EVENT: "issues" }, 1);
  for (const count of [4, 6]) check({ RESULTS: Array(count).fill("success").join(" ") }, 1);
});

test("a planned job passes only as selected-and-succeeded or not-selected-and-skipped", () => {
  for (const rust of ["true:success", "false:skipped"]) for (const pictures of ["true:success", "false:skipped"]) {
    check({ EVENT: "pull_request", SELECTED: `rust:${rust} pictures:${pictures}`, WORKSPACE: "false" }, 0);
  }
  for (const bad of ["true:skipped", "true:cancelled", "true:failure", "false:success", "false:failure", ":skipped", ":success", "true:", "maybe:success"]) {
    check({ EVENT: "pull_request", SELECTED: `rust:${bad} pictures:true:success` }, 1);
    check({ EVENT: "pull_request", SELECTED: `rust:true:success pictures:${bad}` }, 1);
  }
  for (const SELECTED of ["", "rust:true:success", "rust:true:success pictures:true:success web:true:success", "pictures:true:success rust:true:success", "rust:true:success rust:true:success"]) {
    check({ EVENT: "pull_request", SELECTED }, 1);
  }
});

test("the workspace selection is required and every non-PR event selects the full suite", () => {
  for (const WORKSPACE of ["", "unknown"]) check({ EVENT: "pull_request", WORKSPACE }, 1);
  check({ EVENT: "pull_request", SELECTED: "rust:false:skipped pictures:false:skipped" }, 1);
  for (const EVENT of ["push", "schedule", "workflow_dispatch"]) {
    const env = { EVENT, QUALIFICATION: EVENT === "push" ? "skipped skipped skipped" : "success success success" };
    check({ ...env, WORKSPACE: "false" }, 1);
    check({ ...env, SELECTED: "rust:true:success pictures:false:skipped" }, 1);
  }
});
