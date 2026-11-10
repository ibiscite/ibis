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
  mkdirSync(join(dir, "apps/desktop/src-tauri"), { recursive: true });
  // The tests landing may run; a script without one has none here.
  mkdirSync(join(dir, "scripts"));
  for (const name of ["check.test.mjs", "land.test.mjs"]) writeFileSync(join(dir, "scripts", name), "");
  const stubs = {
    git: `printf 'git %s\\n' "$*" >> "$CALLS"
case "$1" in
  rev-parse)
    if [ "$2" = --abbrev-ref ]; then echo "$BRANCH"
    else pwd; fi ;;
  merge-tree) exit "$CONFLICT" ;;
  diff) printf '%s\n' $CHANGED ;;
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
if [ "$*" = "scripts/check.mjs --plan" ]; then echo "$PLAN"; exit "$PLAN_STATUS"; fi
case "$1" in --test) exit 0 ;; esac
[ "$*" = scripts/check-agent-identity.mjs ] || exit 99
exit "$IDENTITY_STATUS"`,
    cargo: `printf 'cargo %s\\n' "$*" >> "$CALLS"`,
    pnpm: `printf 'pnpm %s\\n' "$*" >> "$CALLS"`,
  };
  for (const [name, body] of Object.entries(stubs)) writeFileSync(join(bin, name), `#!/usr/bin/env bash\n${body}\n`, { mode: 0o755 });
  return (overrides = {}) => {
    writeFileSync(calls, "");
    const result = spawnSync("bash", [fileURLToPath(new URL("./land.sh", import.meta.url))], {
      cwd: dir, encoding: "utf8", timeout: 5000,
      env: { ...process.env, PATH: `${bin}${delimiter}${process.env.PATH}`, CALLS: calls, BRANCH: "fix/synthetic", CONFLICT: "0", CHANGED: "README.md", IDENTITY_STATUS: "0", PUSH_STATUS: "0", PR_NUMBER: "42", LOOKUP_STATUS: "0", MERGE_STATUS: "0", PLAN: "", PLAN_STATUS: "0", ...overrides },
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
    assert.match(result.stdout, /#42 joins the merge queue once its CI is green/);
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

test("a conflict with main stops landing before anything is checked or published", { skip: process.platform === "win32" }, (t) => {
  const run = fixture(t);
  const clean = run();
  assert.equal(clean.status, 0, clean.stderr);
  assert.ok(!clean.calls.some(call => /^git (merge|commit) /.test(call)), "main is not merged in when nothing conflicts");
  const conflict = run({ CONFLICT: "1" });
  assert.equal(conflict.status, 1);
  assert.match(conflict.stdout, /Merge conflict with main/);
  assert.ok(!conflict.calls.some(call => /^(git push|gh |cargo |node scripts\/check)/.test(call)));
});

test("only the changed script tests and the tests of changed scripts run", { skip: process.platform === "win32" }, (t) => {
  const run = fixture(t);
  const ran = run({ CHANGED: "scripts/land.sh scripts/check.test.mjs scripts/no-test.sh website/index.html" });
  assert.equal(ran.status, 0, ran.stderr);
  const tests = ran.calls.filter(call => call.startsWith("node --test"));
  assert.deepEqual(tests, ["node --test scripts/check.test.mjs scripts/land.test.mjs"]);
  const none = run({ CHANGED: "website/index.html" });
  assert.ok(!none.calls.some(call => call.startsWith("node --test")));
});

test("landing preserves GitHub failures and explains a branch without a PR", { skip: process.platform === "win32" }, (t) => {
  const run = fixture(t);
  const failed = run({ MERGE_STATUS: "74" });
  assert.equal(failed.status, 74);
  assert.doesNotMatch(failed.stdout, /joins the merge queue/);
  const unavailable = run({ LOOKUP_STATUS: "75" });
  assert.equal(unavailable.status, 75);
  assert.doesNotMatch(unavailable.stdout, /Open its pull request|joins the merge queue/);
  for (const number of ["", "null"]) {
    const missing = run({ PR_NUMBER: number });
    assert.equal(missing.status, 0, missing.stderr);
    assert.match(missing.stdout, /Open its pull request/);
    assert.ok(!missing.calls.some(call => call.startsWith("gh pr merge")));
  }
});

test("landing checks what CI's plan selects and leaves test suites to CI", { skip: process.platform === "win32" }, (t) => {
  const run = fixture(t);
  const native = run({ PLAN: "core interface native scripts web" });
  assert.equal(native.status, 0, native.stderr);
  assert.ok(native.calls.includes("cargo check --workspace --all-targets --locked -q"), "a change that no longer compiles stops here");
  assert.deepEqual(native.calls.filter(call => call.startsWith("cargo fmt")), ["cargo fmt --all --check"]);
  assert.ok(native.calls.includes("pnpm exec tsc --noEmit -p ."));
  assert.ok(!native.calls.some((call) => call.startsWith("cargo test")), "the test suites run in CI");
  const docs = run({ PLAN: "" });
  assert.equal(docs.status, 0, docs.stderr);
  assert.ok(!docs.calls.some((call) => /^(cargo|pnpm) /.test(call)), docs.calls.join("\n"));
  const front = run({ PLAN: "interface web" });
  assert.ok(front.calls.includes("pnpm exec tsc --noEmit -p .") && !front.calls.some((call) => call.startsWith("cargo ")));
  const failed = run({ PLAN_STATUS: "1" });
  assert.equal(failed.status, 1);
  assert.ok(!failed.calls.some((call) => call.startsWith("git push") || call.startsWith("gh ")));
});

test("a rejected public commit identity stops before publishing anything", { skip: process.platform === "win32" }, (t) => {
  const result = fixture(t)({ IDENTITY_STATUS: "77" });
  assert.equal(result.status, 77);
  assert.ok(!result.calls.some(call => call.startsWith("git push") || call.startsWith("gh ")));
});

test("a real Git branch is pushed as it is, and a real conflict stops it", { skip: process.platform === "win32" }, (t) => {
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
  for (const name of ["check-agent-identity.mjs", "check.mjs", "ci-paths.mjs"]) writeFileSync(join(repo, "scripts", name), readFileSync(new URL(`./${name}`, import.meta.url)));
  writeFileSync(join(repo, ".git/info/exclude"), "scripts/\n"); // The landing tools are not the branch's change.
  writeFileSync(join(bin, "gh"), "#!/bin/sh\nif [ \"$1\" = api ]; then echo 42; else [ \"$*\" = 'pr merge 42 --squash --auto' ]; fi\n", { mode: 0o755 });
  const land = () => {
    const result = spawnSync("bash", [fileURLToPath(new URL("./land.sh", import.meta.url))], {
      cwd: repo, encoding: "utf8", timeout: 10_000,
      env: { ...env, PATH: `${bin}${delimiter}${env.PATH}` },
    });
    assert.equal(result.status, 0, result.stderr);
  };
  const branchHead = git("rev-parse", "HEAD");
  land();
  // Nothing conflicts, so main is not merged in: the queue tests the combination.
  assert.equal(git("rev-parse", "HEAD"), branchHead);
  assert.equal(readFileSync(join(repo, "README.md"), "utf8"), "initial main\n");
  assert.equal(git("--git-dir", origin, "rev-parse", "refs/heads/fix/synthetic"), branchHead);
  // The same file changed on both sides stops landing with nothing pushed.
  writeFileSync(join(repo, "README.md"), "branch edit\n");
  git("commit", "-am", "fixture conflicting edit");
  const result = spawnSync("bash", [fileURLToPath(new URL("./land.sh", import.meta.url))], {
    cwd: repo, encoding: "utf8", timeout: 10_000, env: { ...env, PATH: `${bin}${delimiter}${env.PATH}` },
  });
  assert.equal(result.status, 1, result.stdout);
  assert.match(result.stdout, /Merge conflict with main/);
  assert.equal(git("--git-dir", origin, "rev-parse", "refs/heads/fix/synthetic"), branchHead);
});
