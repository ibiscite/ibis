import assert from "node:assert/strict";
import { test } from "node:test";
import { requireMainCi } from "./ci-main.mjs";
const sha = "a".repeat(40), newer = "b".repeat(40);
const pass = (head) => ({ workflow_runs: [{ event: "push", head_branch: "main", head_sha: head, status: "completed", conclusion: "success" }] });
const reader = (result = pass) => (path) => {
  assert.equal(path, `actions/workflows/ci.yml/runs?event=push&branch=main&head_sha=${sha}&per_page=1`);
  return result(sha);
};
test("nightly needs successful push CI for its own commit", () => {
  requireMainCi("example/ibis", sha, reader());
  for (const invalid of [undefined, null, {}, { workflow_runs: [] }, pass(newer), { workflow_runs: [{ ...pass(sha).workflow_runs[0], head_branch: "feature" }] }, ...["failure", "cancelled", "neutral", null].map((conclusion) => ({ workflow_runs: [{ ...pass(sha).workflow_runs[0], conclusion }] })), { workflow_runs: [{ ...pass(sha).workflow_runs[0], event: "pull_request" }] }, { workflow_runs: [{ ...pass(sha).workflow_runs[0], status: "in_progress" }] }]) {
    assert.throws(() => requireMainCi("example/ibis", sha, reader(() => invalid)), /not passed/);
  }
  assert.throws(() => requireMainCi("example/ibis", "main", reader()), /Invalid/);
  assert.throws(() => requireMainCi("example/ibis", sha, () => { throw Error("API failed"); }), /API failed/);
});
