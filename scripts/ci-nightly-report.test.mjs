import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

test("failed daily qualification creates or updates one issue; ordinary runs never post", { skip: process.platform === "win32" }, (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-nightly-report-")), calls = join(dir, "calls"), body = join(dir, "body");
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  writeFileSync(join(dir, "gh"), '#!/bin/sh\nprintf "%s\\n" "$*" >> "$CALLS"\n[ "$GH_STATUS" = 0 ] || exit "$GH_STATUS"\nif [ "$2" = list ]; then echo "$ISSUE"; fi\nwhile [ "$#" -gt 0 ]; do if [ "$1" = --body-file ]; then cp "$2" "$BODY"; fi; shift; done\n', { mode: 0o755 });
  const run = (env = {}) => {
    writeFileSync(calls, "");
    const result = spawnSync("bash", [fileURLToPath(new URL("./ci-nightly-report.sh", import.meta.url))], { encoding: "utf8", env: { ...process.env, PATH: `${dir}${delimiter}${process.env.PATH}`, CALLS: calls, BODY: body, EVENT: "schedule", REF: "refs/heads/main", RESULT: "failure", ISSUE: "", GH_STATUS: "0", GITHUB_REPOSITORY: "example/ibis", RUN_URL: "https://example.invalid/runs/7", ...env } });
    return { ...result, calls: readFileSync(calls, "utf8") };
  };
  assert.match(run().calls, /issue create/);
  assert.match(readFileSync(body, "utf8"), /qualification report.*runs\/7/);
  const updated = run({ ISSUE: "42" });
  assert.match(updated.calls, /issue edit 42/);
  assert.match(updated.calls, /issue reopen 42/);
  assert.doesNotMatch(updated.calls, /issue create|issue comment/);
  assert.equal(run({ GH_STATUS: "17" }).status, 17);
  for (const env of [{ EVENT: "pull_request" }, { EVENT: "push" }, { REF: "refs/heads/feature" }, { RESULT: "success" }]) assert.equal(run(env).calls, "");
});
