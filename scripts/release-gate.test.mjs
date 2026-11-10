import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

// The real gate, with only GitHub's API answering from the scenario.
function gate(t, env) {
  const dir = mkdtempSync(join(tmpdir(), "ibis-release-gate-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  writeFileSync(join(dir, "gh"), `#!/usr/bin/env bash
case "$2" in
  */commits/*) [ "$FILES_STATUS" = 0 ] || exit 1; printf '%b\\n' "$FILES" ;;
  *event=merge_group*head_sha=$SHA*) [ "$RUNS_STATUS" = 0 ] || exit 1; echo "$CHECKED" ;;
  *) exit 44 ;;
esac
`, { mode: 0o755 });
  return spawnSync("bash", [fileURLToPath(new URL("./release-gate.sh", import.meta.url))], {
    encoding: "utf8", timeout: 5000,
    env: { ...process.env, PATH: `${dir}${delimiter}${process.env.PATH}`, REPO: "example/ibis", SHA: "c".repeat(40), FILES_STATUS: "0", RUNS_STATUS: "0", CHECKED: "false", ...env },
  });
}

const release = "CHANGELOG.md\\n.release-please-manifest.json\\nCargo.toml";

test("a release is tagged only when the merge queue checked that exact commit", { skip: process.platform === "win32" }, (t) => {
  assert.equal(gate(t, { FILES: release, CHECKED: "true" }).status, 0);
  const refused = gate(t, { FILES: release, CHECKED: "false" });
  assert.equal(refused.status, 1);
  assert.match(refused.stdout, /::error::.*not tagged/);
  // Unreadable evidence never releases.
  for (const env of [{ FILES: release, RUNS_STATUS: "1" }, { FILES: release, CHECKED: "" }, { FILES_STATUS: "1" }]) {
    assert.notEqual(gate(t, env).status, 0, JSON.stringify(env));
  }
});

test("any other push to main goes on to update the release pull request", { skip: process.platform === "win32" }, (t) => {
  assert.equal(gate(t, { FILES: "crates/ibis-core/src/lib.rs\\nCargo.lock" }).status, 0);
});
