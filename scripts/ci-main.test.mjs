import assert from "node:assert/strict";
import { test } from "node:test";
import { requireMainCi } from "./ci-main.mjs";
const sha = "a".repeat(40), newer = "b".repeat(40);
const pass = (head) => ({ workflow_runs: [{ event: "push", head_branch: "main", head_sha: head, status: "completed", conclusion: "success" }] });
const reader = (main = sha, result = pass, final = main) => {
  let refs = 0;
  return (path) => path === "git/ref/heads/main" ? { object: { type: "commit", sha: ++refs === 1 ? main : final } } : result(path.includes(newer) ? newer : sha);
};
test("nightly needs successful push CI for its candidate and current main", () => {
  requireMainCi("example/ibis", sha, reader());
  requireMainCi("example/ibis", sha, reader(newer));
  for (const invalid of [undefined, null, {}, { workflow_runs: [] }, pass(newer), { workflow_runs: [{ ...pass(sha).workflow_runs[0], head_branch: "feature" }] }, ...["failure", "cancelled", "neutral", null].map((conclusion) => ({ workflow_runs: [{ ...pass(sha).workflow_runs[0], conclusion }] })), { workflow_runs: [{ ...pass(sha).workflow_runs[0], event: "pull_request" }] }, { workflow_runs: [{ ...pass(sha).workflow_runs[0], status: "in_progress" }] }]) {
    assert.throws(() => requireMainCi("example/ibis", sha, reader(sha, () => invalid)));
  }
  assert.throws(() => requireMainCi("example/ibis", sha, reader(newer, (head) => head === sha ? pass(head) : { workflow_runs: [] })), /not passed/);
  assert.throws(() => requireMainCi("example/ibis", sha, reader(sha, pass, newer)), /changed/);
  assert.throws(() => requireMainCi("example/ibis", sha, () => { throw Error("API failed"); }), /API failed/);
});
