import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("./ci-advisories.sh", import.meta.url));
const posix = { skip: process.platform === "win32" };

function fixture(t) {
  const cwd = mkdtempSync(join(tmpdir(), "ibis-ci-advisories-"));
  t.after(() => rmSync(cwd, { recursive: true, force: true }));
  writeFileSync(join(cwd, "git"), '#!/bin/sh\nprintf "%s\\n" "$*" >> "$CALLS"\n[ "$DIFF_STATUS" = 0 ] || exit "$DIFF_STATUS"\nprintf "%s" "$FILES"\n', { mode: 0o755 });
  writeFileSync(join(cwd, "gh"), `#!/bin/sh
printf '%s\\n' "$*" >> "$CALLS"
[ "$GH_STATUS" = 0 ] || exit "$GH_STATUS"
if [ "$2" = list ]; then printf '%s' "$ISSUE"; fi
while [ "$#" -gt 0 ]; do
  if [ "$1" = --body-file ]; then cat "$2" > "$BODY"; break; fi
  shift
done
`, { mode: 0o755 });
  return (mode, overrides = {}) => {
    const calls = join(cwd, "calls"), body = join(cwd, "body");
    writeFileSync(calls, ""); writeFileSync(body, "");
    const result = spawnSync("bash", [script, mode], {
      cwd, encoding: "utf8", timeout: 5000,
      env: { ...process.env, PATH: `${cwd}${delimiter}${process.env.PATH}`, CALLS: calls, BODY: body, DIFF_STATUS: "0", GH_STATUS: "0", ISSUE: "", FILES: "", EVENT: "pull_request", BASE_SHA: "a".repeat(40), HEAD_SHA: "b".repeat(40), REF: "refs/pull/7/merge", HEAD_REF: "fix", SCHEDULE: "", OUTCOME: "failure", BLOCKING: "false", GITHUB_REPOSITORY: "example/ibis", RUN_URL: "https://github.com/example/ibis/actions/runs/7", ...overrides },
    });
    return { ...result, calls: readFileSync(calls, "utf8"), body: readFileSync(body, "utf8") };
  };
}

test("only dependency changes block PR and queue advisory failures; unreadable ranges enforce the policy", posix, (t) => {
  const run = fixture(t);
  for (const event of ["pull_request", "merge_group"]) {
    assert.equal(run("policy", { EVENT: event }).stdout, "blocking=false\n");
    for (const file of ["Cargo.toml", "Cargo.lock", "deny.toml", "crates/example/Cargo.toml"]) {
      assert.equal(run("policy", { EVENT: event, FILES: file }).stdout, "blocking=true\n");
    }
    const failed = run("policy", { EVENT: event, DIFF_STATUS: "128" });
    assert.equal(failed.stdout, "blocking=true\n");
    assert.match(failed.stderr, /::warning::/);
    assert.equal(run("policy", { EVENT: event, BASE_SHA: "" }).stdout, "blocking=true\n");
    assert.match(run("policy", { EVENT: event }).calls, event === "pull_request" ? /a{40}\.\.\.b{40}/ : /a{40}\.\.b{40}/);
  }
});

test("real diffs include nested manifests and both sides of renames, without counting main's unrelated dependency changes", posix, (t) => {
  const cwd = mkdtempSync(join(tmpdir(), "ibis-ci-advisory-paths-"));
  t.after(() => rmSync(cwd, { recursive: true, force: true }));
  const env = { ...Object.fromEntries(Object.entries(process.env).filter(([key]) => !/^GIT(HUB)?_/.test(key))), GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_NOSYSTEM: "1", GIT_AUTHOR_NAME: "Synthetic", GIT_AUTHOR_EMAIL: "contributor@example.org", GIT_COMMITTER_NAME: "Synthetic", GIT_COMMITTER_EMAIL: "contributor@example.org" };
  const git = (...args) => {
    const result = spawnSync("git", ["-c", "commit.gpgsign=false", ...args], { cwd, env, encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim();
  };
  const commit = (path, contents) => {
    mkdirSync(join(cwd, path, ".."), { recursive: true }); writeFileSync(join(cwd, path), contents);
    git("add", "."); git("commit", "-qm", "synthetic change");
    return git("rev-parse", "HEAD");
  };
  git("init", "-q", "-b", "main");
  let base = commit("Cargo.toml", "synthetic manifest\n");
  const run = (event, head, before = base) => {
    const result = spawnSync("bash", [script, "policy"], { cwd, env: { ...env, EVENT: event, BASE_SHA: before, HEAD_SHA: head }, encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout;
  };
  for (const [path, blocking] of [["README.md", false], ["crates/example/Cargo.toml", true], ["Cargo.lock", true], ["deny.toml", true], ["docs/deny.toml", false], ["Cargo.toml.orig", false]]) {
    const head = commit(path, "synthetic content\n");
    for (const event of ["pull_request", "merge_group"]) assert.equal(run(event, head), `blocking=${blocking}\n`, path);
    base = head;
  }
  git("mv", "Cargo.toml", "old-manifest"); git("commit", "-qm", "synthetic rename away");
  let head = git("rev-parse", "HEAD");
  assert.equal(run("pull_request", head), "blocking=true\n"); base = head;
  git("mv", "old-manifest", "Cargo.toml"); git("commit", "-qm", "synthetic rename back");
  head = git("rev-parse", "HEAD");
  assert.equal(run("pull_request", head), "blocking=true\n"); base = head;
  const main = commit("Cargo.toml", "synthetic main update\n");
  git("switch", "-q", "--detach", base);
  const feature = commit("README.md", "synthetic feature documentation\n");
  assert.equal(run("pull_request", feature, main), "blocking=false\n", "PR compares with the merge base");
});

test("tags, release preparation and weekly checks enforce advisories while ordinary main builds report", posix, (t) => {
  const run = fixture(t);
  for (const env of [{ EVENT: "push", REF: "refs/tags/v0.4.0" }, { EVENT: "release" }, { EVENT: "schedule", SCHEDULE: "43 5 * * 1" }, { EVENT: "workflow_dispatch", REF: "refs/heads/release-please--branches--main" }]) {
    assert.equal(run("policy", env).stdout, "blocking=true\n");
  }
  for (const event of ["push", "workflow_dispatch", "schedule"]) assert.equal(run("policy", { EVENT: event, REF: "refs/heads/main" }).stdout, "blocking=false\n");
});

test("an unrelated advisory failure annotates a warning and succeeds; blocking checks still fail", posix, (t) => {
  const run = fixture(t);
  const warning = run("report");
  assert.equal(warning.status, 0, warning.stderr);
  assert.match(warning.stdout, /::warning::/);
  assert.equal(warning.calls, "");
  assert.equal(run("report", { BLOCKING: "true" }).status, 1);
  assert.equal(run("report", { OUTCOME: "" }).status, 1);
  assert.equal(run("report", { OUTCOME: "success", BLOCKING: "true" }).calls, "");
});

test("the weekly main check opens or updates one issue, and a failed issue write remains visible", posix, (t) => {
  const run = fixture(t);
  const weekly = { EVENT: "schedule", SCHEDULE: "43 5 * * 1", REF: "refs/heads/main", BLOCKING: "true" };
  const opened = run("report", weekly);
  assert.equal(opened.status, 1);
  assert.match(opened.calls, /issue create .*--title RustSec advisory check failing on main/);
  assert.match(opened.body, /https:\/\/github.com\/example\/ibis\/actions\/runs\/7/);
  const updated = run("report", { ...weekly, ISSUE: "42" });
  assert.equal(updated.status, 1);
  assert.match(updated.calls, /issue edit 42 /);
  assert.doesNotMatch(updated.calls, /issue create|issue comment/);
  assert.equal(run("report", { ...weekly, GH_STATUS: "17" }).status, 17);
  for (const env of [{ SCHEDULE: "17 5 * * *" }, { REF: "refs/heads/other" }, { EVENT: "pull_request" }, { OUTCOME: "success" }]) {
    assert.equal(run("report", { ...weekly, ...env }).calls, "", JSON.stringify(env));
  }
});
