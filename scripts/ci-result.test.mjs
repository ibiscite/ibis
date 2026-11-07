import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { classify } from "./ci-paths.mjs";

const script = fileURLToPath(new URL("./ci-result.sh", import.meta.url));
const check = (overrides, expected) => {
  const result = spawnSync(script, [], {
    encoding: "utf8", timeout: 5000,
    env: { ...process.env, EVENT: "pull_request", BASE: "main", WEB: "true", INTERFACE: "true", NATIVE: "true", CORE: "true", PROVEN: "false", RESULTS: "success success success success", TEST: "success", WEBKIT: "success", ...overrides },
  });
  assert.equal(result.status, expected, `${JSON.stringify(overrides)}: ${result.stdout}${result.stderr}`);
  if (expected !== 0) assert.match(result.stdout, /::error::/, "a refusal says why");
};

test("CI result accepts only deliberate skips and rejects failures, cancellation and missing decisions", { skip: process.platform === "win32" }, () => {
  check({}, 0);
  for (const status of ["added", "removed"]) {
    const selection = classify("pull_request", 1, [{ filename: "crates/ibis-csl/vendor/README.md", status }]);
    check({ WEB: String(selection.web), INTERFACE: String(selection.interface), NATIVE: String(selection.native), CORE: String(selection.core), TEST: "skipped", WEBKIT: "skipped" }, 1);
  }
  const skipped = { TEST: "skipped", WEBKIT: "skipped" };
  check({ WEB: "false", INTERFACE: "false", NATIVE: "false", CORE: "false", ...skipped }, 0);
  check({ INTERFACE: "false", NATIVE: "false", CORE: "false", ...skipped }, 0); // The website.
  check({ NATIVE: "false", CORE: "false", TEST: "skipped" }, 0); // The interface.
  check({ NATIVE: "false", CORE: "false", ...skipped }, 1); // The interface always reaches WebKit.
  check({ CORE: "false", TEST: "skipped" }, 0); // Changes only inside the app.
  check({ EVENT: "push", BASE: "" }, 0);
  check({ EVENT: "merge_group", BASE: "" }, 0); // A queue run has no base_ref.
  check({ EVENT: "merge_group", BASE: "", CORE: "false", TEST: "skipped" }, 0); // It runs what its entry changes.
  check({ EVENT: "merge_group", BASE: "", WEB: "false", INTERFACE: "false", NATIVE: "false", CORE: "false", ...skipped }, 0);
  for (const event of ["schedule", "workflow_dispatch"]) {
    check({ EVENT: event, BASE: "" }, 0);
    check({ EVENT: event, BASE: "", CORE: "false", TEST: "skipped" }, 1);
  }
  check({ BASE: "feature" }, 1);
  for (const value of ["", "unknown"]) for (const name of ["WEB", "INTERFACE", "NATIVE", "CORE"]) check({ [name]: value }, 1);
  check({ NATIVE: "false" }, 1); // The crates never run without the app.
  check({ INTERFACE: "false", WEBKIT: "skipped" }, 1); // Native changes always reach WebKit.
  check({ WEB: "false", ...skipped }, 1);
  check({ EVENT: "push", WEB: "false", INTERFACE: "false", NATIVE: "false", CORE: "false", ...skipped }, 1);
  check({ EVENT: "push", CORE: "false", TEST: "skipped" }, 1);
  for (const state of ["failure", "cancelled", "skipped", ""]) {
    for (let i = 0; i < 4; i++) {
      const results = ["success", "success", "success", "success"];
      results[i] = state;
      check({ RESULTS: results.join(" ") }, 1);
    }
    check({ TEST: state }, 1);
    check({ WEBKIT: state }, 1);
  }
  for (const state of ["failure", "cancelled", "success", ""]) {
    for (const results of [{ TEST: state }, { WEBKIT: state }]) check({ WEB: "false", INTERFACE: "false", NATIVE: "false", CORE: "false", ...skipped, ...results }, 1);
  }
  check({ RESULTS: "success success success success success" }, 1); // A fifth result means a job was added or renamed.
  check({ PROVEN: "" }, 1);
  check({ PROVEN: "unknown" }, 1);
});

test("only a queue entry or main may reuse a passing run of the same tree, and then nothing else runs", { skip: process.platform === "win32" }, () => {
  const reused = { BASE: "", PROVEN: "true", WEB: "false", INTERFACE: "false", NATIVE: "false", CORE: "false", RESULTS: "success skipped skipped success", TEST: "skipped", WEBKIT: "skipped" };
  for (const event of ["merge_group", "push"]) {
    check({ ...reused, EVENT: event }, 0);
    for (const name of ["WEB", "INTERFACE", "NATIVE", "CORE"]) check({ ...reused, EVENT: event, [name]: "true" }, 1);
    // Dependency policy still runs, and Web and Desktop must not have run.
    for (const results of ["success skipped skipped skipped", "success success skipped success", "success skipped success success", "success skipped skipped failure", "failure skipped skipped success"]) {
      check({ ...reused, EVENT: event, RESULTS: results }, 1);
    }
    for (const name of ["TEST", "WEBKIT"]) check({ ...reused, EVENT: event, [name]: "success" }, 1);
  }
  for (const event of ["pull_request", "schedule", "workflow_dispatch", ""]) check({ ...reused, EVENT: event, BASE: "main" }, 1);
});
