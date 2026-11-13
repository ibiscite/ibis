import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), "ibis-land-")), bin = join(root, "bin"), calls = join(root, "calls");
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(bin);
  for (const [name, body] of Object.entries({
    git: `case "$1" in
      rev-parse) case "$2" in --abbrev-ref) echo "$BRANCH";; --show-toplevel) pwd;; *) if [ -f "$ROOT/late" ]; then echo bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb; elif [ -f "$ROOT/checked" ]; then echo "$AFTER_SHA"; else echo "$SHA"; fi;; esac;;
      status) [ "$STATUS_ERROR" = 0 ] || exit "$STATUS_ERROR"; if [ "$DIRTY" = 1 ] || { [ "$DIRTY_AFTER" = 1 ] && [ -f "$ROOT/checked" ]; }; then echo ' M synthetic'; fi;;
      merge-tree) exit "$CONFLICT";;
      show) if [ "$3" = --format=%s ]; then echo 'ci: simplify the pipeline'; else printf 'Synthetic change.\\n\\nCloses #7\\n'; fi;;
      push) exit "$PUSH_STATUS";;
    esac`,
    node: `if [ "$1" = scripts/check.mjs ]; then touch "$ROOT/checked"; exit "$CHECK_STATUS"; fi`,
    gh: `case "$1 $2" in
      'api user') echo "$GH_ACTOR";;
      'pr list') if [ "$LATE_CHANGE" = 1 ]; then touch "$ROOT/late"; fi; if [ "$HAS_PR" = 1 ]; then printf '42\\t%s\\t%s\\n' "$AUTHOR" "$FORK"; fi;;
      'pr create'|'pr edit') while [ "$#" -gt 0 ]; do if [ "$1" = --body-file ]; then cp "$2" "$ROOT/body"; fi; shift; done; echo https://example.invalid/pull/42;;
      'pr view') echo https://example.invalid/pull/42;;
      'pr merge') exit "$MERGE_STATUS";;
    esac`,
  })) writeFileSync(join(bin, name), `#!/usr/bin/env bash\nprintf '${name} %s\\n' "$*" >> "$CALLS"\n${body}\n`, { mode: 0o755 });
  return (overrides = {}) => {
    writeFileSync(calls, ""); rmSync(join(root, "checked"), { force: true }); rmSync(join(root, "late"), { force: true }); rmSync(join(root, "body"), { force: true });
    const result = spawnSync("bash", [fileURLToPath(new URL("./land.sh", import.meta.url))], { cwd: root, encoding: "utf8", timeout: 5000,
      env: { ...process.env, PATH: `${bin}${delimiter}${process.env.PATH}`, ROOT: root, CALLS: calls, BRANCH: "ci/synthetic", SHA: "a".repeat(40), AFTER_SHA: "a".repeat(40), DIRTY: "0", DIRTY_AFTER: "0", LATE_CHANGE: "0", STATUS_ERROR: "0", GH_ACTOR: "diyanko", CONFLICT: "0", CHECK_STATUS: "0", PUSH_STATUS: "0", HAS_PR: "1", AUTHOR: "diyanko", FORK: "false", MERGE_STATUS: "0", ...overrides } });
    let body = ""; try { body = readFileSync(join(root, "body"), "utf8"); } catch {}
    return { ...result, calls: readFileSync(calls, "utf8"), body };
  };
}
const posix = { skip: process.platform === "win32" };

test("landing creates or updates a PR from the checked commit and pins squash auto-merge to it", posix, (t) => {
  const run = fixture(t);
  for (const has of ["0", "1"]) {
    const result = run({ HAS_PR: has });
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.calls, /node scripts\/check.mjs\n/);
    assert.doesNotMatch(result.calls, /cargo |pnpm |--plan/);
    assert.match(result.calls, new RegExp(`gh pr merge .* --squash --auto --match-head-commit ${"a".repeat(40)}`));
    assert.match(result.body, /Synthetic change\.[\s\S]*Closes #7[\s\S]*## Handoff[\s\S]*Local gate passed/);
    assert.match(result.stdout, /Checked SHA: a+.*\n[\s\S]*https:.*\nNext:/);
  }
});

test("dirty files, a moving HEAD, conflicts or incomplete checks prevent publication", posix, (t) => {
  const run = fixture(t);
  for (const override of [{ BRANCH: "main" }, { BRANCH: "HEAD" }, { DIRTY: "1" }, { STATUS_ERROR: "74" }, { DIRTY_AFTER: "1" }, { LATE_CHANGE: "1" }, { AFTER_SHA: "b".repeat(40) }, { CONFLICT: "1" }, ...["1", "2", "75"].map((CHECK_STATUS) => ({ CHECK_STATUS }))]) {
    const result = run(override);
    assert.notEqual(result.status, 0, JSON.stringify(override));
    assert.doesNotMatch(result.calls, /git push|gh pr (edit|create|merge)/);
  }
});

test("failed pushes and other authors or forks never request auto-merge", posix, (t) => {
  const run = fixture(t);
  for (const override of [{ PUSH_STATUS: "73" }, { GH_ACTOR: "someone" }, { AUTHOR: "someone" }, { FORK: "true" }]) {
    const result = run(override);
    assert.notEqual(result.status, 0);
    assert.doesNotMatch(result.calls, /gh pr (create|edit|merge)/);
  }
  const failed = run({ MERGE_STATUS: "74" });
  assert.equal(failed.status, 74);
  assert.doesNotMatch(failed.stdout, /Next:/);
});
