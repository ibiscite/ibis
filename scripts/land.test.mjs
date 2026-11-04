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
  rev-parse)
    if [ "$2" = --abbrev-ref ]; then echo "$BRANCH"
    elif [ "$4" = MERGE_HEAD ]; then [ "$MERGE_PENDING" = 1 ]
    else pwd; fi ;;
  diff) echo README.md ;;
  commit) exit "$COMMIT_STATUS" ;;
  push) exit "$PUSH_STATUS" ;;
esac`,
    gh: `printf 'gh %s\\n' "$*" >> "$CALLS"
if [ "$1" = api ] && [ "$2" != -X ]; then
  [ "$LOOKUP_STATUS" = 0 ] || exit "$LOOKUP_STATUS"
  echo "$PR_NUMBER"
elif [ "$*" = 'pr merge 42 --squash --auto' ]; then exit "$MERGE_STATUS"
else echo 'unsupported GitHub command' >&2; exit 44
fi`,
    node: `printf 'node %s\\n' "$*" >> "$CALLS"
[ "$*" = scripts/check-agent-identity.mjs ] || exit 99
exit "$IDENTITY_STATUS"`,
  };
  for (const [name, body] of Object.entries(stubs)) writeFileSync(join(bin, name), `#!/usr/bin/env bash\n${body}\n`, { mode: 0o755 });
  return (overrides = {}) => {
    writeFileSync(calls, "");
    const result = spawnSync("bash", [fileURLToPath(new URL("./land.sh", import.meta.url))], {
      cwd: dir, encoding: "utf8", timeout: 5000,
      env: { ...process.env, PATH: `${bin}${delimiter}${process.env.PATH}`, CALLS: calls, BRANCH: "fix/synthetic", MERGE_PENDING: "0", COMMIT_STATUS: "0", IDENTITY_STATUS: "0", PUSH_STATUS: "0", PR_NUMBER: "42", LOOKUP_STATUS: "0", MERGE_STATUS: "0", ...overrides },
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

test("landing refuses main or detached HEAD and never requests a merge after a failed push", { skip: process.platform === "win32" }, (t) => {
  const run = fixture(t);
  for (const branch of ["main", "HEAD"]) {
    const refused = run({ BRANCH: branch });
    assert.equal(refused.status, 2);
    assert.equal(refused.calls.length, 1);
  }
  const failed = run({ PUSH_STATUS: "73" });
  assert.equal(failed.status, 73);
  assert.ok(!failed.calls.some(call => call.startsWith("gh ")));
});

test("a main integration is signed off and disclosed before push; a failed commit stops landing", { skip: process.platform === "win32" }, (t) => {
  const run = fixture(t);
  const integrated = run({ MERGE_PENDING: "1" });
  assert.equal(integrated.status, 0, integrated.stderr);
  const commit = integrated.calls.findIndex(call => call.startsWith("git commit -s --no-edit --trailer Assisted-by:"));
  assert.ok(commit > 0, "the integration has a sign-off and assistance trailer");
  assert.ok(commit < integrated.calls.indexOf("git push -q -u origin fix/synthetic"));
  const unchanged = run();
  assert.equal(unchanged.status, 0, unchanged.stderr);
  assert.ok(!unchanged.calls.some(call => call.startsWith("git commit")));
  const failed = run({ MERGE_PENDING: "1", COMMIT_STATUS: "76" });
  assert.equal(failed.status, 76);
  assert.ok(!failed.calls.some(call => call.startsWith("git push") || call.startsWith("gh ")));
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

test("a rejected public commit identity stops before publishing anything", { skip: process.platform === "win32" }, (t) => {
  const result = fixture(t)({ IDENTITY_STATUS: "77" });
  assert.equal(result.status, 77);
  assert.ok(!result.calls.some(call => call.startsWith("git push") || call.startsWith("gh ")));
});

test("a real Git integration preserves both changes and publishes signed, disclosed metadata once", { skip: process.platform === "win32" }, (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-land-git-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const repo = join(root, "repo"), origin = join(root, "origin.git"), bin = join(root, "bin");
  mkdirSync(repo); mkdirSync(bin);
  // Inherited Git source/config overrides must never redirect fixture commands
  // into the caller's repository, identity, hooks or credential helpers.
  const env = { ...Object.fromEntries(Object.entries(process.env).filter(([key]) => !/^GIT(HUB)?_/.test(key))), GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_NOSYSTEM: "1" };
  const git = (...args) => {
    const result = spawnSync("git", args, { cwd: repo, env, encoding: "utf8", timeout: 10_000 });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim();
  };
  git("init", "-b", "main");
  git("config", "user.name", "Synthetic Maintainer");
  git("config", "user.email", "noreply@github.com");
  writeFileSync(join(repo, "README.md"), "initial main\n");
  git("add", "."); git("commit", "-m", "initial fixture");
  git("clone", "--bare", repo, origin);
  git("remote", "add", "origin", origin);
  git("switch", "-c", "fix/synthetic");
  writeFileSync(join(repo, "feature.md"), "branch work\n");
  git("add", "."); git("commit", "-m", "fixture branch");
  git("switch", "main");
  writeFileSync(join(repo, "README.md"), "upstream main\n");
  git("commit", "-am", "fixture upstream");
  git("--git-dir", origin, "fetch", repo, "main:main");
  git("switch", "fix/synthetic");
  mkdirSync(join(repo, "scripts"));
  writeFileSync(join(repo, "scripts/check-agent-identity.mjs"), readFileSync(new URL("./check-agent-identity.mjs", import.meta.url)));
  writeFileSync(join(bin, "gh"), "#!/bin/sh\nif [ \"$1\" = api ]; then echo 42; else [ \"$*\" = 'pr merge 42 --squash --auto' ]; fi\n", { mode: 0o755 });
  const land = () => {
    const result = spawnSync("bash", [fileURLToPath(new URL("./land.sh", import.meta.url))], {
      cwd: repo, encoding: "utf8", timeout: 10_000,
      env: { ...env, PATH: `${bin}${delimiter}${env.PATH}` },
    });
    assert.equal(result.status, 0, result.stderr);
  };
  land();
  const head = git("rev-parse", "HEAD");
  assert.equal(git("show", "-s", "--format=%P").split(" ").length, 2);
  const message = git("log", "-1", "--format=%B");
  assert.match(message, /^Signed-off-by: Synthetic Maintainer <noreply@github\.com>$/m);
  assert.match(message, /^Assisted-by: .+AI-assisted.+$/m);
  assert.equal(readFileSync(join(repo, "README.md"), "utf8"), "upstream main\n");
  assert.equal(readFileSync(join(repo, "feature.md"), "utf8"), "branch work\n");
  assert.equal(git("--git-dir", origin, "rev-parse", "refs/heads/fix/synthetic"), head);
  land();
  assert.equal(git("rev-parse", "HEAD"), head, "an up-to-date branch gains no empty commit");
});
