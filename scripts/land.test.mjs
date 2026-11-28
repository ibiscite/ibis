import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const sha = "a".repeat(40), posix = { skip: process.platform === "win32" };
function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), "ibis-land-")), bin = join(root, "bin"), calls = join(root, "calls");
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const path of ["bin", "scripts", "checkout/scripts"]) mkdirSync(join(root, path), { recursive: true });
  copyFileSync(fileURLToPath(new URL("./pr-lint.mjs", import.meta.url)), join(root, "scripts/pr-lint.mjs"));
  copyFileSync(fileURLToPath(new URL("./pr-lint.mjs", import.meta.url)), join(root, "checkout/scripts/pr-lint.mjs"));
  writeFileSync(join(root, "supplied-body"), "Caller supplied body.\n");
  writeFileSync(join(root, "checkout/scripts/check.sh"), `printf 'check %s\\n' "$*" >> "$CALLS"
if [ "$MOVE_HEAD" = 1 ]; then touch "$ROOT/changed"; fi
[ "$FAIL_CHECK" != check ]
`);
  for (const [name, body] of Object.entries({
    git: `if [ "$1" = -C ]; then shift 2; fi
    case "$1" in
      rev-parse) case "$2" in --abbrev-ref) echo "$BRANCH";; --show-toplevel) echo "$ROOT/checkout";; *) if [ -f "$ROOT/changed" ]; then echo bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb; else echo "$SHA"; fi;; esac;;
      status) [ "$STATUS_ERROR" = 0 ] || exit "$STATUS_ERROR"; if [ "$DIRTY" = 1 ]; then echo ' M synthetic'; fi;;
      rev-list) echo "$SHA";;
      show) if [ "$3" = --format=%s ]; then echo 'ci: simplify the pipeline'; else printf 'Synthetic change.\\n\\nCloses #7\\n\\nSigned-off-by: Synthetic <synthetic@example.org>\\nAssisted-by: Codex\\n'; fi;;
      interpret-trailers) exec /usr/bin/git "$@";;
      log) printf 'Signed-off-by: Synthetic <synthetic@example.org>\\nAssisted-by: Codex\\n';;
      show-ref) [ "$BRANCH_EXISTS" = 1 ];;
      worktree) if [ "$2" = add ]; then mkdir -p "$5"; else rm -rf "$3"; fi;;
      push) exit "$PUSH_STATUS";;
    esac`,
    cargo: `[ "$*" != "$FAIL_CHECK" ]`,
    pnpm: `[ "$*" != "$FAIL_CHECK" ]`,
    node: `case "$1" in
      *pr-lint.mjs) exec "$REAL_NODE" "$@";;
    esac
    [ "$*" != "$FAIL_CHECK" ]`,
    gh: `case "$1 $2" in
      'api user') echo "$GH_ACTOR";;
      'repo view') echo example/ibis;;
      'api repos/example/ibis/actions/workflows/ci.yml/runs?branch=main&event=push&status=completed&per_page=1') case "$MAIN_CI" in failure) echo https://example.invalid/actions/runs/9;; error) exit 1;; esac;;
      'pr list') if [[ "$*" == *'--state merged'* ]]; then [ "$MERGED" = 0 ] || echo "$SHA"; elif [ "$HAS_PR" = 1 ]; then printf '42\\t%s\\t%s\\t%s\\n' "$AUTHOR" "$FORK" "$DRAFT"; fi;;
      'pr view') case "$*" in *--json\\ title*) echo "$PR_TITLE";; *--json\\ body*) printf 'Reviewed body.\\n\\n## Handoff\\nExisting instructions.\\n';; *--json\\ headRefOid*) echo "$PR_SHA";; *) echo https://example.invalid/pull/42;; esac;;
      'pr create'|'pr edit') while [ "$#" -gt 0 ]; do if [ "$1" = --body-file ]; then cp "$2" "$ROOT/body"; fi; shift; done; echo https://example.invalid/pull/42;;
      'pr ready') exit "$READY_STATUS";;
      'pr merge') while [ "$#" -gt 0 ]; do if [ "$1" = --body-file ]; then cp "$2" "$ROOT/merge-body"; fi; shift; done; exit "$MERGE_STATUS";;
    esac`,
  })) writeFileSync(join(bin, name), `#!/usr/bin/env bash\nprintf '${name} %s\\n' "$*" >> "$CALLS"\n${body}\n`, { mode: 0o755 });
  return (overrides = {}, args = []) => {
    for (const path of ["changed", "body", "merge-body"]) rmSync(join(root, path), { force: true });
    writeFileSync(calls, "");
    const result = spawnSync("bash", [fileURLToPath(new URL("./land.sh", import.meta.url)), ...args], { cwd: root, encoding: "utf8", timeout: 15000,
      env: { ...process.env, PATH: `${bin}${delimiter}${process.env.PATH}`, ROOT: root, CALLS: calls, REAL_NODE: process.execPath, BRANCH: "ci/synthetic", BRANCH_EXISTS: "1", SHA: sha, PR_SHA: sha, PR_TITLE: "fix: preserve the reviewed title", DIRTY: "0", MOVE_HEAD: "0", STATUS_ERROR: "0", GH_ACTOR: "diyanko", PUSH_STATUS: "0", HAS_PR: "1", MERGED: "1", AUTHOR: "diyanko", FORK: "false", DRAFT: "false", READY_STATUS: "0", MERGE_STATUS: "0", FAIL_CHECK: "", MAIN_CI: "success", ...overrides } });
    let body = ""; try { body = readFileSync(join(root, "body"), "utf8"); } catch {}
    let mergeBody = ""; try { mergeBody = readFileSync(join(root, "merge-body"), "utf8"); } catch {}
    return { ...result, calls: readFileSync(calls, "utf8"), body, mergeBody };
  };
}

test("CI's local checks precede push and a new PR requests hosted auto-merge", posix, (t) => {
  const result = fixture(t)({ HAS_PR: "0" });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.ok(result.calls.includes("check \n"), result.calls);
  assert.ok(result.calls.indexOf("check \n") < result.calls.indexOf("git push"));
  assert.match(result.calls, new RegExp(`git push -q -u origin ${sha}:refs/heads/ci/synthetic`));
  assert.match(result.calls, new RegExp(`gh pr merge .*--auto --squash --body-file .* --match-head-commit ${sha}`));
  assert.doesNotMatch(result.calls, /ssh |statuses\/|--admin|--delete-branch|cargo (clippy|test|deny)|vitest|pnpm .* install/);
  assert.match(result.body, /Synthetic change\.[\s\S]*Closes #7/);
  assert.match(result.mergeBody, /Signed-off-by: Synthetic <synthetic@example.org>\nAssisted-by: Codex/);
  assert.equal(result.mergeBody.match(/Signed-off-by:/g).length, 1);
  assert.match(result.stdout, /Auto-merge requested/);
});

test("invalid titles fail before checks or push start", posix, (t) => {
  const run = fixture(t);
  for (const [env, args] of [[{ PR_TITLE: `fix: ${"a".repeat(68)}` }, []], [{ HAS_PR: "0" }, ["--title", `fix: ${"a".repeat(68)}`]]]) {
    const result = run(env, args);
    assert.notEqual(result.status, 0);
    assert.match(result.stdout, /73 characters/);
    assert.doesNotMatch(result.calls, /cargo |git push/);
  }
});

test("start claims a branch in a worktree and end removes only a clean merged head", posix, (t) => {
  const run = fixture(t);
  const started = run({}, ["start", "fix-shelf", "7"]);
  assert.equal(started.status, 0, started.stdout + started.stderr);
  assert.match(started.calls, /worktree add -b fix-shelf .*\/ibis-work\/fix-shelf origin\/main/);
  assert.match(started.calls, /pnpm -C apps\/desktop install --frozen-lockfile --offline/);
  assert.match(started.calls, /commit --allow-empty -s -m chore: fix shelf/);
  assert.match(started.calls, /pr create --draft .*--title chore: fix shelf --body Refs #7/);
  for (const env of [{ DIRTY: "1" }, { MERGED: "0" }, { BRANCH: "other" }]) {
    const result = run({ BRANCH: "fix-shelf", ...env }, ["end", "fix-shelf"]);
    assert.notEqual(result.status, 0);
    assert.doesNotMatch(result.calls, /worktree remove|branch -D/);
  }
  const ended = run({ BRANCH: "fix-shelf" }, ["end", "fix-shelf"]);
  assert.equal(ended.status, 0, ended.stdout + ended.stderr);
  assert.match(ended.calls, /worktree remove .*\/ibis-work\/fix-shelf/);
  assert.match(ended.calls, /branch -D fix-shelf/);
  assert.equal(run({}, ["start", "fix-shelf"]).status, 0);
  const alreadyDeleted = run({ BRANCH: "main", BRANCH_EXISTS: "0" }, ["end", "fix-shelf"]);
  assert.equal(alreadyDeleted.status, 0, alreadyDeleted.stdout + alreadyDeleted.stderr);
  assert.doesNotMatch(alreadyDeleted.calls, /branch -D/);
  assert.notEqual(run({}, ["start", "../bad"]).status, 0);
});

test("each failed local check stops before push or PR changes", posix, (t) => {
  const run = fixture(t);
  const result = run({ FAIL_CHECK: "check" });
  assert.notEqual(result.status, 0, result.stdout + result.stderr);
  assert.doesNotMatch(result.calls, /git push|gh pr (create|edit|ready|merge)/);
  assert.equal(run({}, ["--skip", "mac"]).status, 2);
});

test("existing PR text survives caller overrides; drafts become ready for auto-merge", posix, (t) => {
  const run = fixture(t);
  for (const author of ["diyanko", "ibis-release[bot]"]) {
    const result = run({ AUTHOR: author, DRAFT: "true" }, ["--title", "ci: caller title", "--body-file", "supplied-body"]);
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.match(result.mergeBody, /^Reviewed body\.\n\n## Handoff\nExisting instructions\./);
    assert.match(result.mergeBody, /Signed-off-by: Synthetic <synthetic@example.org>\nAssisted-by: Codex/);
    assert.match(result.calls, /gh pr ready 42/);
    assert.match(result.calls, /pr-lint.mjs --title fix: preserve the reviewed title/);
    assert.doesNotMatch(result.calls, /gh pr (create|edit)/);
    assert.doesNotMatch(result.mergeBody, /Caller supplied body/);
  }
  const created = run({ HAS_PR: "0" }, ["--title", "ci: caller title", "--body-file", "supplied-body"]);
  assert.equal(created.status, 0, created.stdout + created.stderr);
  assert.match(created.body, /^Caller supplied body\./);
  assert.match(created.calls, /pr create .*--title ci: caller title/);
});

test("dirty files, moving heads, wrong owners and failed push refuse auto-merge", posix, (t) => {
  const run = fixture(t);
  for (const env of [{ BRANCH: "main" }, { BRANCH: "HEAD" }, { DIRTY: "1" }, { STATUS_ERROR: "74" }, { MOVE_HEAD: "1" }, { PUSH_STATUS: "73" }, { GH_ACTOR: "someone" }, { AUTHOR: "someone" }, { FORK: "true" }, { PR_SHA: "b".repeat(40) }]) {
    const result = run(env);
    assert.notEqual(result.status, 0, JSON.stringify(env) + result.stdout + result.stderr);
    assert.doesNotMatch(result.calls, /gh pr merge/);
  }
  assert.doesNotMatch(run({ DRAFT: "true", READY_STATUS: "74" }).calls, /gh pr merge/);
  assert.equal(run({ MERGE_STATUS: "74" }).status, 74);
});

test("a red main lands only fixes and reverts; an unknown state does not block", posix, (t) => {
  const run = fixture(t);
  for (const PR_TITLE of ["fix: preserve the reviewed title", "fix(ci)!: restore main", "revert: undo the shelf"]) {
    const result = run({ MAIN_CI: "failure", PR_TITLE });
    assert.equal(result.status, 0, PR_TITLE + result.stdout + result.stderr);
  }
  const refused = run({ MAIN_CI: "failure", PR_TITLE: "feat: add a shelf" });
  assert.equal(refused.status, 1);
  assert.match(refused.stdout, /Main is red \(https:\/\/example\.invalid\/actions\/runs\/9\)/);
  assert.doesNotMatch(refused.calls, /cargo |git push|gh pr (create|ready|merge)/);
  for (const MAIN_CI of ["success", "error"]) assert.equal(run({ MAIN_CI, PR_TITLE: "feat: add a shelf" }).status, 0, MAIN_CI);
});
