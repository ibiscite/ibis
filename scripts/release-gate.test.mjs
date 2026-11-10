import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const SHA = "c".repeat(40);

// The real gate in a stand-in checkout, with only GitHub's API answering
// from the scenario.
function gate(t, env, config = "{}") {
  const dir = mkdtempSync(join(tmpdir(), "ibis-release-gate-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  mkdirSync(join(dir, "bin"));
  writeFileSync(join(dir, "release-please-config.json"), config);
  writeFileSync(join(dir, "bin/gh"), `#!/usr/bin/env bash
case "$*" in
  *graphql*) [ "$PENDING_STATUS" = 0 ] || exit 1; printf '%b' "$PENDING" ;;
  *event=merge_group*head_sha=${SHA}*) [ "$RUNS_STATUS" = 0 ] || exit 1; echo "$CHECKED" ;;
  *) exit 44 ;;
esac
`, { mode: 0o755 });
  return spawnSync("bash", [fileURLToPath(new URL("./release-gate.sh", import.meta.url))], {
    cwd: dir, encoding: "utf8", timeout: 5000,
    env: { ...process.env, PATH: `${join(dir, "bin")}${delimiter}${process.env.PATH}`, REPO: "example/ibis", PENDING_STATUS: "0", RUNS_STATUS: "0", CHECKED: "false", ...env },
  });
}

const pending = { PENDING: `1 1\\n1779 ${SHA}\\n` };

test("a pending release is released only when the merge queue checked its merged commit", { skip: process.platform === "win32" }, (t) => {
  assert.equal(gate(t, { ...pending, CHECKED: "true" }).status, 0);
  const refused = gate(t, { ...pending, CHECKED: "false" });
  assert.equal(refused.status, 1);
  assert.match(refused.stdout, /::error::Release #1779 .*nothing is released/);
  // Unreadable or incomplete evidence never releases.
  for (const env of [{ RUNS_STATUS: "1" }, { CHECKED: "" }, { PENDING_STATUS: "1" }, { PENDING: "2 1\\n1779 " + SHA + "\\n" },
    { PENDING: "" }, { PENDING: "1 1\\n1779 not-a-commit\\n" }, { PENDING: "1 1\\n1779 \\n" }]) {
    assert.notEqual(gate(t, { ...pending, CHECKED: "true", ...env }).status, 0, JSON.stringify(env));
  }
});

test("with no pending release, the push goes on to update the release pull request", { skip: process.platform === "win32" }, (t) => {
  assert.equal(gate(t, { PENDING: "0 0\\n" }).status, 0);
});

test("a label of its own in the config stops the gate until it knows that label", { skip: process.platform === "win32" }, (t) => {
  const refused = gate(t, { PENDING: "0 0\\n" }, JSON.stringify({ label: "release: pending" }));
  assert.equal(refused.status, 1);
  assert.match(refused.stdout, /sets its own label/);
});
