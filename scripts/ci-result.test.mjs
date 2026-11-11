import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { classify } from "./ci-paths.mjs";

const script = fileURLToPath(new URL("./ci-result.sh", import.meta.url));
const check = (overrides, expected) => {
  const result = spawnSync("bash", [script], { encoding: "utf8", timeout: 5000,
    env: { ...process.env, EVENT: "pull_request", BASE: "main", WEB: "true", INTERFACE: "true", NATIVE: "true", CORE: "true", RESULTS: "success success success success", TEST: "success", WEBKIT: "skipped", QUALIFICATION: "skipped skipped skipped", ...overrides } });
  assert.equal(result.status, expected, `${JSON.stringify(overrides)}: ${result.stdout}${result.stderr}`);
  if (expected !== 0) assert.match(result.stdout, /::error::/);
};

test("CI result requires every selected check and only permits intentional skips", () => {
  for (const event of ["pull_request", "push"]) for (const filename of ["README.md", "website/index.html", "apps/desktop/src/App.tsx", "apps/desktop/src-tauri/src/lib.rs", "crates/ibis-core/src/lib.rs"]) {
    const selected = classify(event, 1, [{ filename, status: "modified" }]);
    const env = Object.fromEntries(Object.entries(selected).map(([key, value]) => [key.toUpperCase(), String(value)]));
    check({ ...env, EVENT: event, TEST: selected.core ? "success" : "skipped" }, 0);
  }
  check({ BASE: "feature" }, 1);
  for (const name of ["WEB", "INTERFACE", "NATIVE", "CORE"]) for (const value of ["", "unknown"]) check({ [name]: value }, 1);
  for (const state of ["failure", "cancelled", "skipped", ""]) {
    for (let i = 0; i < 4; i++) {
      const results = ["success", "success", "success", "success"]; results[i] = state;
      check({ RESULTS: results.join(" ") }, 1);
    }
    check({ TEST: state }, 1);
  }
  check({ RESULTS: "success success success success success" }, 1);
  check({ WEBKIT: "success" }, 1);
});

test("daily and manual qualification cannot turn failure or unavailable coverage into a green result", () => {
  for (const EVENT of ["schedule", "workflow_dispatch"]) {
    const complete = { EVENT, WEBKIT: "success", QUALIFICATION: "success success success" };
    check(complete, 0);
    check({ ...complete, CORE: "false", TEST: "skipped" }, 1);
    for (const state of ["failure", "cancelled", "skipped", "unavailable", ""]) {
      check({ ...complete, WEBKIT: state }, 1);
      for (let i = 0; i < 3; i++) { const results = ["success", "success", "success"]; results[i] = state; check({ ...complete, QUALIFICATION: results.join(" ") }, 1); }
    }
  }
});
