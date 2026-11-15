import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

test("native journeys report passed, failed or unavailable and preserve refusal status", { skip: process.platform === "win32" }, (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-journey-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const [status, word] of [[0, "passed"], [1, "failed"], [2, "unavailable"]]) {
    const summary = join(root, `summary-${status}`);
    const result = spawnSync("bash", [fileURLToPath(new URL("./ci-native-journey.sh", import.meta.url)), "bash", "-c", `exit ${status}`], { encoding: "utf8", env: { ...process.env, GITHUB_STEP_SUMMARY: summary } });
    assert.equal(result.status, status);
    assert.match(result.stdout, new RegExp(`Native journey: ${word}`));
    assert.match(readFileSync(summary, "utf8"), new RegExp(`\\*\\*${word}\\*\\*`));
  }
});
