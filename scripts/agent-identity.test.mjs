import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const lint = fileURLToPath(new URL("./pr-lint.mjs", import.meta.url));
const guard = fileURLToPath(new URL("./check-agent-identity.mjs", import.meta.url));
const publicEmail = "diyanko@ibiscite.org", rejected = "maintainer@example.invalid";
const body = "Closes #1181\n\n## What and why\n\nProtect metadata.\n\n## How it works\n\nCheck identities.\n\n## Verification\n\nSynthetic tests.\n";

function fixture(t) {
  const cwd = mkdtempSync(join(tmpdir(), "ibis-identity-"));
  t.after(() => rmSync(cwd, { recursive: true, force: true }));
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith("GIT_") && !key.startsWith("GITHUB_")));
  const git = (...args) => {
    const result = spawnSync("git", ["-c", `core.hooksPath=${join(cwd, "no-hooks")}`, "-c", "commit.gpgsign=false", ...args], { cwd, env, encoding: "utf8" });
    assert.equal(result.status, 0, "synthetic Git setup must succeed");
    return result.stdout.trim();
  };
  git("init", "-q", "--object-format=sha1");
  const commit = (email = publicEmail, message = "Assisted-by: Codex\n\nSigned-off-by: Diyanko Bhowmik <" + publicEmail + ">", name = "Diyanko Bhowmik", author) => {
    git("-c", `user.name=${name}`, "-c", `user.email=${email}`, "commit", "--allow-empty", ...(author ? ["--author", author] : []), "-qm", `fix: synthetic change\n\n${message}`);
    return git("rev-parse", "HEAD");
  };
  const base = commit("base@example.invalid", "Synthetic baseline", "Contributor");
  git("update-ref", "refs/remotes/origin/main", base);
  writeFileSync(join(cwd, "body.md"), body);
  const run = (args = [], extraEnv = {}) => spawnSync(process.execPath, args.length ? [lint, ...args] : [guard], {
    cwd, env: { ...env, ...extraEnv }, encoding: "utf8", timeout: 15_000,
  });
  const event = (value, name) => {
    const path = join(cwd, "event.json"); writeFileSync(path, JSON.stringify(value));
    return { GITHUB_ACTIONS: "true", GITHUB_EVENT_PATH: path, GITHUB_EVENT_NAME: name, GITHUB_SHA: git("rev-parse", "HEAD") };
  };
  return { cwd, env, git, commit, base, run, event };
}

test("PR lint rejects a synthetic unintended maintainer author without printing it", (t) => {
  const f = fixture(t);
  f.commit(rejected);
  const result = f.run(["--title", "fix: protect agent identities", "--branch", "fix/1181-agent-identity", "--body-file", "body.md"]);
  assert.equal(result.status, 1, "identity validation must fail the existing PR lint");
  assert.match(result.stdout, /identity/i);
  assert.doesNotMatch(result.stdout + result.stderr, /maintainer@example\.invalid|fatal:/);
});

test("author, committer, sign-off and co-author are independently checked as raw metadata", (t) => {
  for (const role of ["author", "committer", "signed-off-by", "co-authored-by"]) {
    const f = fixture(t);
    const message = role.endsWith("by") ? `Assisted-by: Codex\n\n${role}: Diyanko Bhowmik <${rejected}>` : "Assisted-by: Codex";
    f.commit(role === "committer" ? rejected : publicEmail, message, "Diyanko Bhowmik", `Diyanko Bhowmik <${role === "author" ? rejected : publicEmail}>`);
    writeFileSync(join(f.cwd, ".mailmap"), `Diyanko Bhowmik <${publicEmail}> <${rejected}>\n`);
    const result = f.run();
    assert.equal(result.status, 1, role);
    assert.match(result.stderr, new RegExp(`maintainer ${role}`));
    assert.doesNotMatch(result.stdout + result.stderr, /maintainer@example\.invalid|fatal:/);
  }
});

test("public project/noreply/bot identities pass without constraining other contributors", (t) => {
  const f = fixture(t);
  for (const email of [publicEmail, "12345+diyanko@users.noreply.github.com", "noreply@github.com"]) f.commit(email);
  f.commit("contributor@example.invalid", "Assisted-by: Codex\n\nCo-authored-by: Other Contributor <other@example.invalid>", "Contributor");
  f.commit("12345+example-bot[bot]@users.noreply.github.com", "Assisted-by: Codex", "example-bot[bot]", `Diyanko <${publicEmail}>`);
  assert.equal(f.run().status, 0);
});

test("manual maintainer metadata is outside the AI repair, folded AI trailers are included", (t) => {
  const manual = fixture(t); manual.commit(rejected, "Manual contribution");
  assert.equal(manual.run().status, 0);
  for (const marker of ["Assisted-by:", "Co-authored-by:\n Codex <agent@example.invalid>"]) {
    const f = fixture(t); f.commit(rejected, `${marker}\nSigned-off-by:\n Diyanko <${rejected}>`);
    assert.equal(f.run().status, 1);
  }
});

test("PR range checks an earlier bad commit even when its exact head is public", (t) => {
  const f = fixture(t); f.commit(rejected); const tip = f.commit();
  const tree = f.git("rev-parse", "HEAD^{tree}");
  const merge = f.git("-c", "user.name=GitHub", "-c", "user.email=noreply@github.com", "commit-tree", tree, "-p", f.base, "-p", tip, "-m", "Synthetic PR merge");
  f.git("update-ref", "HEAD", merge);
  const event = { pull_request: { base: { sha: f.base }, head: { sha: tip }, commits: 2 } };
  const result = f.run([], f.event(event, "pull_request"));
  assert.equal(result.status, 1); assert.match(result.stderr, /identity 1: maintainer author/);
  event.pull_request.commits = 1;
  assert.match(f.run([], f.event(event, "pull_request")).stderr, /incomplete/);
});

test("actual merge-group candidate metadata is checked even without an AI trailer", (t) => {
  const f = fixture(t); const head = f.commit(rejected, "Synthetic merge-group candidate");
  const event = { merge_group: { base_sha: f.base, head_sha: head } };
  assert.match(f.run([], f.event(event, "merge_group")).stderr, /maintainer author/);
  event.merge_group.head_sha = f.base;
  assert.match(f.run([], f.event(event, "merge_group")).stderr, /match the merge-group/);
});

test("valid PR and merge-group candidates accept complete intended ranges", (t) => {
  const f = fixture(t); const tip = f.commit();
  const tree = f.git("rev-parse", "HEAD^{tree}");
  const metadata = ["-c", "user.name=GitHub", "-c", "user.email=noreply@github.com"];
  const newerBase = f.git(...metadata, "commit-tree", tree, "-p", f.base, "-m", "Synthetic newer main");
  const merge = f.git(...metadata, "commit-tree", tree, "-p", newerBase, "-p", tip, "-m", "Synthetic PR merge");
  f.git("update-ref", "HEAD", merge);
  assert.equal(f.run([], f.event({ pull_request: { base: { sha: f.base }, head: { sha: tip }, commits: 1 } }, "pull_request")).status, 0);
  assert.equal(f.run([], f.event({ merge_group: { base_sha: newerBase, head_sha: merge } }, "merge_group")).status, 0);
});

test("Git-valid spaced trailer separators cannot hide candidate identities", (t) => {
  for (const key of ["Signed-off-by", "Co-authored-by"]) {
    const f = fixture(t); const head = f.commit(publicEmail, `${key} : Diyanko <${rejected}>`);
    assert.equal(f.run([], f.event({ merge_group: { base_sha: f.base, head_sha: head } }, "merge_group")).status, 1);
  }
});

test("co-author disclosures do not depend on a fixed AI product-name list", (t) => {
  for (const name of ["Copilot", "Cursor", "Example Future Tool"]) {
    const f = fixture(t); f.commit(rejected, `Co-authored-by: ${name} <agent@example.invalid>`);
    assert.equal(f.run().status, 1);
  }
});

test("missing, mismatched, shallow and empty intended inputs fail without sensitive diagnostics", (t) => {
  const f = fixture(t); const head = f.commit();
  for (const [event, name, pattern] of [
    [{}, "pull_request", /range is missing/],
    [{ merge_group: { base_sha: "a".repeat(40), head_sha: head } }, "merge_group", /unavailable/],
    [{ merge_group: { base_sha: f.base, head_sha: "short" } }, "merge_group", /full base\/head/],
    [{ merge_group: { base_sha: head, head_sha: head } }, "merge_group", /empty or incomplete/],
  ]) {
    const result = f.run([], f.event(event, name));
    assert.equal(result.status, 1); assert.match(result.stderr, pattern);
    assert.doesNotMatch(result.stdout + result.stderr, /fatal:|example\.invalid|ibis-identity-/);
  }
  const event = f.event({ merge_group: { base_sha: f.base, head_sha: head } }, "merge_group");
  assert.match(f.run([], { ...event, GITHUB_SHA: f.base }).stderr, /checkout does not match/);
  assert.match(f.run([], { ...event, GIT_SHALLOW_FILE: "synthetic-unavailable" }).stderr, /source overrides/);
  writeFileSync(join(f.cwd, ".git", "shallow"), head + "\n");
  assert.match(f.run([], event).stderr, /shallow inputs/);
});
