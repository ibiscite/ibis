import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

test("failed daily qualification creates or updates one issue; ordinary runs never post", { skip: process.platform === "win32" }, (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-nightly-report-")), calls = join(dir, "calls"), body = join(dir, "body"), summary = join(dir, "summary");
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  writeFileSync(join(dir, "gh"), '#!/bin/sh\nprintf "%s\\n" "$*" >> "$CALLS"\n[ "$GH_STATUS" = 0 ] || exit "$GH_STATUS"\nif [ "$2" = list ]; then echo "$ISSUE"; fi\nwhile [ "$#" -gt 0 ]; do if [ "$1" = --body-file ]; then cp "$2" "$BODY"; fi; shift; done\n', { mode: 0o755 });
  const run = (env = {}) => {
    writeFileSync(calls, "");
    writeFileSync(summary, "");
    const result = spawnSync("bash", [fileURLToPath(new URL("./ci-nightly-report.sh", import.meta.url))], { encoding: "utf8", env: { ...process.env, PATH: `${dir}${delimiter}${process.env.PATH}`, CALLS: calls, BODY: body, EVENT: "schedule", REF: "refs/heads/main", RESULT: "failure", ISSUE: "", GH_STATUS: "0", GITHUB_REPOSITORY: "example/ibis", RUN_URL: "https://example.invalid/runs/7", GITHUB_STEP_SUMMARY: summary, MACOS_BUDGET: "", ...env } });
    return { ...result, calls: readFileSync(calls, "utf8"), summary: readFileSync(summary, "utf8") };
  };
  assert.match(run().calls, /issue create/);
  assert.match(readFileSync(body, "utf8"), /qualification report.*runs\/7/);
  assert.match(readFileSync(body, "utf8"), /Unavailable.*did not publish a sample/);
  const budget = "255.0 MiB physical footprint across 5 processes (over, 200 MiB budget, footprint). 0.50 idle wake-ups/s.";
  const updated = run({ ISSUE: "42", MACOS_BUDGET: budget });
  assert.match(updated.calls, /issue edit 42/);
  assert.match(updated.calls, /issue reopen 42/);
  assert.doesNotMatch(updated.calls, /issue create|issue comment/);
  assert.ok(readFileSync(body, "utf8").includes(budget));
  const passed = run({ RESULT: "success", MACOS_BUDGET: budget });
  assert.equal(passed.calls, "");
  assert.ok(passed.summary.includes(budget));
  assert.equal(run({ GH_STATUS: "17" }).status, 17);
  for (const env of [{ EVENT: "pull_request" }, { EVENT: "push" }, { REF: "refs/heads/feature" }]) {
    const skipped = run(env);
    assert.equal(skipped.calls, "");
    assert.equal(skipped.summary, "");
  }
});
