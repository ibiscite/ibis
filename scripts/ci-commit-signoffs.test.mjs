import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("./ci-commit-signoffs.sh", import.meta.url));
const posix = { skip: process.platform === "win32" };
const signed = "fix: synthetic change\n\nSigned-off-by: Example Contributor <contributor@example.org>";

function fixture(t, identity = {}) {
  const cwd = mkdtempSync(join(tmpdir(), "ibis-ci-signoffs-"));
  t.after(() => rmSync(cwd, { recursive: true, force: true }));
  const env = { ...Object.fromEntries(Object.entries(process.env).filter(([key]) => !/^GIT(HUB)?_/.test(key))), GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_NOSYSTEM: "1", GIT_AUTHOR_NAME: "Example Contributor", GIT_AUTHOR_EMAIL: "contributor@example.org", GIT_COMMITTER_NAME: "Example Contributor", GIT_COMMITTER_EMAIL: "contributor@example.org", ...identity };
  const git = (...args) => {
    const result = spawnSync("git", ["-c", "commit.gpgsign=false", ...args], { cwd, env, encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim();
  };
  git("init", "-q", "-b", "main");
  const commit = (message = signed) => { git("commit", "--allow-empty", "-qm", message); return git("rev-parse", "HEAD"); };
  const base = commit();
  const run = (head = git("rev-parse", "HEAD"), baseSha = base) => spawnSync("bash", [script], { cwd, env: { ...env, BASE_SHA: baseSha, HEAD_SHA: head }, encoding: "utf8", timeout: 5000 });
  return { git, commit, base, run };
}

test("ordinary non-merge commits need a sign-off, including an unsigned earlier commit and a Merge subject", posix, (t) => {
  const f = fixture(t);
  assert.equal(f.run(f.base, "").status, 0, "a signed root commit needs no range");
  f.commit(); assert.equal(f.run().status, 0);
  f.commit("Merge a synthetic feature");
  f.commit();
  const result = f.run();
  assert.equal(result.status, 1);
  assert.match(result.stdout, /::error::Commit [a-f0-9]+ needs a Signed-off-by trailer/);
  assert.equal(f.run(undefined, "").status, 0, "without a range the signed head is checked");
  assert.equal(f.run("missing").status, 1);
  assert.notEqual(f.run(undefined, "a".repeat(40)).status, 0);
});

const releaseIdentity = {
  GIT_AUTHOR_NAME: "ibis-release[bot]",
  GIT_AUTHOR_EMAIL: "347725936+ibis-release[bot]@users.noreply.github.com",
  GIT_COMMITTER_NAME: "GitHub",
  GIT_COMMITTER_EMAIL: "noreply@github.com",
};

for (const { name, identity, status } of [
  { name: "a release-app commit made by GitHub needs no sign-off", identity: releaseIdentity, status: 0 },
  { name: "the release-app author with a different committer name still needs a sign-off", identity: { ...releaseIdentity, GIT_COMMITTER_NAME: "Example Contributor" }, status: 1 },
  { name: "the release-app author with a different committer email still needs a sign-off", identity: { ...releaseIdentity, GIT_COMMITTER_EMAIL: "contributor@example.org" }, status: 1 },
  { name: "a different author email committed by GitHub still needs a sign-off", identity: { ...releaseIdentity, GIT_AUTHOR_EMAIL: "contributor@example.org" }, status: 1 },
  { name: "an ordinary commit without a sign-off fails", identity: {}, status: 1 },
]) {
  test(name, posix, (t) => {
    const f = fixture(t, identity);
    f.commit("chore: synthetic unsigned change");
    const result = f.run();
    assert.equal(result.status, status, result.stdout + result.stderr);
    if (status === 1) assert.match(result.stdout, /::error::Commit [a-f0-9]+ needs a Signed-off-by trailer/);
  });
}

test("a git merge syncing main needs no sign-off, but its unsigned source commits still fail", posix, (t) => {
  const f = fixture(t);
  f.git("switch", "-qc", "feature"); const feature = f.commit(signed.replace("synthetic change", "synthetic feature"));
  f.git("switch", "-q", "main"); f.commit(signed.replace("synthetic change", "synthetic main update"));
  f.git("switch", "-q", "feature"); f.git("merge", "--no-ff", "--no-edit", "main");
  assert.equal(f.git("show", "-s", "--format=%P", "HEAD").split(" ").length, 2);
  assert.match(f.git("show", "-s", "--format=%s", "HEAD"), /^Merge/);
  assert.equal(f.run().status, 0);
  const tree = f.git("rev-parse", "HEAD^{tree}");
  const custom = f.git("commit-tree", tree, "-p", feature, "-p", f.base, "-m", "Combine synthetic branches");
  assert.equal(f.run(custom).status, 1, "two parents alone are insufficient");
  const bad = f.commit("fix: unsigned synthetic change");
  const merged = f.git("commit-tree", tree, "-p", bad, "-p", f.base, "-m", "Merge synthetic branches");
  assert.equal(f.run(merged).status, 1, "the exemption never hides an unsigned parent");
  const third = f.git("commit-tree", tree, "-p", feature, "-p", bad, "-p", f.base, "-m", "Merge three branches");
  assert.equal(f.run(third, bad).status, 1, "only two-parent merges are exempt");
});
