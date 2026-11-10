import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const SHA = "c".repeat(40);

// The real gate, with only GitHub's API answering from the scenario.
function gate(t, env) {
  const dir = mkdtempSync(join(tmpdir(), "ibis-release-gate-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  writeFileSync(join(dir, "gh"), `#!/usr/bin/env bash
args="$*"
case "$args" in
  *search/issues*'.total_count'*) [ "$SEARCH_STATUS" = 0 ] || exit 1; echo "$SEARCH" ;;
  *search/issues*'.items[].number'*) printf '%b' "$NUMBERS" ;;
  *repos/example/ibis/pulls/*) echo "$MERGED" ;;
  *event=merge_group*head_sha=$MERGED*) [ "$RUNS_STATUS" = 0 ] || exit 1; echo "$CHECKED" ;;
  *) exit 44 ;;
esac
`, { mode: 0o755 });
  return spawnSync("bash", [fileURLToPath(new URL("./release-gate.sh", import.meta.url))], {
    encoding: "utf8", timeout: 5000,
    env: { ...process.env, PATH: `${dir}${delimiter}${process.env.PATH}`, REPO: "example/ibis", SEARCH_STATUS: "0", RUNS_STATUS: "0", MERGED: SHA, CHECKED: "false", ...env },
  });
}

const pending = { SEARCH: "1 false 1", NUMBERS: "1779\\n" };

test("a pending release is released only when the merge queue checked its merged commit", { skip: process.platform === "win32" }, (t) => {
  assert.equal(gate(t, { ...pending, CHECKED: "true" }).status, 0);
  const refused = gate(t, { ...pending, CHECKED: "false" });
  assert.equal(refused.status, 1);
  assert.match(refused.stdout, /::error::Release #1779 .*nothing is released/);
  // Unreadable or incomplete evidence never releases.
  for (const env of [{ RUNS_STATUS: "1" }, { CHECKED: "" }, { MERGED: "" }, { SEARCH_STATUS: "1" }, { SEARCH: "2 false 1" }, { SEARCH: "1 true 1" }, { SEARCH: "" }]) {
    assert.notEqual(gate(t, { ...pending, CHECKED: "true", ...env }).status, 0, JSON.stringify(env));
  }
});

test("with no pending release, the push goes on to update the release pull request", { skip: process.platform === "win32" }, (t) => {
  assert.equal(gate(t, { SEARCH: "0 false 0", NUMBERS: "" }).status, 0);
});
