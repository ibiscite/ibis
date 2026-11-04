import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

// Run the real landing script without a repository, network or build. Only its
// Git and GitHub boundaries are replaced; every command is recorded in order.
function fixture(t) {
  const dir = mkdtempSync(join(tmpdir(), "ibis-land-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const bin = join(dir, "bin"), calls = join(dir, "calls");
  mkdirSync(bin);
  const stubs = {
    git: `printf 'git %s\\n' "$*" >> "$CALLS"
case "$1" in
  rev-parse) if [ "$2" = --abbrev-ref ]; then echo "$BRANCH"; else pwd; fi ;;
  diff) echo README.md ;;
  push) exit "$PUSH_STATUS" ;;
esac`,
    gh: `printf 'gh %s\\n' "$*" >> "$CALLS"
if [ "$1" = api ] && [ "$2" != -X ]; then
  [ "$LOOKUP_STATUS" = 0 ] || exit "$LOOKUP_STATUS"
  echo "$PR_NUMBER"
elif [ "$*" = 'pr merge 42 --squash --auto' ]; then exit "$MERGE_STATUS"
else echo 'unsupported GitHub command' >&2; exit 44
fi`,
  };
  for (const [name, body] of Object.entries(stubs)) writeFileSync(join(bin, name), `#!/usr/bin/env bash\n${body}\n`, { mode: 0o755 });
  return (overrides = {}) => {
    writeFileSync(calls, "");
    const result = spawnSync("bash", [fileURLToPath(new URL("./land.sh", import.meta.url))], {
      cwd: dir, encoding: "utf8", timeout: 5000,
      env: { ...process.env, PATH: `${bin}${delimiter}${process.env.PATH}`, CALLS: calls, BRANCH: "fix/synthetic", PUSH_STATUS: "0", PR_NUMBER: "42", LOOKUP_STATUS: "0", MERGE_STATUS: "0", ...overrides },
    });
    return { ...result, calls: readFileSync(calls, "utf8").trim().split("\n") };
  };
}

test("landing pushes before requesting squash auto-merge through gh", { skip: process.platform === "win32" }, (t) => {
  const run = fixture(t);
  // A second invocation uses the same safe command: no approval or bypass.
  for (let attempt = 0; attempt < 2; attempt++) {
    const result = run();
    assert.equal(result.status, 0, result.stderr);
    assert.ok(result.calls.indexOf("git push -q -u origin fix/synthetic") < result.calls.indexOf("gh pr merge 42 --squash --auto"));
    assert.equal(result.calls.at(-1), "gh pr merge 42 --squash --auto");
    assert.match(result.stdout, /#42 merges itself on a green CI result/);
  }
});

test("landing refuses main and never requests a merge after a failed push", { skip: process.platform === "win32" }, (t) => {
  const run = fixture(t);
  const main = run({ BRANCH: "main" });
  assert.equal(main.status, 2);
  assert.equal(main.calls.length, 1);
  const failed = run({ PUSH_STATUS: "73" });
  assert.equal(failed.status, 73);
  assert.ok(!failed.calls.some(call => call.startsWith("gh ")));
});

test("landing preserves GitHub failures and explains a branch without a PR", { skip: process.platform === "win32" }, (t) => {
  const run = fixture(t);
  const failed = run({ MERGE_STATUS: "74" });
  assert.equal(failed.status, 74);
  assert.doesNotMatch(failed.stdout, /merges itself/);
  const unavailable = run({ LOOKUP_STATUS: "75" });
  assert.equal(unavailable.status, 75);
  assert.doesNotMatch(unavailable.stdout, /Open its pull request|merges itself/);
  for (const number of ["", "null"]) {
    const missing = run({ PR_NUMBER: number });
    assert.equal(missing.status, 0, missing.stderr);
    assert.match(missing.stdout, /Open its pull request/);
    assert.ok(!missing.calls.some(call => call.startsWith("gh pr merge")));
  }
});
